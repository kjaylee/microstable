use anyhow::{anyhow, Context, Result};
use borsh::BorshDeserialize;
use solana_client::{rpc_client::RpcClient, rpc_config::RpcSendTransactionConfig};
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    pubkey::Pubkey,
    signature::{read_keypair_file, Keypair, Signature, Signer},
    transaction::Transaction,
};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
};
use tracing::Level;
use tracing_subscriber::{fmt, EnvFilter};

#[derive(Debug, Clone)]
pub struct DerivedAccounts {
    pub protocol_state: Pubkey,
    pub circuit_breaker: Pubkey,
    pub vaults: [Pubkey; 4],
}

impl DerivedAccounts {
    pub fn derive(program_id: &Pubkey) -> Self {
        let (protocol_state, _) = Pubkey::find_program_address(&[b"protocol_state"], program_id);
        let (circuit_breaker, _) = Pubkey::find_program_address(&[b"circuit_breaker"], program_id);
        let vaults =
            [0u8, 1, 2, 3].map(|i| Pubkey::find_program_address(&[b"collateral_vault", &[i]], program_id).0);

        Self {
            protocol_state,
            circuit_breaker,
            vaults,
        }
    }
}

pub fn init_tracing(json_logs: bool) {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::builder()
            .with_default_directive(Level::INFO.into())
            .from_env_lossy()
    });

    let formatter = fmt().with_env_filter(env_filter).with_target(false);
    if json_logs {
        formatter.json().flatten_event(true).init();
    } else {
        formatter.compact().init();
    }
}

pub fn expand_tilde(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    if !raw.starts_with("~/") {
        return path.to_path_buf();
    }

    let home = std::env::var("HOME").unwrap_or_else(|_| String::from("/"));
    PathBuf::from(home).join(raw.trim_start_matches("~/"))
}

pub fn load_keypairs(paths: &[PathBuf]) -> Result<Vec<Keypair>> {
    let mut out = Vec::with_capacity(paths.len());
    let mut seen_pubkeys = HashSet::new();

    for path in paths {
        let expanded = expand_tilde(path);
        validate_keypair_file_security(&expanded)?;

        let kp = read_keypair_file(&expanded)
            .map_err(|e| anyhow!("failed to read keypair {}: {e}", expanded.display()))?;

        if !seen_pubkeys.insert(kp.pubkey()) {
            return Err(anyhow!(
                "duplicate keeper keypair detected in config: {}",
                kp.pubkey()
            ));
        }

        out.push(kp);
    }

    Ok(out)
}

fn validate_keypair_file_security(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to stat keypair path: {}", path.display()))?;

    if metadata.file_type().is_symlink() {
        return Err(anyhow!(
            "refusing symlinked keypair file: {}",
            path.display()
        ));
    }

    if !metadata.is_file() {
        return Err(anyhow!("keypair path is not a file: {}", path.display()));
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};

        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            return Err(anyhow!(
                "insecure keypair file mode {:o} for {} (must not be group/world accessible)",
                mode,
                path.display()
            ));
        }

        let owner_uid = metadata.uid();
        let effective_uid = effective_uid()?;
        if owner_uid != effective_uid {
            return Err(anyhow!(
                "keypair file {} owned by uid {}, expected uid {}",
                path.display(),
                owner_uid,
                effective_uid
            ));
        }
    }

    Ok(())
}

#[cfg(unix)]
fn effective_uid() -> Result<u32> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("failed to execute `id -u` for keypair owner validation")?;

    if !output.status.success() {
        return Err(anyhow!(
            "`id -u` failed while validating keypair ownership"
        ));
    }

    let raw = String::from_utf8(output.stdout).context("`id -u` returned non-utf8 output")?;
    let uid = raw
        .trim()
        .parse::<u32>()
        .context("failed to parse uid from `id -u` output")?;
    Ok(uid)
}

pub fn keeper_quorum_for_protocol<'a>(
    keepers: &'a [Keypair],
    protocol_keeper_set: &[Pubkey; 3],
) -> Result<(&'a Keypair, &'a Keypair)> {
    let mut members: Vec<&Keypair> = Vec::new();

    for kp in keepers {
        let pubkey = kp.pubkey();
        if protocol_keeper_set.iter().any(|k| *k == pubkey)
            && !members.iter().any(|existing| existing.pubkey() == pubkey)
        {
            members.push(kp);
        }
    }

    if members.len() < 2 {
        return Err(anyhow!(
            "configured keypairs do not satisfy protocol keeper quorum; protocol set={:?}",
            protocol_keeper_set
        ));
    }

    Ok((members[0], members[1]))
}

pub fn verify_program_deployed(rpc: &RpcClient, program_id: &Pubkey) -> Result<()> {
    let account = rpc
        .get_account(program_id)
        .with_context(|| format!("program account not found: {program_id}"))?;
    if !account.executable {
        return Err(anyhow!(
            "program account exists but is not executable: {program_id}"
        ));
    }
    Ok(())
}

pub fn fetch_account<T: BorshDeserialize>(
    rpc: &RpcClient,
    address: &Pubkey,
    account_name: &str,
) -> Result<T> {
    let account = rpc
        .get_account(address)
        .with_context(|| format!("failed to fetch account: {address}"))?;
    crate::wire::decode_account::<T>(&account.data, account_name)
        .with_context(|| format!("failed to decode {account_name} at {address}"))
}

pub fn send_instructions(
    rpc: &RpcClient,
    payer: &Keypair,
    signers: &[&Keypair],
    instructions: Vec<Instruction>,
) -> Result<Signature> {
    if instructions.is_empty() {
        return Err(anyhow!("no instructions to send"));
    }

    let blockhash = rpc
        .get_latest_blockhash()
        .context("failed to fetch latest blockhash")?;
    let tx = Transaction::new_signed_with_payer(
        &instructions,
        Some(&payer.pubkey()),
        signers,
        blockhash,
    );

    let sig = rpc
        .send_and_confirm_transaction_with_spinner_and_config(
            &tx,
            CommitmentConfig::confirmed(),
            RpcSendTransactionConfig {
                skip_preflight: false,
                preflight_commitment: Some(CommitmentConfig::processed().commitment),
                ..RpcSendTransactionConfig::default()
            },
        )
        .context("failed to send transaction")?;

    Ok(sig)
}

pub fn parse_pubkey(raw: &str) -> Result<Pubkey> {
    Pubkey::from_str(raw).with_context(|| format!("invalid pubkey: {raw}"))
}
