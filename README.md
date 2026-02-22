# Microstable

Self-evolving multi-collateral stablecoin protocol on Solana.

## Architecture
- `solana/programs/` — On-chain Anchor program
- `solana/keeper/` — Rust keeper daemon (oracle updates, CB monitoring, rebalancing)
- `solana/tests/` — Anchor integration tests
- `simulation/` — Archived Python simulation (reference only)
- `docs/` — Security reports, whitepaper, specs

## Program
- Program ID: `BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3`
- Main on-chain code: `solana/programs/microstable/src/lib.rs`

## Keeper (devnet-first)
```bash
cd solana
cargo run -p microstable-keeper -- --config keeper/config.devnet.json
```

## Tests
Keeper unit + integration tests:
```bash
cd solana/keeper
cargo test -p microstable-keeper
```
Last verified (2026-02-23): **123** total tests (58 unit + 65 integration).

## Simulation Archive
Python simulation and red-team suites are preserved under `simulation/` for historical reference.
They are no longer production runtime components.
