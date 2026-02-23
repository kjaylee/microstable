#![cfg(test)]

use std::{path::Path, str::FromStr};

use solana_sdk::pubkey::Pubkey;

#[test]
fn tc_mp_01_pm2_default_home_variants_are_detected_as_shared() {
    assert!(super::is_default_pm2_home(
        Path::new("~/.pm2"),
        Some(Path::new("/home/spritz"))
    ));

    assert!(super::is_default_pm2_home(
        Path::new("$HOME/.pm2"),
        Some(Path::new("/home/spritz"))
    ));

    assert!(super::is_default_pm2_home(
        Path::new("/home/spritz/.pm2"),
        Some(Path::new("/home/spritz"))
    ));
}

#[test]
fn tc_mp_02_dedicated_pm2_home_is_not_treated_as_shared() {
    assert!(!super::is_default_pm2_home(
        Path::new("/home/spritz/.pm2-keeper"),
        Some(Path::new("/home/spritz"))
    ));

    assert!(!super::is_default_pm2_home(
        Path::new("/opt/custom/pm2"),
        None
    ));
}

#[test]
fn tc_mp_03_pm2_default_home_with_trailing_slash_is_detected_as_shared() {
    assert!(super::is_default_pm2_home(
        Path::new("/home/spritz/.pm2/"),
        Some(Path::new("/home/spritz"))
    ));
}

#[test]
fn tc_mp_04_rebalance_eligibility_requires_active_and_tier_two() {
    let active_tier_two = super::wire::AgentRecord {
        agent: Pubkey::from_str("11111111111111111111111111111111").expect("valid pubkey"),
        stake: 1,
        reputation: 1,
        role: super::wire::AgentRole::Monitor,
        tier: 2,
        status: super::wire::AgentStatus::Active,
        proposals_submitted: 0,
        proposals_accepted: 0,
        registered_at: 0,
        last_active_at: 0,
        agent_score: 0,
        bump: 255,
    };

    let mut cooldown = active_tier_two.clone();
    cooldown.status = super::wire::AgentStatus::Cooldown;

    let mut tier_one = active_tier_two.clone();
    tier_one.tier = 1;

    assert!(super::agent_record_is_rebalance_eligible(&active_tier_two));
    assert!(!super::agent_record_is_rebalance_eligible(&cooldown));
    assert!(!super::agent_record_is_rebalance_eligible(&tier_one));
}

#[test]
fn tc_mp_05_rebalance_guidance_mentions_registration_and_promotion() {
    let guidance = super::rebalance_preflight_instructions();

    assert!(guidance.contains("register-agents.ts"));
    assert!(guidance.contains("update_agent_score"));
    assert!(guidance.contains("promote_agent"));
}

#[cfg(unix)]
#[test]
fn tc_mp_06_pm2_symlink_path_is_detected_as_shared() {
    use std::{
        fs,
        os::unix::fs::symlink,
        time::{SystemTime, UNIX_EPOCH},
    };

    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before epoch")
        .as_nanos();

    let root = std::env::temp_dir().join(format!("microstable-pm2-test-{suffix}"));
    let home = root.join("home");
    let default_pm2 = home.join(".pm2");
    let symlink_path = home.join("pm2-link");

    fs::create_dir_all(&default_pm2).expect("create default pm2 path");
    symlink(&default_pm2, &symlink_path).expect("create symlink");

    let shared = super::is_default_pm2_home(&symlink_path, Some(&home));

    fs::remove_file(&symlink_path).ok();
    fs::remove_dir_all(&root).ok();

    assert!(shared);
}

#[cfg(unix)]
#[test]
fn tc_mp_07_env_mode_must_be_exactly_600() {
    assert!(super::has_restrictive_env_permissions(0o600));
    assert!(!super::has_restrictive_env_permissions(0o640));
    assert!(!super::has_restrictive_env_permissions(0o644));
}
