#![cfg(test)]

use std::path::Path;

#[test]
fn tc_mp_01_pm2_default_home_is_detected_as_shared() {
    assert!(super::is_default_pm2_home(
        Path::new("~/.pm2"),
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

#[cfg(unix)]
#[test]
fn tc_mp_03_env_mode_must_be_exactly_600() {
    assert!(super::has_restrictive_env_permissions(0o600));
    assert!(!super::has_restrictive_env_permissions(0o640));
    assert!(!super::has_restrictive_env_permissions(0o644));
}
