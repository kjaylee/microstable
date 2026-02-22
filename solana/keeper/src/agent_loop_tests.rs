#![cfg(test)]

use crate::{
    agent_loop::{maybe_run_aig_cycle, maybe_run_tournament_cycle, AgentLoopState},
    config::KeeperConfig,
};
use std::time::{Duration, Instant};

fn test_config() -> KeeperConfig {
    let mut cfg = KeeperConfig::default_devnet();
    cfg.aig_enabled = false;
    cfg.tournament_enabled = false;
    cfg.aig_interval_secs = 60;
    cfg.tournament_interval_secs = 60;
    cfg
}

#[test]
fn tc_al_01_aig_cycle_skip_when_disabled() {
    let cfg = test_config();
    let mut state = AgentLoopState::default();

    maybe_run_aig_cycle(&cfg, &mut state).unwrap();

    assert!(state.last_aig_run.is_none());
}

#[test]
fn tc_al_02_aig_cycle_skip_when_interval_not_elapsed() {
    let mut cfg = test_config();
    cfg.aig_enabled = true;

    let mut state = AgentLoopState::default();
    let last = Instant::now();
    state.last_aig_run = Some(last);

    maybe_run_aig_cycle(&cfg, &mut state).unwrap();

    assert_eq!(state.last_aig_run, Some(last));
}

#[test]
fn tc_al_03_aig_cycle_runs_when_enabled_and_interval_elapsed() {
    let mut cfg = test_config();
    cfg.aig_enabled = true;
    cfg.aig_interval_secs = 1;

    let mut state = AgentLoopState::default();
    let previous = Instant::now() - Duration::from_secs(3);
    state.last_aig_run = Some(previous);

    maybe_run_aig_cycle(&cfg, &mut state).unwrap();

    let last_run = state.last_aig_run.expect("expected AIG run timestamp");
    assert!(last_run > previous);
}

#[test]
fn tc_al_04_tournament_cycle_skip_when_disabled() {
    let cfg = test_config();
    let mut state = AgentLoopState::default();

    maybe_run_tournament_cycle(&cfg, &mut state).unwrap();

    assert!(state.last_tournament_run.is_none());
}

#[test]
fn tc_al_05_tournament_cycle_runs_when_enabled_and_interval_elapsed() {
    let mut cfg = test_config();
    cfg.tournament_enabled = true;
    cfg.tournament_interval_secs = 1;

    let mut state = AgentLoopState::default();
    let previous = Instant::now() - Duration::from_secs(3);
    state.last_tournament_run = Some(previous);

    maybe_run_tournament_cycle(&cfg, &mut state).unwrap();

    let last_run = state
        .last_tournament_run
        .expect("expected tournament run timestamp");
    assert!(last_run > previous);
}

#[test]
fn tc_al_06_default_agent_loop_state_has_none_timestamps() {
    let state = AgentLoopState::default();

    assert!(state.last_aig_run.is_none());
    assert!(state.last_tournament_run.is_none());
}
