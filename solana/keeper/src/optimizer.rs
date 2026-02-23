use serde::{Deserialize, Serialize};
use solana_sdk::hash::hashv;
use std::{
    env,
    error::Error,
    f64::consts::PI,
    fmt, fs,
    path::{Path, PathBuf},
};

const PARAM_DIM: usize = 7;
const WEIGHT_DIM: usize = 4;
const SAFETY_TOLERANCE: f64 = 1e-9;
const DEFAULT_CHECKPOINT_FILE: &str = ".state/microstable/optimizer_checkpoint.json";

/// Full optimizer parameter vector (θ) used by the keeper.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ParamVector {
    /// Collateral weights for 4 vaults (projected to simplex by safety projection).
    pub weights: [f64; WEIGHT_DIM],
    /// Target collateral ratio.
    pub target_cr: f64,
    /// Mint fee rate.
    pub mint_fee: f64,
    /// Redeem fee rate.
    pub redeem_fee: f64,
}

impl Default for ParamVector {
    fn default() -> Self {
        Self {
            weights: [0.25; WEIGHT_DIM],
            target_cr: 1.20,
            mint_fee: 0.001,
            redeem_fee: 0.001,
        }
    }
}

impl ParamVector {
    pub fn zeros() -> Self {
        Self {
            weights: [0.0; WEIGHT_DIM],
            target_cr: 0.0,
            mint_fee: 0.0,
            redeem_fee: 0.0,
        }
    }

    pub fn infinities() -> Self {
        Self {
            weights: [f64::INFINITY; WEIGHT_DIM],
            target_cr: f64::INFINITY,
            mint_fee: f64::INFINITY,
            redeem_fee: f64::INFINITY,
        }
    }

    pub fn is_finite(&self) -> bool {
        self.weights.iter().all(|w| w.is_finite())
            && self.target_cr.is_finite()
            && self.mint_fee.is_finite()
            && self.redeem_fee.is_finite()
    }

    pub fn l2_norm(&self) -> f64 {
        let flat = self.flatten();
        flat.iter().map(|x| x * x).sum::<f64>().sqrt()
    }

    pub fn flatten(&self) -> [f64; PARAM_DIM] {
        [
            self.weights[0],
            self.weights[1],
            self.weights[2],
            self.weights[3],
            self.target_cr,
            self.mint_fee,
            self.redeem_fee,
        ]
    }

    pub fn from_flat(flat: [f64; PARAM_DIM]) -> Self {
        Self {
            weights: [flat[0], flat[1], flat[2], flat[3]],
            target_cr: flat[4],
            mint_fee: flat[5],
            redeem_fee: flat[6],
        }
    }

    pub fn add(&self, other: &Self) -> Self {
        let a = self.flatten();
        let b = other.flatten();
        let mut out = [0.0; PARAM_DIM];
        for i in 0..PARAM_DIM {
            out[i] = a[i] + b[i];
        }
        Self::from_flat(out)
    }

    pub fn scale(&self, s: f64) -> Self {
        let a = self.flatten();
        let mut out = [0.0; PARAM_DIM];
        for i in 0..PARAM_DIM {
            out[i] = a[i] * s;
        }
        Self::from_flat(out)
    }

    pub fn abs_diff_leq(&self, other: &Self, max_delta: &Self, tol: f64) -> bool {
        let a = self.flatten();
        let b = other.flatten();
        let d = max_delta.flatten();

        for i in 0..PARAM_DIM {
            if (a[i] - b[i]).abs() > d[i] + tol {
                return false;
            }
        }

        true
    }
}

/// Runtime protocol snapshot used for loss evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolSnapshot {
    /// Observed market price for the protocol token.
    pub peg_price: f64,
    /// Current collateral ratio.
    pub collateral_ratio: f64,
    /// NAV history window used to compute volatility term.
    pub nav_history: Vec<f64>,
    /// Current collateral weights.
    pub current_weights: [f64; WEIGHT_DIM],
    /// Previous epoch collateral weights.
    pub previous_weights: [f64; WEIGHT_DIM],
    /// Per-oracle quality score in [0,1].
    pub oracle_quality_scores: [f64; WEIGHT_DIM],
    /// Current target CR parameter.
    pub target_cr: f64,
    /// Current mint fee parameter.
    pub mint_fee: f64,
    /// Current redeem fee parameter.
    pub redeem_fee: f64,
    /// Optional custom loss configuration used by optimize_step.
    pub loss_function: Option<LossFunction>,
}

impl Default for ProtocolSnapshot {
    fn default() -> Self {
        Self {
            peg_price: 1.0,
            collateral_ratio: 1.2,
            nav_history: vec![1.0, 1.0],
            current_weights: [0.25; WEIGHT_DIM],
            previous_weights: [0.25; WEIGHT_DIM],
            oracle_quality_scores: [1.0; WEIGHT_DIM],
            target_cr: 1.2,
            mint_fee: 0.001,
            redeem_fee: 0.001,
            loss_function: None,
        }
    }
}

impl ProtocolSnapshot {
    pub fn validate(&self) -> Result<(), OptimizerError> {
        if !self.peg_price.is_finite() {
            return Err(OptimizerError::InvalidInput(
                "peg_price must be finite".to_string(),
            ));
        }
        if !self.collateral_ratio.is_finite() {
            return Err(OptimizerError::InvalidInput(
                "collateral_ratio must be finite".to_string(),
            ));
        }
        if !self.target_cr.is_finite() || !self.mint_fee.is_finite() || !self.redeem_fee.is_finite()
        {
            return Err(OptimizerError::InvalidInput(
                "target_cr/mint_fee/redeem_fee must be finite".to_string(),
            ));
        }

        if self.nav_history.iter().any(|x| !x.is_finite()) {
            return Err(OptimizerError::InvalidInput(
                "nav_history contains non-finite value".to_string(),
            ));
        }

        if self
            .current_weights
            .iter()
            .chain(self.previous_weights.iter())
            .any(|x| !x.is_finite())
        {
            return Err(OptimizerError::InvalidInput(
                "current_weights/previous_weights contain non-finite value".to_string(),
            ));
        }

        if self.oracle_quality_scores.iter().any(|x| !x.is_finite()) {
            return Err(OptimizerError::InvalidInput(
                "oracle_quality_scores contain non-finite value".to_string(),
            ));
        }

        Ok(())
    }

    pub fn with_params(&self, params: &ParamVector) -> Self {
        let mut cloned = self.clone();
        cloned.current_weights = params.weights;
        cloned.target_cr = params.target_cr;
        cloned.mint_fee = params.mint_fee;
        cloned.redeem_fee = params.redeem_fee;
        cloned
    }
}

/// Per-term scalar loss values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct LossTerms {
    pub price: f64,
    pub collateral_ratio: f64,
    pub volatility: f64,
    pub turnover: f64,
    pub concentration: f64,
    pub oracle_quality: f64,
}

/// Per-term gradients in θ-space.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LossGradients {
    pub price: ParamVector,
    pub collateral_ratio: ParamVector,
    pub volatility: ParamVector,
    pub turnover: ParamVector,
    pub concentration: ParamVector,
    pub oracle_quality: ParamVector,
}

impl Default for LossGradients {
    fn default() -> Self {
        Self {
            price: ParamVector::zeros(),
            collateral_ratio: ParamVector::zeros(),
            volatility: ParamVector::zeros(),
            turnover: ParamVector::zeros(),
            concentration: ParamVector::zeros(),
            oracle_quality: ParamVector::zeros(),
        }
    }
}

impl LossGradients {
    pub fn total(&self) -> ParamVector {
        self.price
            .add(&self.collateral_ratio)
            .add(&self.volatility)
            .add(&self.turnover)
            .add(&self.concentration)
            .add(&self.oracle_quality)
    }
}

/// Output of a loss evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LossResult {
    pub total_loss: f64,
    pub terms: LossTerms,
    pub gradients: LossGradients,
    pub total_gradient: ParamVector,
}

/// 6-term loss function from the whitepaper with configurable λ coefficients.
///
/// We preserve the whitepaper terms and use a differentiable keeper parameterization:
/// - CR shortfall uses `max(0, target_cr - collateral_ratio)^2`.
/// - Peg term is coupled to fee skew (`mint_fee - redeem_fee`) so fee gradients are analytical.
/// - Concentration term is centered around equal-weight allocation (minimum at 25% each).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LossFunction {
    pub lambda_price: f64,
    pub lambda_cr: f64,
    pub lambda_vol: f64,
    pub lambda_turn: f64,
    pub lambda_conc: f64,
    pub lambda_oracle: f64,
}

impl Default for LossFunction {
    fn default() -> Self {
        Self {
            lambda_price: 1.0,
            lambda_cr: 1.0,
            lambda_vol: 1.0,
            lambda_turn: 1.0,
            lambda_conc: 1.0,
            lambda_oracle: 1.0,
        }
    }
}

impl LossFunction {
    pub fn compute(&self, state: &ProtocolSnapshot) -> Result<LossResult, OptimizerError> {
        state.validate()?;
        self.validate_lambdas()?;

        let fee_skew = state.mint_fee - state.redeem_fee;
        let peg_error = state.peg_price - 1.0 + fee_skew;
        let price_term = self.lambda_price * peg_error * peg_error;

        let cr_shortfall = (state.target_cr - state.collateral_ratio).max(0.0);
        let cr_term = self.lambda_cr * cr_shortfall * cr_shortfall;

        let nav_var = variance_of_diffs(&state.nav_history)?;
        let vol_term = self.lambda_vol * nav_var;

        let turnover_l1 = l1_distance(&state.current_weights, &state.previous_weights);
        let turnover_term = self.lambda_turn * turnover_l1;

        let equal_w = 1.0 / WEIGHT_DIM as f64;
        let centered_hhi = state
            .current_weights
            .iter()
            .map(|w| {
                let d = w - equal_w;
                d * d
            })
            .sum::<f64>();
        let conc_term = self.lambda_conc * centered_hhi;

        let q_t = dot(&state.current_weights, &state.oracle_quality_scores);
        let oracle_err = 1.0 - q_t;
        let oracle_term = self.lambda_oracle * oracle_err * oracle_err;

        let mut gradients = LossGradients::default();

        // ∂L_price/∂mint_fee = 2 λ_p e,  ∂L_price/∂redeem_fee = -2 λ_p e
        let d_price = 2.0 * self.lambda_price * peg_error;
        gradients.price.mint_fee = d_price;
        gradients.price.redeem_fee = -d_price;

        // ∂L_cr/∂target_cr = 2 λ_cr shortfall if active.
        if cr_shortfall > 0.0 {
            gradients.collateral_ratio.target_cr = 2.0 * self.lambda_cr * cr_shortfall;
        }

        // Volatility term currently treated as exogenous to θ for one-step update.

        // ∂L_turn/∂w_i = λ_turn * sign(w_i - w_{i,prev})
        for i in 0..WEIGHT_DIM {
            let delta = state.current_weights[i] - state.previous_weights[i];
            gradients.turnover.weights[i] = self.lambda_turn * signed_unit(delta);
        }

        // Centered concentration: ∂L_conc/∂w_i = 2 λ_conc (w_i - 1/N)
        for i in 0..WEIGHT_DIM {
            gradients.concentration.weights[i] =
                2.0 * self.lambda_conc * (state.current_weights[i] - equal_w);
        }

        // q_t = Σ w_i q_i  -> ∂q_t/∂w_i = q_i
        // L_orc = λ_orc (1 - q_t)^2
        // ∂L_orc/∂w_i = -2 λ_orc (1 - q_t) q_i
        for i in 0..WEIGHT_DIM {
            gradients.oracle_quality.weights[i] =
                -2.0 * self.lambda_oracle * oracle_err * state.oracle_quality_scores[i];
        }

        let terms = LossTerms {
            price: ensure_finite(price_term, "price term")?,
            collateral_ratio: ensure_finite(cr_term, "collateral ratio term")?,
            volatility: ensure_finite(vol_term, "volatility term")?,
            turnover: ensure_finite(turnover_term, "turnover term")?,
            concentration: ensure_finite(conc_term, "concentration term")?,
            oracle_quality: ensure_finite(oracle_term, "oracle quality term")?,
        };

        let total_loss = terms.price
            + terms.collateral_ratio
            + terms.volatility
            + terms.turnover
            + terms.concentration
            + terms.oracle_quality;

        let total_gradient = gradients.total();
        if !total_gradient.is_finite() {
            return Err(OptimizerError::NonFinite(
                "total gradient contains non-finite value".to_string(),
            ));
        }

        Ok(LossResult {
            total_loss: ensure_finite(total_loss, "total loss")?,
            terms,
            gradients,
            total_gradient,
        })
    }

    fn validate_lambdas(&self) -> Result<(), OptimizerError> {
        let lambdas = [
            self.lambda_price,
            self.lambda_cr,
            self.lambda_vol,
            self.lambda_turn,
            self.lambda_conc,
            self.lambda_oracle,
        ];

        if lambdas.iter().any(|l| !l.is_finite() || *l < 0.0) {
            return Err(OptimizerError::InvalidInput(
                "all lambda weights must be finite and non-negative".to_string(),
            ));
        }

        Ok(())
    }
}

/// Adam moment buffers and timestep.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdamState {
    pub m: ParamVector,
    pub v: ParamVector,
    pub t: u64,
}

impl Default for AdamState {
    fn default() -> Self {
        Self {
            m: ParamVector::zeros(),
            v: ParamVector::zeros(),
            t: 0,
        }
    }
}

/// Adam optimizer with gradient clipping and warmup+cosine LR schedule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdamOptimizer {
    pub learning_rate: f64,
    pub beta1: f64,
    pub beta2: f64,
    pub epsilon: f64,
    pub max_grad_norm: f64,
    pub warmup_steps: u64,
    pub decay_steps: u64,
    pub min_learning_rate: f64,
    pub state: AdamState,
}

impl Default for AdamOptimizer {
    fn default() -> Self {
        Self {
            learning_rate: 0.01,
            beta1: 0.9,
            beta2: 0.999,
            epsilon: 1e-8,
            max_grad_norm: 1.0,
            warmup_steps: 25,
            decay_steps: 5_000,
            min_learning_rate: 1e-4,
            state: AdamState::default(),
        }
    }
}

impl AdamOptimizer {
    pub fn learning_rate_for_step(&self, step: u64) -> f64 {
        if step == 0 {
            return 0.0;
        }

        if self.warmup_steps > 0 && step <= self.warmup_steps {
            return self.learning_rate * (step as f64 / self.warmup_steps as f64);
        }

        let decay_progress = if self.decay_steps == 0 {
            1.0
        } else {
            ((step.saturating_sub(self.warmup_steps)) as f64 / self.decay_steps as f64)
                .clamp(0.0, 1.0)
        };

        let cosine = 0.5 * (1.0 + (PI * decay_progress).cos());
        self.min_learning_rate + (self.learning_rate - self.min_learning_rate) * cosine
    }

    pub fn clip_gradients(&self, gradients: &ParamVector) -> ParamVector {
        if !gradients.is_finite() {
            return *gradients;
        }

        if !self.max_grad_norm.is_finite() || self.max_grad_norm <= 0.0 {
            return *gradients;
        }

        let norm = gradients.l2_norm();
        if norm <= self.max_grad_norm || norm == 0.0 {
            *gradients
        } else {
            gradients.scale(self.max_grad_norm / norm)
        }
    }

    /// Performs one Adam update step and returns the updated parameter vector.
    pub fn step(&mut self, params: &ParamVector, gradients: &ParamVector) -> ParamVector {
        if !params.is_finite() || !gradients.is_finite() {
            return *params;
        }

        let g = self.clip_gradients(gradients);

        self.state.t = self.state.t.saturating_add(1);
        let t = self.state.t as f64;
        let lr = self.learning_rate_for_step(self.state.t);
        if !lr.is_finite() || lr < 0.0 {
            return *params;
        }

        let beta1 = if self.beta1.is_finite() {
            self.beta1.clamp(0.0, 1.0)
        } else {
            0.9
        };
        let beta2 = if self.beta2.is_finite() {
            self.beta2.clamp(0.0, 1.0)
        } else {
            0.999
        };
        let eps = if self.epsilon.is_finite() && self.epsilon > 0.0 {
            self.epsilon
        } else {
            1e-8
        };

        let g_flat = g.flatten();
        let m_flat = self.state.m.flatten();
        let v_flat = self.state.v.flatten();

        let mut new_m = [0.0; PARAM_DIM];
        let mut new_v = [0.0; PARAM_DIM];
        let mut update = [0.0; PARAM_DIM];

        let m_bias_correction = 1.0 - beta1.powf(t);
        let v_bias_correction = 1.0 - beta2.powf(t);

        for i in 0..PARAM_DIM {
            new_m[i] = beta1 * m_flat[i] + (1.0 - beta1) * g_flat[i];
            new_v[i] = beta2 * v_flat[i] + (1.0 - beta2) * g_flat[i] * g_flat[i];

            let m_hat = if m_bias_correction.abs() <= f64::EPSILON {
                new_m[i]
            } else {
                new_m[i] / m_bias_correction
            };
            let v_hat = if v_bias_correction.abs() <= f64::EPSILON {
                new_v[i].abs()
            } else {
                (new_v[i] / v_bias_correction).abs()
            };

            let denom = v_hat.sqrt() + eps;
            let candidate_update = lr * m_hat / denom;
            update[i] = if candidate_update.is_finite() {
                candidate_update
            } else {
                0.0
            };
        }

        self.state.m = ParamVector::from_flat(new_m);
        self.state.v = ParamVector::from_flat(new_v);

        let p = params.flatten();
        let mut next = [0.0; PARAM_DIM];
        for i in 0..PARAM_DIM {
            next[i] = p[i] - update[i];
        }

        ParamVector::from_flat(next)
    }
}

/// Bounds that define the optimizer safety set Π_Ω.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafetyBounds {
    pub weight_caps: [f64; WEIGHT_DIM],
    pub cr_min: f64,
    pub cr_max: f64,
    pub fee_min: f64,
    pub fee_max: f64,
    pub max_delta: ParamVector,
    pub reference_params: Option<ParamVector>,
}

impl Default for SafetyBounds {
    fn default() -> Self {
        Self {
            weight_caps: [1.0; WEIGHT_DIM],
            cr_min: 1.0,
            cr_max: 2.0,
            fee_min: 0.0,
            fee_max: 0.05,
            max_delta: ParamVector::infinities(),
            reference_params: None,
        }
    }
}

impl SafetyBounds {
    #[cfg(test)]
    pub fn with_reference(mut self, reference: ParamVector) -> Self {
        self.reference_params = Some(reference);
        self
    }

    pub fn validate(&self) -> Result<(), OptimizerError> {
        if self
            .weight_caps
            .iter()
            .any(|c| !c.is_finite() || *c < 0.0 || *c > 1.0)
        {
            return Err(OptimizerError::InvalidInput(
                "weight caps must be finite and within [0,1]".to_string(),
            ));
        }
        if self.weight_caps.iter().sum::<f64>() + SAFETY_TOLERANCE < 1.0 {
            return Err(OptimizerError::InvalidInput(
                "sum(weight_caps) must be >= 1.0 for feasible capped simplex".to_string(),
            ));
        }
        if !self.cr_min.is_finite() || !self.cr_max.is_finite() || self.cr_min > self.cr_max {
            return Err(OptimizerError::InvalidInput(
                "invalid collateral ratio bounds".to_string(),
            ));
        }
        if !self.fee_min.is_finite() || !self.fee_max.is_finite() || self.fee_min > self.fee_max {
            return Err(OptimizerError::InvalidInput(
                "invalid fee bounds".to_string(),
            ));
        }
        let deltas = self.max_delta.flatten();
        if deltas
            .iter()
            .any(|d| d.is_nan() || (!d.is_infinite() && *d < 0.0))
        {
            return Err(OptimizerError::InvalidInput(
                "max_delta must be non-negative (or +inf) and not NaN".to_string(),
            ));
        }

        Ok(())
    }
}

/// Euclidean projection onto Π_Ω safety set:
/// - capped simplex on weights
/// - CR and fee box constraints
/// - optional per-epoch delta bounds relative to `reference_params`
pub fn project_to_safety_set(params: &ParamVector, bounds: &SafetyBounds) -> ParamVector {
    let mut projected = *params;

    let mut lower: [f64; WEIGHT_DIM] = [0.0; WEIGHT_DIM];
    let mut upper: [f64; WEIGHT_DIM] = [0.0; WEIGHT_DIM];
    for i in 0..WEIGHT_DIM {
        upper[i] = bounds.weight_caps[i].clamp(0.0, 1.0);
    }

    if let Some(reference) = bounds.reference_params {
        for i in 0..WEIGHT_DIM {
            let d = bounds.max_delta.weights[i].max(0.0);
            lower[i] = lower[i].max(reference.weights[i] - d);
            upper[i] = upper[i].min(reference.weights[i] + d);
        }

        projected.target_cr = clamp_with_optional_delta(
            projected.target_cr,
            reference.target_cr,
            bounds.max_delta.target_cr,
            bounds.cr_min,
            bounds.cr_max,
        );
        projected.mint_fee = clamp_with_optional_delta(
            projected.mint_fee,
            reference.mint_fee,
            bounds.max_delta.mint_fee,
            bounds.fee_min,
            bounds.fee_max,
        );
        projected.redeem_fee = clamp_with_optional_delta(
            projected.redeem_fee,
            reference.redeem_fee,
            bounds.max_delta.redeem_fee,
            bounds.fee_min,
            bounds.fee_max,
        );
    } else {
        projected.target_cr = projected.target_cr.clamp(bounds.cr_min, bounds.cr_max);
        projected.mint_fee = projected.mint_fee.clamp(bounds.fee_min, bounds.fee_max);
        projected.redeem_fee = projected.redeem_fee.clamp(bounds.fee_min, bounds.fee_max);
    }

    for i in 0..WEIGHT_DIM {
        lower[i] = lower[i].clamp(0.0, 1.0);
        upper[i] = upper[i].clamp(0.0, 1.0);
        if lower[i] > upper[i] {
            std::mem::swap(&mut lower[i], &mut upper[i]);
        }
    }

    projected.weights = project_onto_bounded_simplex(projected.weights, lower, upper);
    projected
}

/// Validate that a parameter vector sits inside Π_Ω.
pub fn validate_safety_set(
    params: &ParamVector,
    bounds: &SafetyBounds,
) -> Result<(), OptimizerError> {
    if !params.is_finite() {
        return Err(OptimizerError::NonFinite(
            "params contain non-finite values".to_string(),
        ));
    }

    let sum_w: f64 = params.weights.iter().sum();
    if (sum_w - 1.0).abs() > 1e-6 {
        return Err(OptimizerError::SafetyViolation(format!(
            "weight simplex violated: sum={sum_w}"
        )));
    }

    for i in 0..WEIGHT_DIM {
        if params.weights[i] < -1e-8 {
            return Err(OptimizerError::SafetyViolation(format!(
                "weight[{i}] negative: {}",
                params.weights[i]
            )));
        }
        if params.weights[i] > bounds.weight_caps[i] + 1e-8 {
            return Err(OptimizerError::SafetyViolation(format!(
                "weight[{i}] exceeds cap: {} > {}",
                params.weights[i], bounds.weight_caps[i]
            )));
        }
    }

    if params.target_cr < bounds.cr_min - 1e-8 || params.target_cr > bounds.cr_max + 1e-8 {
        return Err(OptimizerError::SafetyViolation(
            "target_cr outside bounds".to_string(),
        ));
    }
    if params.mint_fee < bounds.fee_min - 1e-8 || params.mint_fee > bounds.fee_max + 1e-8 {
        return Err(OptimizerError::SafetyViolation(
            "mint_fee outside bounds".to_string(),
        ));
    }
    if params.redeem_fee < bounds.fee_min - 1e-8 || params.redeem_fee > bounds.fee_max + 1e-8 {
        return Err(OptimizerError::SafetyViolation(
            "redeem_fee outside bounds".to_string(),
        ));
    }

    if let Some(reference) = bounds.reference_params {
        if !params.abs_diff_leq(&reference, &bounds.max_delta, 1e-8) {
            return Err(OptimizerError::SafetyViolation(
                "delta bounds violated".to_string(),
            ));
        }
    }

    Ok(())
}

/// Persistent optimizer checkpoint used for CB-4 rollback.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OptimizerCheckpoint {
    pub params: ParamVector,
    pub adam_state: AdamState,
    pub tick: u64,
    pub loss: f64,
}

impl OptimizerCheckpoint {
    pub fn save_default(&self) -> Result<(), OptimizerError> {
        self.save_to_path(checkpoint_path())
    }

    pub fn save_to_path<P: AsRef<Path>>(&self, path: P) -> Result<(), OptimizerError> {
        let path_ref = path.as_ref();
        if let Some(parent) = path_ref.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let checkpoint_json = serde_json::to_vec_pretty(self)?;
        let envelope = OptimizerCheckpointEnvelope {
            version: OPTIMIZER_CHECKPOINT_VERSION,
            integrity_tag: checkpoint_integrity_tag(&checkpoint_json)?,
            checkpoint: self.clone(),
        };

        let data = serde_json::to_vec_pretty(&envelope)?;
        let tmp_path = path_ref.with_extension("json.tmp");
        fs::write(&tmp_path, data)?;
        fs::rename(&tmp_path, path_ref)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path_ref, fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self, OptimizerError> {
        let path_ref = path.as_ref();
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let metadata = fs::metadata(path_ref)?;
            let mode = metadata.permissions().mode() & 0o777;
            if mode & 0o077 != 0 {
                return Err(OptimizerError::Io(format!(
                    "checkpoint file has insecure permissions {:o}: {}",
                    mode,
                    path_ref.display()
                )));
            }
            let owner_uid = metadata.uid();
            let effective_uid = unsafe { libc::geteuid() as u32 };
            if owner_uid != effective_uid {
                return Err(OptimizerError::Io(format!(
                    "checkpoint file owner mismatch for {} (owner_uid={}, effective_uid={})",
                    path_ref.display(),
                    owner_uid,
                    effective_uid
                )));
            }
        }

        let data = fs::read(path_ref)?;

        let envelope = serde_json::from_slice::<OptimizerCheckpointEnvelope>(&data)?;
        if envelope.version != OPTIMIZER_CHECKPOINT_VERSION {
            return Err(OptimizerError::Serialization(format!(
                "unsupported checkpoint envelope version: {}",
                envelope.version
            )));
        }

        let checkpoint_json = serde_json::to_vec_pretty(&envelope.checkpoint)?;
        let expected_tag = checkpoint_integrity_tag(&checkpoint_json)?;
        if envelope.integrity_tag.trim().to_ascii_lowercase() != expected_tag {
            return Err(OptimizerError::Serialization(
                "optimizer checkpoint integrity verification failed".to_string(),
            ));
        }

        Ok(envelope.checkpoint)
    }
}

const OPTIMIZER_CHECKPOINT_VERSION: u8 = 2;
const STATE_HMAC_ENV_KEY: &str = "MICROSTABLE_STATE_HMAC_KEY";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct OptimizerCheckpointEnvelope {
    version: u8,
    integrity_tag: String,
    checkpoint: OptimizerCheckpoint,
}

fn checkpoint_integrity_tag(payload: &[u8]) -> Result<String, OptimizerError> {
    let key = state_hmac_key()?;
    let digest = hashv(&[
        b"microstable:optimizer-checkpoint:v2",
        key.as_bytes(),
        payload,
        key.as_bytes(),
    ])
    .to_bytes();
    Ok(digest.iter().map(|b| format!("{:02x}", b)).collect())
}

fn state_hmac_key() -> Result<String, OptimizerError> {
    let key = env::var(STATE_HMAC_ENV_KEY)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            OptimizerError::Io(format!(
                "{} must be set to load/save optimizer checkpoint",
                STATE_HMAC_ENV_KEY
            ))
        })?;
    Ok(key)
}

/// Errors produced by the optimizer module.
#[derive(Debug)]
pub enum OptimizerError {
    InvalidInput(String),
    NonFinite(String),
    SafetyViolation(String),
    Io(String),
    Serialization(String),
}

impl fmt::Display for OptimizerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            Self::NonFinite(msg) => write!(f, "non-finite arithmetic: {msg}"),
            Self::SafetyViolation(msg) => write!(f, "safety violation: {msg}"),
            Self::Io(msg) => write!(f, "io error: {msg}"),
            Self::Serialization(msg) => write!(f, "serialization error: {msg}"),
        }
    }
}

impl Error for OptimizerError {}

impl From<std::io::Error> for OptimizerError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

impl From<serde_json::Error> for OptimizerError {
    fn from(value: serde_json::Error) -> Self {
        Self::Serialization(value.to_string())
    }
}

/// Integration entrypoint used by rebalance/keeper loop.
///
/// Flow:
/// 1) Compute loss + gradient at current params
/// 2) Save pre-update checkpoint (disk)
/// 3) Adam update
/// 4) Project to Π_Ω
/// 5) Validate safety, compute post-update loss, save good checkpoint
/// 6) On NaN/Inf/safety violations -> rollback to last good checkpoint
pub fn optimize_step(
    snapshot: &ProtocolSnapshot,
    current_params: &ParamVector,
    optimizer: &mut AdamOptimizer,
    bounds: &SafetyBounds,
    checkpoint: &mut Option<OptimizerCheckpoint>,
) -> Result<ParamVector, OptimizerError> {
    bounds.validate()?;

    if !current_params.is_finite() {
        return rollback_or_error(
            optimizer,
            checkpoint,
            OptimizerError::NonFinite("current params are non-finite".to_string()),
        );
    }

    let eval_snapshot = snapshot.with_params(current_params);
    let loss_fn = eval_snapshot.loss_function.unwrap_or_default();

    let baseline_loss = match loss_fn.compute(&eval_snapshot) {
        Ok(result) => result,
        Err(err) => return rollback_or_error(optimizer, checkpoint, err),
    };

    let pre_checkpoint = OptimizerCheckpoint {
        params: *current_params,
        adam_state: optimizer.state.clone(),
        tick: optimizer.state.t,
        loss: baseline_loss.total_loss,
    };
    pre_checkpoint.save_default()?;
    *checkpoint = Some(pre_checkpoint);

    if !baseline_loss.total_gradient.is_finite() {
        return rollback_or_error(
            optimizer,
            checkpoint,
            OptimizerError::NonFinite("gradient contains NaN/Inf".to_string()),
        );
    }

    let candidate = optimizer.step(current_params, &baseline_loss.total_gradient);
    if !candidate.is_finite() {
        return rollback_or_error(
            optimizer,
            checkpoint,
            OptimizerError::NonFinite("candidate params contain NaN/Inf".to_string()),
        );
    }

    // Enforce per-epoch deltas against current params.
    let mut projection_bounds = bounds.clone();
    projection_bounds.reference_params = Some(*current_params);

    let projected = project_to_safety_set(&candidate, &projection_bounds);
    if let Err(err) = validate_safety_set(&projected, &projection_bounds) {
        return rollback_or_error(optimizer, checkpoint, err);
    }

    let post_snapshot = snapshot.with_params(&projected);
    let post_loss = match loss_fn.compute(&post_snapshot) {
        Ok(result) => result.total_loss,
        Err(err) => return rollback_or_error(optimizer, checkpoint, err),
    };

    let good_checkpoint = OptimizerCheckpoint {
        params: projected,
        adam_state: optimizer.state.clone(),
        tick: optimizer.state.t,
        loss: post_loss,
    };
    good_checkpoint.save_default()?;
    *checkpoint = Some(good_checkpoint);

    Ok(projected)
}

fn rollback_or_error(
    optimizer: &mut AdamOptimizer,
    checkpoint: &Option<OptimizerCheckpoint>,
    err: OptimizerError,
) -> Result<ParamVector, OptimizerError> {
    if let Some(last_good) = checkpoint {
        optimizer.state = last_good.adam_state.clone();
        return Ok(last_good.params);
    }

    Err(err)
}

pub fn checkpoint_path() -> PathBuf {
    default_checkpoint_path()
}

fn default_checkpoint_path() -> PathBuf {
    env::var("MICROSTABLE_OPTIMIZER_CHECKPOINT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_CHECKPOINT_FILE))
}

fn dot(a: &[f64; WEIGHT_DIM], b: &[f64; WEIGHT_DIM]) -> f64 {
    let mut sum = 0.0;
    for i in 0..WEIGHT_DIM {
        sum += a[i] * b[i];
    }
    sum
}

fn l1_distance(a: &[f64; WEIGHT_DIM], b: &[f64; WEIGHT_DIM]) -> f64 {
    let mut sum = 0.0;
    for i in 0..WEIGHT_DIM {
        sum += (a[i] - b[i]).abs();
    }
    sum
}

fn signed_unit(x: f64) -> f64 {
    if x > 0.0 {
        1.0
    } else if x < 0.0 {
        -1.0
    } else {
        0.0
    }
}

fn variance_of_diffs(history: &[f64]) -> Result<f64, OptimizerError> {
    if history.len() < 2 {
        return Ok(0.0);
    }

    let mut diffs = Vec::with_capacity(history.len().saturating_sub(1));
    for pair in history.windows(2) {
        let d = pair[1] - pair[0];
        if !d.is_finite() {
            return Err(OptimizerError::NonFinite(
                "NAV delta became non-finite".to_string(),
            ));
        }
        diffs.push(d);
    }

    if diffs.is_empty() {
        return Ok(0.0);
    }

    let mean = diffs.iter().sum::<f64>() / diffs.len() as f64;
    let variance = diffs
        .iter()
        .map(|x| {
            let d = *x - mean;
            d * d
        })
        .sum::<f64>()
        / diffs.len() as f64;

    ensure_finite(variance, "variance of NAV deltas")
}

fn ensure_finite(value: f64, label: &str) -> Result<f64, OptimizerError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(OptimizerError::NonFinite(format!("{label} is not finite")))
    }
}

fn clamp_with_optional_delta(
    value: f64,
    reference: f64,
    max_delta: f64,
    lower_bound: f64,
    upper_bound: f64,
) -> f64 {
    let mut lo = lower_bound;
    let mut hi = upper_bound;

    if max_delta.is_finite() {
        let d = max_delta.max(0.0);
        lo = lo.max(reference - d);
        hi = hi.min(reference + d);
    }

    if lo > hi {
        return lo;
    }

    value.clamp(lo, hi)
}

#[cfg(test)]
fn project_onto_capped_simplex(v: [f64; WEIGHT_DIM], caps: [f64; WEIGHT_DIM]) -> [f64; WEIGHT_DIM] {
    let lower = [0.0; WEIGHT_DIM];
    let mut upper = [1.0; WEIGHT_DIM];
    for i in 0..WEIGHT_DIM {
        upper[i] = if caps[i].is_finite() {
            caps[i].clamp(0.0, 1.0)
        } else {
            1.0
        };
    }

    if upper.iter().sum::<f64>() < 1.0 - SAFETY_TOLERANCE {
        return project_onto_simplex(v);
    }

    project_onto_bounded_simplex(v, lower, upper)
}

fn project_onto_bounded_simplex(
    v: [f64; WEIGHT_DIM],
    lower: [f64; WEIGHT_DIM],
    upper: [f64; WEIGHT_DIM],
) -> [f64; WEIGHT_DIM] {
    let mut l = [0.0; WEIGHT_DIM];
    let mut u = [1.0; WEIGHT_DIM];
    for i in 0..WEIGHT_DIM {
        l[i] = lower[i].clamp(0.0, 1.0);
        u[i] = upper[i].clamp(0.0, 1.0);
        if l[i] > u[i] {
            std::mem::swap(&mut l[i], &mut u[i]);
        }
    }

    let sum_l = l.iter().sum::<f64>();
    let sum_u = u.iter().sum::<f64>();
    if sum_l > 1.0 + SAFETY_TOLERANCE || sum_u < 1.0 - SAFETY_TOLERANCE {
        return project_onto_simplex(v);
    }

    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for i in 0..WEIGHT_DIM {
        lo = lo.min(v[i] - u[i]);
        hi = hi.max(v[i] - l[i]);
    }

    for _ in 0..120 {
        let mid = 0.5 * (lo + hi);
        let mut sum = 0.0;
        for i in 0..WEIGHT_DIM {
            sum += (v[i] - mid).clamp(l[i], u[i]);
        }

        if sum > 1.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let tau = hi;
    let mut w = [0.0; WEIGHT_DIM];
    for i in 0..WEIGHT_DIM {
        w[i] = (v[i] - tau).clamp(l[i], u[i]);
    }

    rebalance_sum_to_one_with_bounds(&mut w, &l, &u);
    w
}

fn project_onto_simplex(v: [f64; WEIGHT_DIM]) -> [f64; WEIGHT_DIM] {
    let mut u = v;
    u.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    let mut cssv = 0.0;
    let mut rho = 0usize;
    for (j, val) in u.iter().enumerate() {
        cssv += *val;
        let t = (cssv - 1.0) / (j as f64 + 1.0);
        if *val > t {
            rho = j;
        }
    }

    let theta = (u.iter().take(rho + 1).sum::<f64>() - 1.0) / (rho as f64 + 1.0);
    let mut w = [0.0; WEIGHT_DIM];
    for i in 0..WEIGHT_DIM {
        w[i] = (v[i] - theta).max(0.0);
    }

    let lower = [0.0; WEIGHT_DIM];
    let upper = [1.0; WEIGHT_DIM];
    rebalance_sum_to_one_with_bounds(&mut w, &lower, &upper);
    w
}

fn rebalance_sum_to_one_with_bounds(
    weights: &mut [f64; WEIGHT_DIM],
    lower: &[f64; WEIGHT_DIM],
    upper: &[f64; WEIGHT_DIM],
) {
    for i in 0..WEIGHT_DIM {
        weights[i] = weights[i].clamp(lower[i], upper[i]);
    }

    let mut sum: f64 = weights.iter().sum();
    let mut diff = 1.0 - sum;

    for _ in 0..32 {
        if diff.abs() <= 1e-12 {
            break;
        }

        if diff > 0.0 {
            let mut free = Vec::new();
            for i in 0..WEIGHT_DIM {
                if upper[i] - weights[i] > 1e-12 {
                    free.push(i);
                }
            }
            if free.is_empty() {
                break;
            }

            let share = diff / free.len() as f64;
            let mut added = 0.0;
            for i in free {
                let room = upper[i] - weights[i];
                let delta = room.min(share);
                weights[i] += delta;
                added += delta;
            }
            diff -= added;
        } else {
            let mut free = Vec::new();
            for i in 0..WEIGHT_DIM {
                if weights[i] - lower[i] > 1e-12 {
                    free.push(i);
                }
            }
            if free.is_empty() {
                break;
            }

            let share = (-diff) / free.len() as f64;
            let mut removed = 0.0;
            for i in free {
                let room = weights[i] - lower[i];
                let delta = room.min(share);
                weights[i] -= delta;
                removed += delta;
            }
            diff += removed;
        }
    }

    // Final tiny correction to avoid drift.
    sum = weights.iter().sum();
    if (sum - 1.0).abs() > 1e-9 {
        let mut best_idx = 0usize;
        let mut best_room = f64::NEG_INFINITY;
        let target_add = 1.0 - sum;
        for i in 0..WEIGHT_DIM {
            let room = if target_add >= 0.0 {
                upper[i] - weights[i]
            } else {
                weights[i] - lower[i]
            };
            if room > best_room {
                best_room = room;
                best_idx = i;
            }
        }

        if target_add >= 0.0 {
            weights[best_idx] += target_add.min((upper[best_idx] - weights[best_idx]).max(0.0));
        } else {
            weights[best_idx] -= (-target_add).min((weights[best_idx] - lower[best_idx]).max(0.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capped_simplex_projection_satisfies_constraints() {
        let v = [0.9, -0.1, 0.3, 0.2];
        let caps = [0.5, 0.5, 0.5, 0.5];
        let w = project_onto_capped_simplex(v, caps);

        let sum: f64 = w.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6);
        for i in 0..WEIGHT_DIM {
            assert!(w[i] >= -1e-9);
            assert!(w[i] <= caps[i] + 1e-9);
        }
    }

    #[test]
    fn checkpoint_round_trip() {
        let checkpoint = OptimizerCheckpoint {
            params: ParamVector::default(),
            adam_state: AdamState::default(),
            tick: 7,
            loss: 0.123,
        };

        std::env::set_var(STATE_HMAC_ENV_KEY, "test-state-hmac-key");
        let tmp = std::env::temp_dir().join("optimizer_checkpoint_round_trip.json");
        checkpoint.save_to_path(&tmp).unwrap();
        let loaded = OptimizerCheckpoint::load_from_path(&tmp).unwrap();
        assert_eq!(checkpoint, loaded);
        let _ = std::fs::remove_file(tmp);
        std::env::remove_var(STATE_HMAC_ENV_KEY);
    }
}
