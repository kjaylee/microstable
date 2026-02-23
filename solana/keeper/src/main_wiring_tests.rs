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
