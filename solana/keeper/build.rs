use sha2::{Digest, Sha256};
use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set for keeper build"),
    );
    let lock_path = manifest_dir.join("../../Cargo.lock");

    println!("cargo:rerun-if-changed={}", lock_path.display());

    let lockfile_bytes = fs::read(&lock_path).unwrap_or_else(|err| {
        panic!(
            "failed to read Cargo.lock at {}: {err}",
            lock_path.display()
        )
    });

    let mut hasher = Sha256::new();
    hasher.update(lockfile_bytes);
    let lock_hash = format!("{:x}", hasher.finalize());

    println!("cargo:rustc-env=KEEPER_CARGO_LOCK_HASH={lock_hash}");
}
