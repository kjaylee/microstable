use crate::risk_manager::{
    assess_risk_level, auto_recovery_step, compute_dynamic_fees, redemption_queue_policy,
    should_throttle_redemptions, RecoveryAction, RiskLevel,
};

#[test]
fn tc_rm_01_assess_risk_normal_when_cr_above_150() {
    assert_eq!(assess_risk_level(1.51, 1.20), RiskLevel::Normal);
}

#[test]
fn tc_rm_02_assess_risk_elevated_in_120_to_150_band() {
    assert_eq!(assess_risk_level(1.20, 1.20), RiskLevel::Elevated);
    assert_eq!(assess_risk_level(1.50, 1.20), RiskLevel::Elevated);
}

#[test]
fn tc_rm_03_assess_risk_high_in_110_to_120_band() {
    assert_eq!(assess_risk_level(1.10, 1.20), RiskLevel::High);
    assert_eq!(assess_risk_level(1.19, 1.20), RiskLevel::High);
}

#[test]
fn tc_rm_04_assess_risk_critical_when_below_110() {
    assert_eq!(assess_risk_level(1.09, 1.20), RiskLevel::Critical);
}

#[test]
fn tc_rm_05_compute_dynamic_fees_normal_is_base() {
    let (mint, redeem) = compute_dynamic_fees(RiskLevel::Normal, 2_000, 2_500);
    assert_eq!(mint, 2_000);
    assert_eq!(redeem, 2_500);
}

#[test]
fn tc_rm_06_compute_dynamic_fees_increase_with_risk() {
    let base_mint = 2_000;
    let base_redeem = 2_000;

    let normal = compute_dynamic_fees(RiskLevel::Normal, base_mint, base_redeem);
    let elevated = compute_dynamic_fees(RiskLevel::Elevated, base_mint, base_redeem);
    let high = compute_dynamic_fees(RiskLevel::High, base_mint, base_redeem);
    let critical = compute_dynamic_fees(RiskLevel::Critical, base_mint, base_redeem);

    assert!(normal.0 <= elevated.0 && elevated.0 <= high.0 && high.0 <= critical.0);
    assert!(normal.1 <= elevated.1 && elevated.1 <= high.1 && high.1 <= critical.1);
}

#[test]
fn tc_rm_07_should_throttle_redemptions_by_level_and_volume() {
    assert!(!should_throttle_redemptions(RiskLevel::Normal, 1_000_000));
    assert!(!should_throttle_redemptions(RiskLevel::Elevated, 99_999));
    assert!(should_throttle_redemptions(RiskLevel::Elevated, 100_000));
    assert!(!should_throttle_redemptions(RiskLevel::High, 49_999));
    assert!(should_throttle_redemptions(RiskLevel::High, 50_000));
    assert!(should_throttle_redemptions(RiskLevel::Critical, 0));
}

#[test]
fn tc_rm_08_redemption_queue_policy_gets_stricter_with_risk() {
    let normal = redemption_queue_policy(RiskLevel::Normal);
    let elevated = redemption_queue_policy(RiskLevel::Elevated);
    let high = redemption_queue_policy(RiskLevel::High);
    let critical = redemption_queue_policy(RiskLevel::Critical);

    assert!(!normal.enabled);
    assert!(elevated.enabled && high.enabled && critical.enabled);

    assert!(normal.max_per_epoch > elevated.max_per_epoch);
    assert!(elevated.max_per_epoch > high.max_per_epoch);
    assert!(high.max_per_epoch > critical.max_per_epoch);

    assert!(normal.delay_slots < elevated.delay_slots);
    assert!(elevated.delay_slots < high.delay_slots);
    assert!(high.delay_slots < critical.delay_slots);
}

#[test]
fn tc_rm_09_auto_recovery_holds_conservative_for_high_or_critical() {
    assert_eq!(
        auto_recovery_step(RiskLevel::High, RiskLevel::Elevated, 3),
        RecoveryAction::HoldConservative
    );
    assert_eq!(
        auto_recovery_step(RiskLevel::Critical, RiskLevel::Normal, 8),
        RecoveryAction::HoldConservative
    );
}

#[test]
fn tc_rm_10_auto_recovery_graduates_from_critical_to_normal() {
    assert_eq!(
        auto_recovery_step(RiskLevel::Normal, RiskLevel::Critical, 0),
        RecoveryAction::HoldConservative
    );
    assert_eq!(
        auto_recovery_step(RiskLevel::Normal, RiskLevel::Critical, 2),
        RecoveryAction::RelaxPartially
    );
    assert_eq!(
        auto_recovery_step(RiskLevel::Normal, RiskLevel::Critical, 4),
        RecoveryAction::ResumeNormal
    );
}
