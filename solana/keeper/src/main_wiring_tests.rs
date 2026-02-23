#![cfg(test)]

#[test]
fn tc_mw_01_run_cycle_wires_risk_manager_between_rebalance_and_aig() {
    let source = include_str!("main.rs");

    let rebalance_idx = source
        .find("rebalance::run_rebalance_cycle")
        .expect("expected rebalance step call in run_cycle");
    let risk_manager_idx = source
        .find("risk_manager::run_risk_manager_cycle")
        .expect("expected risk manager step call in run_cycle");

    let aig_idx = source
        .find("agent_loop::maybe_run_aig_cycle_with_tx")
        .or_else(|| source.find("agent_loop::maybe_run_aig_cycle"))
        .expect("expected AIG step call in run_cycle");

    assert!(
        rebalance_idx < risk_manager_idx && risk_manager_idx < aig_idx,
        "risk manager should run after rebalance and before aig"
    );
}

#[test]
fn tc_mw_02_main_loop_has_consecutive_failure_guardrail() {
    let source = include_str!("main.rs");

    assert!(
        source.contains("consecutive_failed_cycles")
            && source.contains("max_consecutive_failed_cycles")
            && source.contains("too many consecutive failed cycles"),
        "main loop must enforce automatic protective exit on repeated cycle failures"
    );
}

#[test]
fn tc_mw_03_dynamic_fee_update_decision_covers_all_risk_levels() {
    let base_mint = 2_000;
    let base_redeem = 2_000;

    let cases = [
        (super::risk_manager::RiskLevel::Normal, 2_000, 2_000),
        (super::risk_manager::RiskLevel::Elevated, 2_200, 2_400),
        (super::risk_manager::RiskLevel::High, 2_700, 3_000),
        (super::risk_manager::RiskLevel::Critical, 3_500, 4_000),
    ];

    for (risk_level, expected_mint, expected_redeem) in cases {
        let update =
            super::dynamic_fee_update_for_risk_level(risk_level, base_mint, base_redeem, 0, 0)
                .expect("fees should require update when current fees differ from target");

        assert_eq!(update.next_mint_fee, expected_mint);
        assert_eq!(update.next_redeem_fee, expected_redeem);

        let no_update = super::dynamic_fee_update_for_risk_level(
            risk_level,
            base_mint,
            base_redeem,
            expected_mint,
            expected_redeem,
        );
        assert!(
            no_update.is_none(),
            "fees should not update when already at target for {:?}",
            risk_level
        );
    }
}

#[test]
fn tc_mw_04_run_cycle_wires_dynamic_fee_application_after_risk_manager() {
    let source = include_str!("main.rs");

    let risk_manager_idx = source
        .find("risk_manager::run_risk_manager_cycle")
        .expect("expected risk manager step call in run_cycle");
    let dynamic_fee_idx = source
        .find("if let Err(err) = maybe_apply_dynamic_fees")
        .expect("expected dynamic fee application call in run_cycle");

    assert!(
        risk_manager_idx < dynamic_fee_idx,
        "dynamic fee application should occur after risk manager step"
    );
    assert!(
        source.contains("wire::ix_update_protocol_params")
            && source.contains("compute_dynamic_fees"),
        "run_cycle should compute dynamic fees and submit update_protocol_params when needed"
    );
}

#[test]
fn tc_mw_05_startup_loads_optimizer_checkpoint_into_rebalance_memory() {
    let source = include_str!("main.rs");

    assert!(
        source.contains("load_optimizer_checkpoint_into_memory")
            && source.contains("OptimizerCheckpoint::load_from_path")
            && source.contains("restore_optimizer_checkpoint"),
        "main startup should load optimizer checkpoint and restore in-memory optimizer state"
    );
}
