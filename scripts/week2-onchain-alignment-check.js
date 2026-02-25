const fs = require("fs");
const os = require("os");
const path = require("path");
const crypto = require("crypto");
const {
  Connection,
  PublicKey,
  Keypair,
  Transaction,
  TransactionInstruction,
  SystemProgram,
  LAMPORTS_PER_SOL,
  sendAndConfirmTransaction,
} = require("@solana/web3.js");
const {
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
  AuthorityType,
  getAssociatedTokenAddressSync,
  createAssociatedTokenAccountIdempotentInstruction,
  createSetAuthorityInstruction,
  getAccount,
  getMint,
} = require("@solana/spl-token");

const RPC_URL = "https://api.devnet.solana.com";
const PROGRAM_ID = new PublicKey("BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3");
const MSTB_MINT = new PublicKey("EZUwC88f1s3k9prgv5DGY6wML8giBqdpRxoA2rLtGA6R");
const USDC_MINT = new PublicKey("VLDKjAMvPXK2rGbhKynrShXgALfikwrwD517CxLRb8C");
const USDT_MINT = new PublicKey("6muD8Dtn4TVbENmXxVN7yoznwB2cVH9Y8cHNZ6hpvxJd");
const DAI_MINT = new PublicKey("6zTaf6yZ6HBFt2Bvi43fhoaXDid2dmLsCk64tq58CvZ4");
const USDS_MINT = new PublicKey("HtFssc7CKVTf67zDPMmK6LiurKgjHEWtaviqG24XKWjk");
const PYTH_USDC = new PublicKey("Dpw1EAVrSB1ibxiDQyTAW6Zip3J4Btk2x4SgApQCeFbX");

function disc(name) {
  return crypto.createHash("sha256").update(`global:${name}`).digest().subarray(0, 8);
}

function u64LE(v) {
  let n = BigInt(v);
  const out = Buffer.alloc(8);
  for (let i = 0; i < 8; i += 1) {
    out[i] = Number(n & 0xffn);
    n >>= 8n;
  }
  return out;
}

function encodeMint(collateralIndex, collateralAmount, maxPrice) {
  return Buffer.concat([
    disc("mint"),
    Buffer.from([Number(collateralIndex) & 0xff]),
    u64LE(collateralAmount),
    u64LE(maxPrice),
  ]);
}

function encodeRegisterAgent(role, stake) {
  return Buffer.concat([
    disc("register_agent"),
    Buffer.from([Number(role) & 0xff]),
    u64LE(stake),
  ]);
}

function encodeUpdateOraclePyth(collateralIndex) {
  return Buffer.concat([disc("update_oracle_pyth"), Buffer.from([Number(collateralIndex) & 0xff])]);
}

function encodeUpdateOracle(collateralIndex, price, confidence, observedSlot) {
  return Buffer.concat([
    disc("update_oracle"),
    Buffer.from([Number(collateralIndex) & 0xff]),
    u64LE(price),
    u64LE(confidence),
    u64LE(observedSlot),
  ]);
}

function loadKeypair(p) {
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(p, "utf8"))));
}

async function maybeAddAtaIx(connection, payer, ata, owner, mint, ixs) {
  const info = await connection.getAccountInfo(ata, "confirmed");
  if (!info) {
    ixs.push(
      createAssociatedTokenAccountIdempotentInstruction(
        payer,
        ata,
        owner,
        mint,
        TOKEN_PROGRAM_ID,
        ASSOCIATED_TOKEN_PROGRAM_ID
      )
    );
  }
}


async function transferLamports(connection, fromKp, toPubkey, lamports) {
  const tx = new Transaction().add(
    SystemProgram.transfer({ fromPubkey: fromKp.publicKey, toPubkey, lamports })
  );
  return sendAndConfirmTransaction(connection, tx, [fromKp], {
    commitment: "confirmed",
    preflightCommitment: "confirmed",
  });
}

async function airdropWithSingleRetry(connection, pubkey, lamports) {
  const out = { attempts: [], success: false, signature: null, error: null };
  for (let i = 1; i <= 2; i += 1) {
    try {
      const sig = await connection.requestAirdrop(pubkey, lamports);
      const latest = await connection.getLatestBlockhash("confirmed");
      await connection.confirmTransaction(
        { signature: sig, blockhash: latest.blockhash, lastValidBlockHeight: latest.lastValidBlockHeight },
        "confirmed"
      );
      out.attempts.push({ attempt: i, ok: true, signature: sig });
      out.success = true;
      out.signature = sig;
      return out;
    } catch (e) {
      const msg = String(e?.message || e);
      out.attempts.push({ attempt: i, ok: false, error: msg });
      const is429 = msg.includes("429") || msg.toLowerCase().includes("too many requests");
      if (!(is429 && i === 1)) {
        out.error = msg;
        return out;
      }
      await new Promise((r) => setTimeout(r, 1500));
    }
  }
  return out;
}

function isSeedMismatch(errMsg) {
  return errMsg.includes("ConstraintSeeds") && errMsg.includes("agent_escrow");
}

async function probeSeeds(connection, agent, agentRecord, role, stake, candidates) {
  const probes = [];
  for (const c of candidates) {
    const [agentEscrow] = PublicKey.findProgramAddressSync(c.seeds, PROGRAM_ID);
    const tx = new Transaction().add(
      new TransactionInstruction({
        programId: PROGRAM_ID,
        keys: [
          { pubkey: agent.publicKey, isSigner: true, isWritable: true },
          { pubkey: agentRecord, isSigner: false, isWritable: true },
          { pubkey: agentEscrow, isSigner: false, isWritable: true },
          { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        ],
        data: encodeRegisterAgent(role, stake),
      })
    );

    let errMsg = "";
    try {
      const sim = await connection.simulateTransaction(tx, [agent], "confirmed");
      const merged = `${JSON.stringify(sim.value.err || "")}\n${(sim.value.logs || []).join("\n")}`;
      errMsg = merged;
    } catch (e) {
      errMsg = String(e?.message || e);
    }

    probes.push({
      label: c.label,
      pda: agentEscrow.toBase58(),
      seedCount: c.seeds.length,
      seedMismatch: isSeedMismatch(errMsg),
      acceptedBySeedCheck: !isSeedMismatch(errMsg),
      errSnippet: errMsg.slice(0, 300),
    });
  }

  const selected = probes.find((p) => p.acceptedBySeedCheck) || null;
  return { probes, selected };
}

async function main() {
  const ts = new Date().toISOString().replace(/[-:]/g, "").replace(/\..+/, "").replace("T", "-");
  const logDir = path.join(process.cwd(), "scripts", "logs", `week2-onchain-alignment-${ts}`);
  fs.mkdirSync(logDir, { recursive: true });
  const resultPath = process.argv[2] || path.join(logDir, "result.json");

  const out = {
    startedAt: new Date().toISOString(),
    rpc: RPC_URL,
    programId: PROGRAM_ID.toBase58(),
    signer: null,
    authorityAlignment: null,
    mint: null,
    register: null,
    seedProbes: [],
    blockers: [],
    signerPath: null,
    keeperPath: null,
  };

  const connection = new Connection(RPC_URL, "confirmed");
  const primaryMainPath = path.join(os.homedir(), ".config", "solana", "devnet-keypair.json");
  const keeper1Path = "/home/spritz/microstable-keeper/keypairs/keeper1.json";
  let mainKp = loadKeypair(primaryMainPath);
  let mainSignerPath = primaryMainPath;

  const primaryBalance = await connection.getBalance(mainKp.publicKey, "confirmed");
  if (primaryBalance < 50_000_000 && fs.existsSync(keeper1Path)) {
    mainKp = loadKeypair(keeper1Path);
    mainSignerPath = keeper1Path;
  }

  const keeperPath = fs.existsSync("/tmp/keeper2.json")
    ? "/tmp/keeper2.json"
    : fs.existsSync("/home/spritz/microstable-keeper/keypairs/keeper2.json")
      ? "/home/spritz/microstable-keeper/keypairs/keeper2.json"
      : fs.existsSync("/tmp/keeper3.json")
        ? "/tmp/keeper3.json"
        : fs.existsSync("/home/spritz/microstable-keeper/keypairs/keeper3.json")
          ? "/home/spritz/microstable-keeper/keypairs/keeper3.json"
          : null;
  const keeperKp = keeperPath ? loadKeypair(keeperPath) : null;

  out.signerPath = mainSignerPath;
  out.signer = mainKp.publicKey.toBase58();
  out.keeperPath = keeperPath;

  const [protocolState] = PublicKey.findProgramAddressSync([Buffer.from("protocol_state")], PROGRAM_ID);
  const [circuitBreaker] = PublicKey.findProgramAddressSync([Buffer.from("circuit_breaker")], PROGRAM_ID);
  const [vaultUsdc] = PublicKey.findProgramAddressSync([Buffer.from("collateral_vault"), Buffer.from([0])], PROGRAM_ID);
  const [vaultUsdt] = PublicKey.findProgramAddressSync([Buffer.from("collateral_vault"), Buffer.from([1])], PROGRAM_ID);
  const [vaultDai] = PublicKey.findProgramAddressSync([Buffer.from("collateral_vault"), Buffer.from([2])], PROGRAM_ID);
  const [vaultUsds] = PublicKey.findProgramAddressSync([Buffer.from("collateral_vault"), Buffer.from([3])], PROGRAM_ID);
  const [userPosition] = PublicKey.findProgramAddressSync([Buffer.from("user_position"), mainKp.publicKey.toBuffer()], PROGRAM_ID);

  const userUsdcAta = getAssociatedTokenAddressSync(USDC_MINT, mainKp.publicKey);
  const userMstbAta = getAssociatedTokenAddressSync(MSTB_MINT, mainKp.publicKey);
  const vaultUsdcAta = getAssociatedTokenAddressSync(USDC_MINT, protocolState, true);

  // 1) Mint authority alignment check
  const mintInfo = await getMint(connection, MSTB_MINT, "confirmed");
  const beforeAuth = mintInfo.mintAuthority ? mintInfo.mintAuthority.toBase58() : null;
  const expectedAuth = protocolState.toBase58();
  const align = {
    expectedAuthority: expectedAuth,
    beforeAuthority: beforeAuth,
    signer: mainKp.publicKey.toBase58(),
    changed: false,
    signature: null,
    error: null,
  };

  if (!beforeAuth) {
    align.error = "MSTB mint authority is null";
  } else if (beforeAuth !== expectedAuth) {
    if (beforeAuth !== mainKp.publicKey.toBase58()) {
      align.error = `mint authority signer mismatch: current=${beforeAuth} signer=${mainKp.publicKey.toBase58()}`;
    } else {
      try {
        const tx = new Transaction().add(
          createSetAuthorityInstruction(
            MSTB_MINT,
            mainKp.publicKey,
            AuthorityType.MintTokens,
            protocolState,
            [],
            TOKEN_PROGRAM_ID
          )
        );
        const sig = await sendAndConfirmTransaction(connection, tx, [mainKp], {
          commitment: "confirmed",
          preflightCommitment: "confirmed",
        });
        align.changed = true;
        align.signature = sig;
      } catch (e) {
        align.error = String(e?.message || e);
      }
    }
  }

  const afterMintInfo = await getMint(connection, MSTB_MINT, "confirmed");
  align.afterAuthority = afterMintInfo.mintAuthority ? afterMintInfo.mintAuthority.toBase58() : null;
  align.ok = !align.error && align.afterAuthority === expectedAuth;
  out.authorityAlignment = align;
  if (!align.ok) out.blockers.push({ step: "mint_authority", error: align.error || "not aligned" });

  // 2) Oracle refresh best-effort
  let oracle = { attempted: false, ok: false, mode: null, signature: null, error: null };
  if (keeperKp) {
    oracle.attempted = true;
    try {
      const tx = new Transaction().add(
        new TransactionInstruction({
          programId: PROGRAM_ID,
          keys: [
            { pubkey: protocolState, isSigner: false, isWritable: true },
            { pubkey: circuitBreaker, isSigner: false, isWritable: true },
            { pubkey: vaultUsdc, isSigner: false, isWritable: true },
            { pubkey: vaultUsdt, isSigner: false, isWritable: true },
            { pubkey: vaultDai, isSigner: false, isWritable: true },
            { pubkey: vaultUsds, isSigner: false, isWritable: true },
            { pubkey: mainKp.publicKey, isSigner: true, isWritable: false },
            { pubkey: keeperKp.publicKey, isSigner: true, isWritable: false },
            { pubkey: PYTH_USDC, isSigner: false, isWritable: false },
          ],
          data: encodeUpdateOraclePyth(0),
        })
      );
      oracle.signature = await sendAndConfirmTransaction(connection, tx, [mainKp, keeperKp], {
        commitment: "confirmed",
        preflightCommitment: "confirmed",
      });
      oracle.ok = true;
      oracle.mode = "pyth";
    } catch (e1) {
      try {
        const slot = await connection.getSlot("confirmed");
        const tx = new Transaction().add(
          new TransactionInstruction({
            programId: PROGRAM_ID,
            keys: [
              { pubkey: protocolState, isSigner: false, isWritable: true },
              { pubkey: circuitBreaker, isSigner: false, isWritable: true },
              { pubkey: vaultUsdc, isSigner: false, isWritable: true },
              { pubkey: vaultUsdt, isSigner: false, isWritable: true },
              { pubkey: vaultDai, isSigner: false, isWritable: true },
              { pubkey: vaultUsds, isSigner: false, isWritable: true },
              { pubkey: mainKp.publicKey, isSigner: true, isWritable: false },
              { pubkey: keeperKp.publicKey, isSigner: true, isWritable: false },
            ],
            data: encodeUpdateOracle(0, 1_000_000n, 1_000n, BigInt(slot)),
          })
        );
        oracle.signature = await sendAndConfirmTransaction(connection, tx, [mainKp, keeperKp], {
          commitment: "confirmed",
          preflightCommitment: "confirmed",
        });
        oracle.ok = true;
        oracle.mode = "manual";
      } catch (e2) {
        oracle.error = `pyth=${String(e1?.message || e1)} | manual=${String(e2?.message || e2)}`;
      }
    }
  } else {
    oracle.error = "keeper keypair missing (/tmp/keeper2.json or /tmp/keeper3.json)";
  }

  // 3) Mint attempt
  try {
    let beforeUserMstbAmount = 0n;
    try {
      beforeUserMstbAmount = (await getAccount(connection, userMstbAta, "confirmed")).amount;
    } catch (_) {
      beforeUserMstbAmount = 0n;
    }
    const beforeSupply = (await getMint(connection, MSTB_MINT, "confirmed")).supply;

    const ixs = [];
    await maybeAddAtaIx(connection, mainKp.publicKey, userUsdcAta, mainKp.publicKey, USDC_MINT, ixs);
    await maybeAddAtaIx(connection, mainKp.publicKey, vaultUsdcAta, protocolState, USDC_MINT, ixs);
    await maybeAddAtaIx(connection, mainKp.publicKey, userMstbAta, mainKp.publicKey, MSTB_MINT, ixs);

    ixs.push(
      new TransactionInstruction({
        programId: PROGRAM_ID,
        keys: [
          { pubkey: protocolState, isSigner: false, isWritable: true },
          { pubkey: circuitBreaker, isSigner: false, isWritable: true },
          { pubkey: vaultUsdc, isSigner: false, isWritable: true },
          { pubkey: vaultUsdt, isSigner: false, isWritable: true },
          { pubkey: vaultDai, isSigner: false, isWritable: true },
          { pubkey: vaultUsds, isSigner: false, isWritable: true },
          { pubkey: mainKp.publicKey, isSigner: true, isWritable: true },
          { pubkey: userPosition, isSigner: false, isWritable: true },
          { pubkey: userUsdcAta, isSigner: false, isWritable: true },
          { pubkey: vaultUsdcAta, isSigner: false, isWritable: true },
          { pubkey: MSTB_MINT, isSigner: false, isWritable: true },
          { pubkey: userMstbAta, isSigner: false, isWritable: true },
          { pubkey: USDC_MINT, isSigner: false, isWritable: false },
          { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
          { pubkey: ASSOCIATED_TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
          { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        ],
        data: encodeMint(0, 100_000n, 2_000_000n),
      })
    );

    const tx = new Transaction().add(...ixs);
    const sig = await sendAndConfirmTransaction(connection, tx, [mainKp], {
      commitment: "confirmed",
      preflightCommitment: "confirmed",
    });

    const afterUserMstb = await getAccount(connection, userMstbAta, "confirmed");
    const afterSupply = (await getMint(connection, MSTB_MINT, "confirmed")).supply;

    out.mint = {
      ok: afterUserMstb.amount > beforeUserMstbAmount,
      signature: sig,
      mintedDeltaRaw: (afterUserMstb.amount - beforeUserMstbAmount).toString(),
      supplyDeltaRaw: (afterSupply - beforeSupply).toString(),
      oracle,
    };
  } catch (e) {
    out.mint = { ok: false, error: String(e?.message || e), oracle };
    out.blockers.push({ step: "mint", error: out.mint.error });
  }

  // 4) register_agent seed probe + execute
  try {
    const agent = Keypair.generate();
    const role = 1;
    const stake = 100_000n;
    const [agentRecord] = PublicKey.findProgramAddressSync([Buffer.from("agent"), agent.publicKey.toBuffer()], PROGRAM_ID);

    const candidates = [
      { label: "v2_wallet", seeds: [Buffer.from("v2:agent_escrow"), agent.publicKey.toBuffer()] },
      { label: "legacy_wallet", seeds: [Buffer.from("agent_escrow"), agent.publicKey.toBuffer()] },
      { label: "legacy_global", seeds: [Buffer.from("agent_escrow")] },
    ];

    const preProbeMin = 2_000_000;
    const preProbeBal = await connection.getBalance(agent.publicKey, "confirmed");
    if (preProbeBal < preProbeMin) {
      const top = Math.min(20_000_000, (await connection.getBalance(mainKp.publicKey, "confirmed")) - 10_000_000);
      if (top > 0) {
        try { await transferLamports(connection, mainKp, agent.publicKey, top); } catch (_) {}
      }
    }

    const probe = await probeSeeds(connection, agent, agentRecord, role, stake, candidates);
    out.seedProbes = probe.probes;
    if (!probe.selected) {
      throw new Error("register_agent seed alignment failed: all candidates hit ConstraintSeeds(agent_escrow)");
    }

    let bal = await connection.getBalance(agent.publicKey, "confirmed");
    const minNeed = Number(stake + 50_000_000n);
    let faucet = null;
    if (bal < minNeed) {
      faucet = await airdropWithSingleRetry(connection, agent.publicKey, 2 * LAMPORTS_PER_SOL);
      bal = await connection.getBalance(agent.publicKey, "confirmed");
    }
    if (bal < minNeed) {
      const donorBal = await connection.getBalance(mainKp.publicKey, "confirmed");
      const missing = minNeed - bal;
      if (donorBal > missing + 10_000_000) {
        try {
          await transferLamports(connection, mainKp, agent.publicKey, missing + 1_000_000);
          bal = await connection.getBalance(agent.publicKey, "confirmed");
        } catch (_) {}
      }
    }

    if (bal < minNeed) {
      throw new Error(
        `Insufficient SOL for register_agent. selectedSeed=${probe.selected.label} need=${minNeed} current=${bal} faucetError=${faucet?.error || "none"}`
      );
    }

    const ordered = [
      probe.selected,
      ...probe.probes.filter((p) => p.label !== probe.selected.label),
    ];

    let done = null;
    let lastErr = null;
    for (const p of ordered) {
      const c = candidates.find((x) => x.label === p.label);
      const [agentEscrow] = PublicKey.findProgramAddressSync(c.seeds, PROGRAM_ID);
      const tx = new Transaction().add(
        new TransactionInstruction({
          programId: PROGRAM_ID,
          keys: [
            { pubkey: agent.publicKey, isSigner: true, isWritable: true },
            { pubkey: agentRecord, isSigner: false, isWritable: true },
            { pubkey: agentEscrow, isSigner: false, isWritable: true },
            { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
          ],
          data: encodeRegisterAgent(role, stake),
        })
      );

      try {
        const sig = await sendAndConfirmTransaction(connection, tx, [agent], {
          commitment: "confirmed",
          preflightCommitment: "confirmed",
        });
        done = { sig, label: c.label, agentEscrow };
        break;
      } catch (e) {
        lastErr = e;
        const msg = String(e?.message || e);
        if (!isSeedMismatch(msg)) break;
      }
    }

    if (!done) throw lastErr || new Error("register_agent failed for all seed candidates");

    const recordInfo = await connection.getAccountInfo(agentRecord, "confirmed");
    const escrowInfo = await connection.getAccountInfo(done.agentEscrow, "confirmed");

    out.register = {
      ok: !!recordInfo,
      agent: agent.publicKey.toBase58(),
      role,
      stakeLamports: stake.toString(),
      selectedSeedLabel: done.label,
      signature: done.sig,
      agentRecord: agentRecord.toBase58(),
      agentEscrow: done.agentEscrow.toBase58(),
      escrowLamports: escrowInfo?.lamports ?? null,
    };
  } catch (e) {
    out.register = { ok: false, error: String(e?.message || e) };
    out.blockers.push({ step: "register_agent", error: out.register.error });
  }

  out.finishedAt = new Date().toISOString();
  out.ok = Boolean(out.mint?.ok && out.register?.ok);

  fs.mkdirSync(path.dirname(resultPath), { recursive: true });
  fs.writeFileSync(resultPath, JSON.stringify(out, null, 2));
  console.log(JSON.stringify({ ok: out.ok, resultPath, blockers: out.blockers }, null, 2));
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
