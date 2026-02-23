#![cfg(test)]

use crate::optimizer::{
    optimize_step, project_to_safety_set, validate_safety_set, AdamOptimizer, AdamState,
    LossFunction, OptimizerCheckpoint, ParamVector, ProtocolSnapshot, SafetyBounds,
};

fn baseline_snapshot() -> ProtocolSnapshot {
    ProtocolSnapshot {
        peg_price: 1.0,
        collateral_ratio: 1.4,
        nav_history: vec![100.0, 100.0, 100.0],
        current_weights: [0.25, 0.25, 0.25, 0.25],
        previous_weights: [0.25, 0.25, 0.25, 0.25],
        oracle_quality_scores: [1.0, 1.0, 1.0, 1.0],
        target_cr: 1.2,
        mint_fee: 0.001,
        redeem_fee: 0.001,
        loss_function: None,
    }
}

fn approx(a: f64, b: f64, tol: f64) {
    assert!(
        (a - b).abs() <= tol,
        "expected {} ~= {} (tol {})",
        a,
        b,
        tol
    );
}

#[test]
fn loss_zero_in_ideal_state() {
    let snapshot = baseline_snapshot();
    let loss = LossFunction::default().compute(&snapshot).unwrap();
    approx(loss.total_loss, 0.0, 1e-12);
}

#[test]
fn loss_each_term_independent() {
    // price
    let mut s = baseline_snapshot();
    s.peg_price = 1.1;
    let mut l = LossFunction::default();
    l.lambda_cr = 0.0;
    l.lambda_vol = 0.0;
    l.lambda_turn = 0.0;
    l.lambda_conc = 0.0;
    l.lambda_oracle = 0.0;
    let out = l.compute(&s).unwrap();
    approx(out.total_loss, 0.01, 1e-10);

    // collateral ratio
    let mut s = baseline_snapshot();
    s.collateral_ratio = 1.05;
    s.target_cr = 1.20;
    let mut l = LossFunction::default();
    l.lambda_price = 0.0;
    l.lambda_vol = 0.0;
    l.lambda_turn = 0.0;
    l.lambda_conc = 0.0;
    l.lambda_oracle = 0.0;
    let out = l.compute(&s).unwrap();
    approx(out.total_loss, 0.15 * 0.15, 1e-10);

    // volatility
    let mut s = baseline_snapshot();
    s.nav_history = vec![100.0, 102.0, 101.0, 105.0];
    let mut l = LossFunction::default();
    l.lambda_price = 0.0;
    l.lambda_cr = 0.0;
    l.lambda_turn = 0.0;
    l.lambda_conc = 0.0;
    l.lambda_oracle = 0.0;
    let out = l.compute(&s).unwrap();
    // diffs=[2,-1,4], mean=5/3, var=38/9
    approx(out.total_loss, 38.0 / 9.0, 1e-10);

    // turnover
    let mut s = baseline_snapshot();
    s.current_weights = [0.40, 0.30, 0.20, 0.10];
    let mut l = LossFunction::default();
    l.lambda_price = 0.0;
    l.lambda_cr = 0.0;
    l.lambda_vol = 0.0;
    l.lambda_conc = 0.0;
    l.lambda_oracle = 0.0;
    let out = l.compute(&s).unwrap();
    approx(out.total_loss, 0.40, 1e-12);

    // concentration (centered around equal weights)
    let mut s = baseline_snapshot();
    s.current_weights = [0.40, 0.30, 0.20, 0.10];
    let mut l = LossFunction::default();
    l.lambda_price = 0.0;
    l.lambda_cr = 0.0;
    l.lambda_vol = 0.0;
    l.lambda_turn = 0.0;
    l.lambda_oracle = 0.0;
    let out = l.compute(&s).unwrap();
    approx(out.total_loss, 0.05, 1e-12);

    // oracle quality
    let mut s = baseline_snapshot();
    s.oracle_quality_scores = [0.9, 0.8, 1.0, 0.7];
    let mut l = LossFunction::default();
    l.lambda_price = 0.0;
    l.lambda_cr = 0.0;
    l.lambda_vol = 0.0;
    l.lambda_turn = 0.0;
    l.lambda_conc = 0.0;
    let out = l.compute(&s).unwrap();
    approx(out.total_loss, 0.0225, 1e-12);
}

#[test]
fn loss_gradient_matches_finite_difference() {
    let mut snapshot = baseline_snapshot();
    snapshot.peg_price = 1.03;
    snapshot.collateral_ratio = 1.10;
    snapshot.nav_history = vec![100.0, 99.0, 101.5, 100.0];
    snapshot.current_weights = [0.50, 0.20, 0.20, 0.10];
    snapshot.previous_weights = [0.25, 0.25, 0.25, 0.25];
    snapshot.oracle_quality_scores = [0.8, 0.9, 0.7, 1.0];
    snapshot.target_cr = 1.25;
    snapshot.mint_fee = 0.004;
    snapshot.redeem_fee = 0.002;

    let loss_fn = LossFunction {
        lambda_price: 1.3,
        lambda_cr: 0.8,
        lambda_vol: 0.4,
        lambda_turn: 0.7,
        lambda_conc: 1.1,
        lambda_oracle: 0.9,
    };

    let analytical = loss_fn.compute(&snapshot).unwrap().total_gradient;
    let eps = 1e-6;

    let base_params = ParamVector {
        weights: snapshot.current_weights,
        target_cr: snapshot.target_cr,
        mint_fee: snapshot.mint_fee,
        redeem_fee: snapshot.redeem_fee,
    };

    let mut num = [0.0_f64; 7];
    for i in 0..7 {
        let mut plus = base_params.flatten();
        let mut minus = base_params.flatten();
        plus[i] += eps;
        minus[i] -= eps;

        let p_plus = ParamVector::from_flat(plus);
        let p_minus = ParamVector::from_flat(minus);

        let s_plus = snapshot.with_params(&p_plus);
        let s_minus = snapshot.with_params(&p_minus);

        let f_plus = loss_fn.compute(&s_plus).unwrap().total_loss;
        let f_minus = loss_fn.compute(&s_minus).unwrap().total_loss;
        num[i] = (f_plus - f_minus) / (2.0 * eps);
    }

    let analytical_flat = analytical.flatten();
    for i in 0..7 {
        approx(analytical_flat[i], num[i], 2e-4);
    }
}

#[test]
fn loss_edge_cases_and_nan_handling() {
    let mut zero_weights = baseline_snapshot();
    zero_weights.current_weights = [0.0, 0.0, 0.0, 0.0];
    let out = LossFunction::default().compute(&zero_weights).unwrap();
    assert!(out.total_loss.is_finite());

    let mut extreme = baseline_snapshot();
    extreme.peg_price = 1_000.0;
    extreme.collateral_ratio = 0.01;
    let out = LossFunction::default().compute(&extreme).unwrap();
    assert!(out.total_loss.is_finite());

    let mut bad = baseline_snapshot();
    bad.peg_price = f64::NAN;
    assert!(LossFunction::default().compute(&bad).is_err());
}

#[test]
fn adam_converges_on_quadratic() {
    let mut optimizer = AdamOptimizer {
        learning_rate: 0.1,
        warmup_steps: 0,
        decay_steps: 0,
        min_learning_rate: 0.1,
        ..AdamOptimizer::default()
    };

    let mut p = ParamVector {
        weights: [0.25, 0.25, 0.25, 0.25],
        target_cr: 10.0,
        mint_fee: 0.0,
        redeem_fee: 0.0,
    };

    for _ in 0..2_000 {
        let g = ParamVector {
            weights: [0.0; 4],
            target_cr: 2.0 * p.target_cr,
            mint_fee: 0.0,
            redeem_fee: 0.0,
        };
        p = optimizer.step(&p, &g);
    }

    assert!(p.target_cr.abs() < 1e-2, "target_cr={}", p.target_cr);
}

#[test]
fn adam_gradient_clipping_works() {
    let optimizer = AdamOptimizer {
        max_grad_norm: 1.0,
        ..AdamOptimizer::default()
    };

    let g = ParamVector {
        weights: [100.0, 0.0, 0.0, 0.0],
        target_cr: 0.0,
        mint_fee: 0.0,
        redeem_fee: 0.0,
    };

    let clipped = optimizer.clip_gradients(&g);
    approx(clipped.l2_norm(), 1.0, 1e-9);
}

#[test]
fn adam_learning_rate_schedule_warmup_and_decay() {
    let optimizer = AdamOptimizer {
        learning_rate: 0.1,
        warmup_steps: 10,
        decay_steps: 100,
        min_learning_rate: 0.01,
        ..AdamOptimizer::default()
    };

    let lr1 = optimizer.learning_rate_for_step(1);
    let lr5 = optimizer.learning_rate_for_step(5);
    let lr10 = optimizer.learning_rate_for_step(10);
    let lr20 = optimizer.learning_rate_for_step(20);
    let lr100 = optimizer.learning_rate_for_step(100);
    let lr110 = optimizer.learning_rate_for_step(110);

    assert!(lr1 < lr5 && lr5 < lr10);
    assert!(lr20 < lr10);
    assert!(lr100 > optimizer.min_learning_rate - 1e-9);
    approx(lr110, optimizer.min_learning_rate, 1e-9);
}

#[test]
fn projection_simplex_and_non_negative() {
    let params = ParamVector {
        weights: [0.5, 0.5, 0.5, 0.5],
        ..ParamVector::default()
    };

    let out = project_to_safety_set(&params, &SafetyBounds::default());
    let sum: f64 = out.weights.iter().sum();
    approx(sum, 1.0, 1e-9);
    for w in out.weights {
        assert!(w >= -1e-9);
    }

    let neg = ParamVector {
        weights: [-0.5, 0.2, 0.2, 0.1],
        ..ParamVector::default()
    };
    let out = project_to_safety_set(&neg, &SafetyBounds::default());
    for w in out.weights {
        assert!(w >= -1e-9);
    }
}

#[test]
fn projection_cap_and_delta_and_scalar_bounds() {
    let params = ParamVector {
        weights: [0.9, 0.05, 0.03, 0.02],
        target_cr: 9.0,
        mint_fee: -0.4,
        redeem_fee: 0.9,
    };

    let reference = ParamVector::default();
    let bounds = SafetyBounds {
        weight_caps: [0.5, 0.5, 0.5, 0.5],
        cr_min: 1.0,
        cr_max: 1.5,
        fee_min: 0.0,
        fee_max: 0.02,
        max_delta: ParamVector {
            weights: [0.1, 0.1, 0.1, 0.1],
            target_cr: 0.05,
            mint_fee: 0.005,
            redeem_fee: 0.005,
        },
        reference_params: Some(reference),
    };

    let projected = project_to_safety_set(&params, &bounds);

    let sum: f64 = projected.weights.iter().sum();
    approx(sum, 1.0, 1e-8);
    assert!(projected.weights[0] <= 0.5 + 1e-8);
    assert!(projected.abs_diff_leq(&reference, &bounds.max_delta, 1e-8));
    assert!(projected.target_cr >= bounds.cr_min && projected.target_cr <= bounds.cr_max);
    assert!(projected.mint_fee >= bounds.fee_min && projected.mint_fee <= bounds.fee_max);
    assert!(projected.redeem_fee >= bounds.fee_min && projected.redeem_fee <= bounds.fee_max);
}

#[test]
fn integration_optimize_step_produces_valid_output() {
    let mut snapshot = baseline_snapshot();
    snapshot.peg_price = 1.02;
    snapshot.collateral_ratio = 1.08;
    snapshot.target_cr = 1.25;
    snapshot.mint_fee = 0.005;
    snapshot.redeem_fee = 0.001;
    snapshot.current_weights = [0.6, 0.2, 0.1, 0.1];
    snapshot.previous_weights = [0.25, 0.25, 0.25, 0.25];
    snapshot.oracle_quality_scores = [0.7, 0.8, 0.9, 1.0];

    let current = ParamVector {
        weights: snapshot.current_weights,
        target_cr: snapshot.target_cr,
        mint_fee: snapshot.mint_fee,
        redeem_fee: snapshot.redeem_fee,
    };

    let mut optimizer = AdamOptimizer::default();
    let bounds = SafetyBounds {
        weight_caps: [0.7, 0.6, 0.6, 0.6],
        cr_min: 1.0,
        cr_max: 1.6,
        fee_min: 0.0,
        fee_max: 0.02,
        max_delta: ParamVector {
            weights: [0.2, 0.2, 0.2, 0.2],
            target_cr: 0.1,
            mint_fee: 0.01,
            redeem_fee: 0.01,
        },
        reference_params: None,
    };

    let mut checkpoint = None;
    let out = optimize_step(
        &snapshot,
        &current,
        &mut optimizer,
        &bounds,
        &mut checkpoint,
    )
    .unwrap();
    validate_safety_set(&out, &bounds.with_reference(current)).unwrap();
}

#[test]
fn integration_nan_input_rolls_back_to_checkpoint() {
    let mut optimizer = AdamOptimizer::default();
    let bounds = SafetyBounds::default();

    let good_snapshot = baseline_snapshot();
    let current = ParamVector::default();
    let mut checkpoint = None;

    let first = optimize_step(
        &good_snapshot,
        &current,
        &mut optimizer,
        &bounds,
        &mut checkpoint,
    )
    .unwrap();

    let mut bad_snapshot = baseline_snapshot();
    bad_snapshot.peg_price = f64::NAN;

    let rolled_back = optimize_step(
        &bad_snapshot,
        &first,
        &mut optimizer,
        &bounds,
        &mut checkpoint,
    )
    .unwrap();

    assert_eq!(rolled_back, first);
}

#[test]
fn integration_multiple_steps_reduce_loss_simple_case() {
    let loss_fn = LossFunction {
        lambda_price: 0.0,
        lambda_cr: 1.0,
        lambda_vol: 0.0,
        lambda_turn: 0.0,
        lambda_conc: 0.0,
        lambda_oracle: 0.0,
    };

    let mut snapshot = baseline_snapshot();
    snapshot.collateral_ratio = 1.1;
    snapshot.target_cr = 1.6;
    snapshot.loss_function = Some(loss_fn);

    let mut params = ParamVector {
        weights: [0.25, 0.25, 0.25, 0.25],
        target_cr: 1.6,
        mint_fee: 0.001,
        redeem_fee: 0.001,
    };

    let mut optimizer = AdamOptimizer {
        learning_rate: 0.05,
        warmup_steps: 0,
        decay_steps: 0,
        min_learning_rate: 0.05,
        ..AdamOptimizer::default()
    };

    let bounds = SafetyBounds {
        cr_min: 1.0,
        cr_max: 2.0,
        max_delta: ParamVector {
            weights: [1.0; 4],
            target_cr: 0.2,
            mint_fee: 0.1,
            redeem_fee: 0.1,
        },
        ..SafetyBounds::default()
    };

    let mut checkpoint = None;
    let mut prev_loss = f64::INFINITY;

    for _ in 0..20 {
        snapshot = snapshot.with_params(&params);
        let loss = loss_fn.compute(&snapshot).unwrap().total_loss;
        assert!(
            loss <= prev_loss + 1e-8,
            "loss increased: {} > {}",
            loss,
            prev_loss
        );
        prev_loss = loss;

        params =
            optimize_step(&snapshot, &params, &mut optimizer, &bounds, &mut checkpoint).unwrap();
    }
}

#[test]
fn checkpoint_round_trip_preserves_adam_state_and_tick() {
    let checkpoint = OptimizerCheckpoint {
        params: ParamVector {
            weights: [0.4, 0.3, 0.2, 0.1],
            target_cr: 1.27,
            mint_fee: 0.003,
            redeem_fee: 0.004,
        },
        adam_state: AdamState {
            m: ParamVector {
                weights: [0.01, -0.02, 0.03, -0.04],
                target_cr: 0.05,
                mint_fee: -0.06,
                redeem_fee: 0.07,
            },
            v: ParamVector {
                weights: [0.11, 0.12, 0.13, 0.14],
                target_cr: 0.15,
                mint_fee: 0.16,
                redeem_fee: 0.17,
            },
            t: 42,
        },
        tick: 42,
        loss: 0.987654321,
    };

    let unique = format!(
        "microstable_optimizer_checkpoint_roundtrip_{}_{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time should be monotonic")
            .as_nanos()
    );
    let path = std::env::temp_dir().join(unique);

    checkpoint
        .save_to_path(&path)
        .expect("checkpoint should save");
    let loaded = OptimizerCheckpoint::load_from_path(&path).expect("checkpoint should load");

    std::fs::remove_file(&path).ok();
    assert_eq!(loaded, checkpoint);
}
