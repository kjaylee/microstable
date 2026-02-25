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
  getAssociatedTokenAddressSync,
  createAssociatedTokenAccountIdempotentInstruction,
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

const out = {
  startedAt: new Date().toISOString(),
  rpc: RPC_URL,
  programId: PROGRAM_ID.toBase58(),
  steps: [],
  mint: null,
  redeem: null,
  register: null,
  blockers: [],
};

function logStep(name, data) {
  const item = { name, ts: new Date().toISOString(), ...data };
  out.steps.push(item);
  console.log(`\n[${name}]`, JSON.stringify(data, null, 2));
}

function loadKeypair(filePath) {
  const raw = fs.readFileSync(filePath, "utf8");
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(raw)));
}

function disc(name) {
  return crypto.createHash("sha256").update(`global:${name}`).digest().subarray(0, 8);
}

function u64LE(value) {
  let v = BigInt(value);
  const b = Buffer.alloc(8);
  for (let i = 0; i < 8; i++) {
    b[i] = Number(v & 0xffn);
    v >>= 8n;
  }
  return b;
}

function encodeMintData(collateralIndex, collateralAmount, maxPrice) {
  return Buffer.concat([
    disc("mint"),
    Buffer.from([Number(collateralIndex) & 0xff]),
    u64LE(collateralAmount),
    u64LE(maxPrice),
  ]);
}

function encodeRedeemData(musdAmount, minOutAmount) {
  return Buffer.concat([disc("redeem"), u64LE(musdAmount), u64LE(minOutAmount)]);
}

function encodeRegisterAgentData(role, stakeLamports) {
  return Buffer.concat([
    disc("register_agent"),
    Buffer.from([Number(role) & 0xff]),
    u64LE(stakeLamports),
  ]);
}

async function maybeAddAtaIx(connection, payer, ata, owner, mint, instructions) {
  const info = await connection.getAccountInfo(ata, "confirmed");
  if (!info) {
    instructions.push(
      createAssociatedTokenAccountIdempotentInstruction(payer, ata, owner, mint, TOKEN_PROGRAM_ID, ASSOCIATED_TOKEN_PROGRAM_ID)
    );
  }
}

async function withAirdropIfNeeded(connection, kp, minLamports) {
  let bal = await connection.getBalance(kp.publicKey, "confirmed");
  if (bal >= minLamports) return { before: bal, after: bal, airdropSig: null, airdropError: null };

  try {
    const sig = await connection.requestAirdrop(kp.publicKey, 2 * LAMPORTS_PER_SOL);
    const latest = await connection.getLatestBlockhash("confirmed");
    await connection.confirmTransaction(
      { signature: sig, blockhash: latest.blockhash, lastValidBlockHeight: latest.lastValidBlockHeight },
      "confirmed"
    );

    bal = await connection.getBalance(kp.publicKey, "confirmed");
    return { before: null, after: bal, airdropSig: sig, airdropError: null };
  } catch (e) {
    bal = await connection.getBalance(kp.publicKey, "confirmed");
    return {
      before: null,
      after: bal,
      airdropSig: null,
      airdropError: String(e?.message || e),
    };
  }
}

async function transferLamports(connection, fromKp, toPubkey, lamports) {
  const tx = new Transaction().add(
    SystemProgram.transfer({
      fromPubkey: fromKp.publicKey,
      toPubkey,
      lamports,
    })
  );
  return sendAndConfirmTransaction(connection, tx, [fromKp], {
    commitment: "confirmed",
    preflightCommitment: "confirmed",
  });
}

async function fundFromLocalDonors(connection, receiver, minLamports) {
  const donorFiles = [
    "wallets/auditor-agent.json",
    "wallets/keeper-agent.json",
    "wallets/watchdog-agent.json",
  ];

  const transfers = [];
  for (const file of donorFiles) {
    const full = path.resolve(process.cwd(), file);
    if (!fs.existsSync(full)) continue;
    const donor = loadKeypair(full);
    const bal = await connection.getBalance(donor.publicKey, "confirmed");
    const send = 350_000_000; // 0.35 SOL
    if (bal <= send + 10_000_000) {
      transfers.push({ donor: donor.publicKey.toBase58(), skipped: true, balance: bal });
      continue;
    }
    const sig = await transferLamports(connection, donor, receiver, send);
    transfers.push({ donor: donor.publicKey.toBase58(), sentLamports: send, signature: sig });

    const now = await connection.getBalance(receiver, "confirmed");
    if (now >= minLamports) break;
  }

  const finalBal = await connection.getBalance(receiver, "confirmed");
  return { transfers, finalBalance: finalBal };
}

async function main() {
  const connection = new Connection(RPC_URL, "confirmed");
  const mainKp = loadKeypair(path.join(os.homedir(), ".config/solana/devnet-keypair.json"));

  const [protocolState] = PublicKey.findProgramAddressSync([Buffer.from("protocol_state")], PROGRAM_ID);
  const [circuitBreaker] = PublicKey.findProgramAddressSync([Buffer.from("circuit_breaker")], PROGRAM_ID);
  const [vaultUsdc] = PublicKey.findProgramAddressSync([Buffer.from("collateral_vault"), Buffer.from([0])], PROGRAM_ID);
  const [vaultUsdt] = PublicKey.findProgramAddressSync([Buffer.from("collateral_vault"), Buffer.from([1])], PROGRAM_ID);
  const [vaultDai] = PublicKey.findProgramAddressSync([Buffer.from("collateral_vault"), Buffer.from([2])], PROGRAM_ID);
  const [vaultUsds] = PublicKey.findProgramAddressSync([Buffer.from("collateral_vault"), Buffer.from([3])], PROGRAM_ID);
  const [userPosition] = PublicKey.findProgramAddressSync([Buffer.from("user_position"), mainKp.publicKey.toBuffer()], PROGRAM_ID);

  const userUsdcAta = getAssociatedTokenAddressSync(USDC_MINT, mainKp.publicKey);
  const userUsdtAta = getAssociatedTokenAddressSync(USDT_MINT, mainKp.publicKey);
  const userDaiAta = getAssociatedTokenAddressSync(DAI_MINT, mainKp.publicKey);
  const userUsdsAta = getAssociatedTokenAddressSync(USDS_MINT, mainKp.publicKey);
  const userMstbAta = getAssociatedTokenAddressSync(MSTB_MINT, mainKp.publicKey);

  const vaultUsdcAta = getAssociatedTokenAddressSync(USDC_MINT, protocolState, true);
  const vaultUsdtAta = getAssociatedTokenAddressSync(USDT_MINT, protocolState, true);
  const vaultDaiAta = getAssociatedTokenAddressSync(DAI_MINT, protocolState, true);
  const vaultUsdsAta = getAssociatedTokenAddressSync(USDS_MINT, protocolState, true);

  const prog = await connection.getAccountInfo(PROGRAM_ID, "confirmed");
  logStep("program-account", {
    exists: !!prog,
    executable: !!prog?.executable,
    owner: prog?.owner?.toBase58(),
    lamports: prog?.lamports,
  });

  const mainSol = await connection.getBalance(mainKp.publicKey, "confirmed");
  const beforeUsdc = await getAccount(connection, userUsdcAta, "confirmed");
  const beforeMstb = await getAccount(connection, userMstbAta, "confirmed");
  const beforeMstbMint = await getMint(connection, MSTB_MINT, "confirmed");

  logStep("pre-balances", {
    wallet: mainKp.publicKey.toBase58(),
    sol: mainSol,
    usdcRaw: beforeUsdc.amount.toString(),
    mstbRaw: beforeMstb.amount.toString(),
    mstbSupplyRaw: beforeMstbMint.supply.toString(),
    userPosition: userPosition.toBase58(),
  });

  const mintAmount = 100_000n; // 0.1 USDC (6 decimals)
  const mintMaxPrice = 2_000_000n;

  try {
    const ixs = [];
    await maybeAddAtaIx(connection, mainKp.publicKey, userUsdcAta, mainKp.publicKey, USDC_MINT, ixs);
    await maybeAddAtaIx(connection, mainKp.publicKey, vaultUsdcAta, protocolState, USDC_MINT, ixs);
    await maybeAddAtaIx(connection, mainKp.publicKey, userMstbAta, mainKp.publicKey, MSTB_MINT, ixs);

    const mintIx = new TransactionInstruction({
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
      data: encodeMintData(0, mintAmount, mintMaxPrice),
    });

    const tx = new Transaction().add(...ixs, mintIx);
    const sig = await sendAndConfirmTransaction(connection, tx, [mainKp], {
      commitment: "confirmed",
      preflightCommitment: "confirmed",
    });

    const afterMstb = await getAccount(connection, userMstbAta, "confirmed");
    const afterMstbMint = await getMint(connection, MSTB_MINT, "confirmed");
    const mintedDelta = afterMstb.amount - beforeMstb.amount;
    const mintedSupplyDelta = afterMstbMint.supply - beforeMstbMint.supply;

    out.mint = {
      ok: mintedDelta > 0n,
      signature: sig,
      amountInRaw: mintAmount.toString(),
      mintedDeltaRaw: mintedDelta.toString(),
      mintedSupplyDeltaRaw: mintedSupplyDelta.toString(),
      explorer: `https://explorer.solana.com/tx/${sig}?cluster=devnet`,
    };
    logStep("mint", out.mint);
  } catch (e) {
    out.mint = { ok: false, error: String(e?.message || e) };
    out.blockers.push({ step: "mint", error: out.mint.error });
    logStep("mint-failed", out.mint);
  }

  let redeemAmount = 0n;
  try {
    const curMstb = await getAccount(connection, userMstbAta, "confirmed");
    redeemAmount = out.mint?.ok ? BigInt(out.mint.mintedDeltaRaw) : curMstb.amount > 0n ? curMstb.amount : 0n;

    if (redeemAmount <= 0n) {
      throw new Error("No MSTB balance available for redeem test");
    }

    const ixs = [];
    await maybeAddAtaIx(connection, mainKp.publicKey, userUsdcAta, mainKp.publicKey, USDC_MINT, ixs);
    await maybeAddAtaIx(connection, mainKp.publicKey, userUsdtAta, mainKp.publicKey, USDT_MINT, ixs);
    await maybeAddAtaIx(connection, mainKp.publicKey, userDaiAta, mainKp.publicKey, DAI_MINT, ixs);
    await maybeAddAtaIx(connection, mainKp.publicKey, userUsdsAta, mainKp.publicKey, USDS_MINT, ixs);
    await maybeAddAtaIx(connection, mainKp.publicKey, vaultUsdcAta, protocolState, USDC_MINT, ixs);
    await maybeAddAtaIx(connection, mainKp.publicKey, vaultUsdtAta, protocolState, USDT_MINT, ixs);
    await maybeAddAtaIx(connection, mainKp.publicKey, vaultDaiAta, protocolState, DAI_MINT, ixs);
    await maybeAddAtaIx(connection, mainKp.publicKey, vaultUsdsAta, protocolState, USDS_MINT, ixs);
    await maybeAddAtaIx(connection, mainKp.publicKey, userMstbAta, mainKp.publicKey, MSTB_MINT, ixs);

    const beforeMstb2 = await getAccount(connection, userMstbAta, "confirmed");
    const beforeMstbMint2 = await getMint(connection, MSTB_MINT, "confirmed");

    const redeemIx = new TransactionInstruction({
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
        { pubkey: userUsdtAta, isSigner: false, isWritable: true },
        { pubkey: userDaiAta, isSigner: false, isWritable: true },
        { pubkey: userUsdsAta, isSigner: false, isWritable: true },
        { pubkey: vaultUsdcAta, isSigner: false, isWritable: true },
        { pubkey: vaultUsdtAta, isSigner: false, isWritable: true },
        { pubkey: vaultDaiAta, isSigner: false, isWritable: true },
        { pubkey: vaultUsdsAta, isSigner: false, isWritable: true },
        { pubkey: USDC_MINT, isSigner: false, isWritable: false },
        { pubkey: USDT_MINT, isSigner: false, isWritable: false },
        { pubkey: DAI_MINT, isSigner: false, isWritable: false },
        { pubkey: USDS_MINT, isSigner: false, isWritable: false },
        { pubkey: MSTB_MINT, isSigner: false, isWritable: true },
        { pubkey: userMstbAta, isSigner: false, isWritable: true },
        { pubkey: TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
        { pubkey: ASSOCIATED_TOKEN_PROGRAM_ID, isSigner: false, isWritable: false },
      ],
      data: encodeRedeemData(redeemAmount, 0n),
    });

    const tx = new Transaction().add(...ixs, redeemIx);
    const sig = await sendAndConfirmTransaction(connection, tx, [mainKp], {
      commitment: "confirmed",
      preflightCommitment: "confirmed",
    });

    const afterMstb2 = await getAccount(connection, userMstbAta, "confirmed");
    const afterMstbMint2 = await getMint(connection, MSTB_MINT, "confirmed");

    const burnUserDelta = beforeMstb2.amount - afterMstb2.amount;
    const burnSupplyDelta = beforeMstbMint2.supply - afterMstbMint2.supply;

    out.redeem = {
      ok: burnUserDelta > 0n,
      signature: sig,
      amountInRaw: redeemAmount.toString(),
      burnUserDeltaRaw: burnUserDelta.toString(),
      burnSupplyDeltaRaw: burnSupplyDelta.toString(),
      explorer: `https://explorer.solana.com/tx/${sig}?cluster=devnet`,
    };
    logStep("redeem", out.redeem);
  } catch (e) {
    out.redeem = { ok: false, error: String(e?.message || e), attemptedAmountRaw: redeemAmount.toString() };
    out.blockers.push({ step: "redeem", error: out.redeem.error });
    logStep("redeem-failed", out.redeem);
  }

  try {
    const agent = Keypair.generate();
    const stake = 1_000_000_000n;
    const role = 1; // Monitor

    const requiredLamports = Number(stake + 50_000_000n);
    let topup = await withAirdropIfNeeded(connection, agent, requiredLamports);
    let donorFunding = null;

    if (topup.after < requiredLamports) {
      donorFunding = await fundFromLocalDonors(connection, agent.publicKey, requiredLamports);
      topup = {
        ...topup,
        after: donorFunding.finalBalance,
      };
    }

    if (topup.after < requiredLamports) {
      throw new Error(
        `Insufficient SOL for register_agent. required=${requiredLamports} current=${topup.after} airdropError=${topup.airdropError || "none"}`
      );
    }

    const [agentRecord] = PublicKey.findProgramAddressSync([Buffer.from("agent"), agent.publicKey.toBuffer()], PROGRAM_ID);
    const [agentEscrow] = PublicKey.findProgramAddressSync([Buffer.from("v2:agent_escrow"), agent.publicKey.toBuffer()], PROGRAM_ID);

    const existing = await connection.getAccountInfo(agentRecord, "confirmed");
    if (existing) throw new Error("Agent record already exists for generated wallet (unexpected)");

    const ix = new TransactionInstruction({
      programId: PROGRAM_ID,
      keys: [
        { pubkey: agent.publicKey, isSigner: true, isWritable: true },
        { pubkey: agentRecord, isSigner: false, isWritable: true },
        { pubkey: agentEscrow, isSigner: false, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
      ],
      data: encodeRegisterAgentData(role, stake),
    });

    const tx = new Transaction().add(ix);
    const sig = await sendAndConfirmTransaction(connection, tx, [agent], {
      commitment: "confirmed",
      preflightCommitment: "confirmed",
    });

    const recordInfo = await connection.getAccountInfo(agentRecord, "confirmed");
    const escrowInfo = await connection.getAccountInfo(agentEscrow, "confirmed");

    out.register = {
      ok: !!recordInfo,
      signature: sig,
      agent: agent.publicKey.toBase58(),
      role,
      stakeLamports: stake.toString(),
      airdropSig: topup.airdropSig,
      airdropError: topup.airdropError,
      donorFunding,
      agentRecord: agentRecord.toBase58(),
      agentEscrow: agentEscrow.toBase58(),
      escrowLamports: escrowInfo?.lamports ?? null,
      explorer: `https://explorer.solana.com/tx/${sig}?cluster=devnet`,
    };
    logStep("register-agent", out.register);
  } catch (e) {
    out.register = { ok: false, error: String(e?.message || e) };
    out.blockers.push({ step: "register_agent", error: out.register.error });
    logStep("register-agent-failed", out.register);
  }

  out.finishedAt = new Date().toISOString();
  out.ok = Boolean(out.mint?.ok && out.redeem?.ok && out.register?.ok);

  console.log("\n[summary]", JSON.stringify({ ok: out.ok, blockers: out.blockers.length }, null, 2));

  if (process.argv[2]) {
    fs.writeFileSync(process.argv[2], JSON.stringify(out, null, 2));
    console.log(`wrote ${process.argv[2]}`);
  }
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
