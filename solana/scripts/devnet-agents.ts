// @ts-nocheck
import * as anchor from "@coral-xyz/anchor";
import { PublicKey, SystemProgram, Connection, Keypair } from "@solana/web3.js";
import BN from "bn.js";
import fs from "fs";
import path from "path";

const PROGRAM_ID = new PublicKey("BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3");
const RPC_URL = "https://api.devnet.solana.com";
const SCALE = 1_000_000;

const ROOT_DIR = path.resolve(__dirname, "..", "..");
const OUTPUT_DIR = path.join(ROOT_DIR, "outputs");
const LOG_PATH = path.join(OUTPUT_DIR, "devnet-agent-run.log");
const SUMMARY_PATH = path.join(OUTPUT_DIR, "devnet-agents-summary.json");

const DEPLOY_KEYPAIR_PATH = "/Users/kjaylee/.config/solana/devnet-keypair.json";
const KEEPER_KEYPAIR_PATH = path.join(ROOT_DIR, "wallets", "keeper-agent.json");
const WATCHDOG_KEYPAIR_PATH = path.join(ROOT_DIR, "wallets", "watchdog-agent.json");
const AUDITOR_KEYPAIR_PATH = path.join(ROOT_DIR, "wallets", "auditor-agent.json");

function loadKeypair(filePath: string): Keypair {
  const raw = JSON.parse(fs.readFileSync(filePath, "utf-8"));
  return Keypair.fromSecretKey(Uint8Array.from(raw));
}

function toNum(v: any): number {
  if (v == null) return 0;
  if (typeof v === "number") return v;
  if (typeof v === "bigint") return Number(v);
  if (v.toNumber) return v.toNumber();
  if (v.toString) return Number(v.toString());
  return Number(v);
}

function extractError(err: any): string {
  if (!err) return "Unknown error";
  if (err.error?.errorMessage) return err.error.errorMessage;
  if (err.message) return err.message;
  return String(err);
}

function serializeError(err: any) {
  return {
    message: extractError(err),
    logs: err?.logs ?? null,
    code: err?.code ?? null,
  };
}

async function main() {
  fs.mkdirSync(OUTPUT_DIR, { recursive: true });
  fs.writeFileSync(LOG_PATH, "", "utf-8");

  const log = (line: string) => {
    const stamped = `[${new Date().toISOString()}] ${line}`;
    console.log(stamped);
    fs.appendFileSync(LOG_PATH, `${stamped}\n`, "utf-8");
  };

  const idlPath = path.join(__dirname, "..", "target", "idl", "microstable.json");
  const idl = JSON.parse(fs.readFileSync(idlPath, "utf-8"));

  const connection = new Connection(RPC_URL, "confirmed");
  const deploy = loadKeypair(DEPLOY_KEYPAIR_PATH);
  const keeper = loadKeypair(KEEPER_KEYPAIR_PATH);
  const watchdog = loadKeypair(WATCHDOG_KEYPAIR_PATH);
  const auditor = loadKeypair(AUDITOR_KEYPAIR_PATH);

  const provider = new anchor.AnchorProvider(
    connection,
    new anchor.Wallet(deploy),
    { commitment: "confirmed" }
  );
  anchor.setProvider(provider);

  const program = new anchor.Program(idl, provider) as any;

  const [protocolState] = PublicKey.findProgramAddressSync(
    [Buffer.from("protocol_state")],
    PROGRAM_ID
  );
  const [circuitBreaker] = PublicKey.findProgramAddressSync(
    [Buffer.from("circuit_breaker")],
    PROGRAM_ID
  );
  const [vaultUsdc] = PublicKey.findProgramAddressSync(
    [Buffer.from("collateral_vault"), Buffer.from([0])],
    PROGRAM_ID
  );
  const [vaultUsdt] = PublicKey.findProgramAddressSync(
    [Buffer.from("collateral_vault"), Buffer.from([1])],
    PROGRAM_ID
  );
  const [vaultDai] = PublicKey.findProgramAddressSync(
    [Buffer.from("collateral_vault"), Buffer.from([2])],
    PROGRAM_ID
  );
  const [vaultUsds] = PublicKey.findProgramAddressSync(
    [Buffer.from("collateral_vault"), Buffer.from([3])],
    PROGRAM_ID
  );

  const [keeperUserPosition] = PublicKey.findProgramAddressSync(
    [Buffer.from("user_position"), keeper.publicKey.toBuffer()],
    PROGRAM_ID
  );
  const [auditorUserPosition] = PublicKey.findProgramAddressSync(
    [Buffer.from("user_position"), auditor.publicKey.toBuffer()],
    PROGRAM_ID
  );

  const txRecords: any[] = [];

  async function fetchState() {
    const protocol = await program.account.protocolState.fetchNullable(protocolState);
    const cb = await program.account.circuitBreakerState.fetchNullable(circuitBreaker);
    const usdc = await program.account.collateralVault.fetchNullable(vaultUsdc);
    const usdt = await program.account.collateralVault.fetchNullable(vaultUsdt);
    const dai = await program.account.collateralVault.fetchNullable(vaultDai);
    const usds = await program.account.collateralVault.fetchNullable(vaultUsds);

    if (!protocol) {
      return {
        initialized: false,
        totalSupply: 0,
        weights: [],
        crTarget: 0,
        collateralValue: 0,
        crRatio: null,
        circuitStatus: null,
      };
    }

    const vaults = [usdc, usdt, dai, usds].filter(Boolean);
    const collateralValue = vaults.reduce((acc: number, v: any) => {
      const deposits = toNum(v.totalDeposits);
      const price = toNum(v.price);
      return acc + Math.floor((deposits * price) / SCALE);
    }, 0);

    const totalSupply = toNum(protocol.totalSupply);
    const crRatio = totalSupply > 0
      ? Math.floor((collateralValue * SCALE) / totalSupply)
      : null;

    return {
      initialized: true,
      totalSupply,
      weights: (protocol.weights || []).map((w: any) => toNum(w)),
      crTarget: toNum(protocol.crTarget),
      collateralValue,
      crRatio,
      circuitStatus: cb ? cb.status.map((x: any) => Number(x)) : null,
    };
  }

  async function logState(label: string) {
    const s = await fetchState();
    if (!s.initialized) {
      log(`[STATE][${label}] protocol not initialized`);
      return;
    }
    const crText = s.crRatio == null ? "N/A" : `${s.crRatio}`;
    log(
      `[STATE][${label}] totalSupply=${s.totalSupply}, weights=[${s.weights.join(","
      )}], crTarget=${s.crTarget}, collateralValue=${s.collateralValue}, CR=${crText}, circuitStatus=${JSON.stringify(
        s.circuitStatus
      )}`
    );
  }

  async function runStep(step: string, actor: string, fn: () => Promise<string>) {
    try {
      const sig = await fn();
      log(`[OK][${step}] actor=${actor} sig=${sig}`);
      txRecords.push({ step, actor, status: "ok", signature: sig });
    } catch (err: any) {
      const e = serializeError(err);
      log(`[ERR][${step}] actor=${actor} msg=${e.message}`);
      if (e.logs) {
        log(`[ERR][${step}] logs=${JSON.stringify(e.logs)}`);
      }
      txRecords.push({ step, actor, status: "error", error: e });
    }
    await logState(step);
  }

  log(`Program ID: ${PROGRAM_ID.toBase58()}`);
  log(`Protocol PDA: ${protocolState.toBase58()}`);
  log(`Keeper Agent: ${keeper.publicKey.toBase58()}`);
  log(`Watchdog Agent: ${watchdog.publicKey.toBase58()}`);
  log(`Auditor Agent: ${auditor.publicKey.toBase58()}`);

  const initAccountInfo = await connection.getAccountInfo(protocolState);
  if (initAccountInfo) {
    log("[SKIP][initialize] protocol_state already exists (already initialized)");
    txRecords.push({ step: "initialize", actor: "keeper", status: "skipped", reason: "already_initialized" });
    await logState("initialize");
  } else {
    await runStep("initialize", "keeper", async () => {
      const sig = await program.methods
        .initialize()
        .accounts({
          protocolState,
          circuitBreaker,
          vaultUsdc,
          vaultUsdt,
          vaultDai,
          vaultUsds,
          authority: keeper.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([keeper])
        .rpc();
      return sig;
    });
  }

  for (let i = 0; i < 4; i += 1) {
    await runStep(`update_oracle_${i}`, "keeper", async () => {
      const slot = await connection.getSlot("confirmed");
      const sig = await program.methods
        .updateOracle(i, new BN(1_000_000), new BN(1_000), new BN(slot))
        .accounts({
          protocolState,
          circuitBreaker,
          vaultUsdc,
          vaultUsdt,
          vaultDai,
          vaultUsds,
          keeper: keeper.publicKey,
        })
        .remainingAccounts([
          { pubkey: watchdog.publicKey, isSigner: true, isWritable: false },
        ])
        .signers([keeper, watchdog])
        .rpc();
      return sig;
    });
  }

  await runStep("mint", "keeper", async () => {
    const sig = await program.methods
      .mint(0, new BN(1_000_000))
      .accounts({
        protocolState,
        circuitBreaker,
        vaultUsdc,
        vaultUsdt,
        vaultDai,
        vaultUsds,
        user: keeper.publicKey,
        userPosition: keeperUserPosition,
        systemProgram: SystemProgram.programId,
      })
      .signers([keeper])
      .rpc();
    return sig;
  });

  await runStep("rebalance", "keeper", async () => {
    const sig = await program.methods
      .rebalance([
        new BN(390_000),
        new BN(310_000),
        new BN(200_000),
        new BN(100_000),
      ])
      .accounts({
        protocolState,
        circuitBreaker,
        vaultUsdc,
        vaultUsdt,
        vaultDai,
        vaultUsds,
        keeper: keeper.publicKey,
      })
      .remainingAccounts([
        { pubkey: watchdog.publicKey, isSigner: true, isWritable: false },
      ])
      .signers([keeper, watchdog])
      .rpc();
    return sig;
  });

  await runStep("depeg_oracle", "keeper", async () => {
    const slot = await connection.getSlot("confirmed");
    const sig = await program.methods
      .updateOracle(0, new BN(970_000), new BN(1_000), new BN(slot))
      .accounts({
        protocolState,
        circuitBreaker,
        vaultUsdc,
        vaultUsdt,
        vaultDai,
        vaultUsds,
        keeper: keeper.publicKey,
      })
      .signers([keeper])
      .rpc();
    return sig;
  });

  await runStep("activate_circuit_breaker", "watchdog", async () => {
    const sig = await program.methods
      .activateCircuitBreaker(1, 0)
      .accounts({
        protocolState,
        circuitBreaker,
        vaultUsdc,
        vaultUsdt,
        vaultDai,
        vaultUsds,
        keeper: watchdog.publicKey,
      })
      .remainingAccounts([
        { pubkey: keeper.publicKey, isSigner: true, isWritable: false },
      ])
      .signers([watchdog, keeper])
      .rpc();
    return sig;
  });

  for (let i = 0; i < 6; i += 1) {
    await runStep(`recovery_oracle_${i}`, "keeper", async () => {
      const slot = await connection.getSlot("confirmed");
      const sig = await program.methods
        .updateOracle(0, new BN(1_000_000), new BN(1_000), new BN(slot))
        .accounts({
          protocolState,
          circuitBreaker,
          vaultUsdc,
          vaultUsdt,
          vaultDai,
          vaultUsds,
          keeper: keeper.publicKey,
        })
        .signers([keeper])
        .rpc();
      return sig;
    });
  }

  await runStep("recover_circuit_breaker", "watchdog", async () => {
    const sig = await program.methods
      .recoverCircuitBreaker(1)
      .accounts({
        protocolState,
        circuitBreaker,
        vaultUsdc,
        vaultUsdt,
        vaultDai,
        vaultUsds,
        keeper: watchdog.publicKey,
      })
      .remainingAccounts([
        { pubkey: keeper.publicKey, isSigner: true, isWritable: false },
      ])
      .signers([watchdog, keeper])
      .rpc();
    return sig;
  });

  await runStep("redeem", "auditor", async () => {
    const sig = await program.methods
      .redeem(new BN(500_000))
      .accounts({
        protocolState,
        circuitBreaker,
        vaultUsdc,
        vaultUsdt,
        vaultDai,
        vaultUsds,
        user: auditor.publicKey,
        userPosition: auditorUserPosition,
      })
      .signers([auditor])
      .rpc();
    return sig;
  });

  const summary = {
    cluster: RPC_URL,
    programId: PROGRAM_ID.toBase58(),
    agents: {
      keeper: keeper.publicKey.toBase58(),
      watchdog: watchdog.publicKey.toBase58(),
      auditor: auditor.publicKey.toBase58(),
    },
    transactions: txRecords,
    generatedAt: new Date().toISOString(),
  };

  fs.writeFileSync(SUMMARY_PATH, JSON.stringify(summary, null, 2), "utf-8");
  log(`Summary written: ${SUMMARY_PATH}`);
  log(`Run log written: ${LOG_PATH}`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
