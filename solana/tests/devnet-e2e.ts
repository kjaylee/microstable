import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { expect } from "chai";
import {
  ASSOCIATED_TOKEN_PROGRAM_ID,
  TOKEN_PROGRAM_ID,
  getAccount,
  getOrCreateAssociatedTokenAccount,
  mintTo,
} from "@solana/spl-token";
import { Keypair, PublicKey, SystemProgram } from "@solana/web3.js";
import BN from "bn.js";
import fs from "fs";
import { Microstable } from "../target/types/microstable";

const MAIN_KEYPAIR = "/Users/kjaylee/.config/solana/devnet-keypair.json";
const KEEPER2_KEYPAIR = "/tmp/keeper2.json";
const KEEPER3_KEYPAIR = "/tmp/keeper3.json";

const USDC_MINT = new PublicKey("VLDKjAMvPXK2rGbhKynrShXgALfikwrwD517CxLRb8C");
const USDT_MINT = new PublicKey("6muD8Dtn4TVbENmXxVN7yoznwB2cVH9Y8cHNZ6hpvxJd");
const DAI_MINT = new PublicKey("6zTaf6yZ6HBFt2Bvi43fhoaXDid2dmLsCk64tq58CvZ4");
const USDS_MINT = new PublicKey("HtFssc7CKVTf67zDPMmK6LiurKgjHEWtaviqG24XKWjk");
const MSTB_MINT = new PublicKey("EZUwC88f1s3k9prgv5DGY6wML8giBqdpRxoA2rLtGA6R");

const PYTH_USDC_USD = new PublicKey("Dpw1EAVrSB1ibxiDQyTAW6Zip3J4Btk2x4SgApQCeFbX");
const PYTH_USDT_USD = new PublicKey("HT2PLQBcG5EiCcNSaMHAjSgd9F98ecpATbk4Sk5oYuM");
const PYTH_DAI_USD = new PublicKey("FmfrxJ7YH8yVxoYpJ9ZDMeb8gUceYXYaSrQiBJ1uSZjN");

function loadKeypair(path: string): Keypair {
  return Keypair.fromSecretKey(Uint8Array.from(JSON.parse(fs.readFileSync(path, "utf-8"))));
}

describe("devnet e2e (SPL collateral + manual oracle)", () => {
  const main = loadKeypair(MAIN_KEYPAIR);
  const keeper2 = loadKeypair(KEEPER2_KEYPAIR);
  const keeper3 = loadKeypair(KEEPER3_KEYPAIR);

  const provider = new anchor.AnchorProvider(
    new anchor.web3.Connection(process.env.ANCHOR_PROVIDER_URL ?? "https://api.devnet.solana.com", "confirmed"),
    new anchor.Wallet(main),
    { commitment: "confirmed", preflightCommitment: "confirmed" }
  );
  anchor.setProvider(provider);

  const program = anchor.workspace.microstable as Program<Microstable>;

  const [protocolState] = PublicKey.findProgramAddressSync(
    [Buffer.from("protocol_state")],
    program.programId
  );
  const [circuitBreaker] = PublicKey.findProgramAddressSync(
    [Buffer.from("circuit_breaker")],
    program.programId
  );
  const [vaultUsdc] = PublicKey.findProgramAddressSync(
    [Buffer.from("collateral_vault"), Buffer.from([0])],
    program.programId
  );
  const [vaultUsdt] = PublicKey.findProgramAddressSync(
    [Buffer.from("collateral_vault"), Buffer.from([1])],
    program.programId
  );
  const [vaultDai] = PublicKey.findProgramAddressSync(
    [Buffer.from("collateral_vault"), Buffer.from([2])],
    program.programId
  );
  const [vaultUsds] = PublicKey.findProgramAddressSync(
    [Buffer.from("collateral_vault"), Buffer.from([3])],
    program.programId
  );
  const [userPosition] = PublicKey.findProgramAddressSync(
    [Buffer.from("user_position"), main.publicKey.toBuffer()],
    program.programId
  );

  it("initialize -> oracle quorum -> mint/redeem", async function () {
    this.timeout(240_000);

    const keeperSet = [main.publicKey, keeper2.publicKey, keeper3.publicKey];

    const userUsdcAta = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      main,
      USDC_MINT,
      main.publicKey
    );
    const userUsdtAta = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      main,
      USDT_MINT,
      main.publicKey
    );
    const userDaiAta = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      main,
      DAI_MINT,
      main.publicKey
    );
    const userUsdsAta = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      main,
      USDS_MINT,
      main.publicKey
    );
    await getOrCreateAssociatedTokenAccount(provider.connection, main, MSTB_MINT, main.publicKey);

    const vaultUsdcAta = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      main,
      USDC_MINT,
      protocolState,
      true
    );
    const vaultUsdtAta = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      main,
      USDT_MINT,
      protocolState,
      true
    );
    const vaultDaiAta = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      main,
      DAI_MINT,
      protocolState,
      true
    );
    const vaultUsdsAta = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      main,
      USDS_MINT,
      protocolState,
      true
    );

    const depositAmount = 1_000_000; // 1 USDC (6 decimals)
    const userUsdcBalance = await getAccount(provider.connection, userUsdcAta.address);
    if (Number(userUsdcBalance.amount) < depositAmount) {
      await mintTo(
        provider.connection,
        main,
        USDC_MINT,
        userUsdcAta.address,
        main,
        depositAmount - Number(userUsdcBalance.amount)
      );
    }

    await program.methods
      .migrateLegacyState(keeperSet)
      .accountsStrict({
        protocolState,
        circuitBreaker,
        vaultUsdc,
        vaultUsdt,
        vaultDai,
        vaultUsds,
        usdcMint: USDC_MINT,
        usdtMint: USDT_MINT,
        daiMint: DAI_MINT,
        usdsMint: USDS_MINT,
        authority: main.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([main])
      .rpc();

    const pythFeeds = [PYTH_USDC_USD, PYTH_USDT_USD, PYTH_DAI_USD];
    for (let i = 0; i < pythFeeds.length; i += 1) {
      await program.methods
        .setPythFeed(i, pythFeeds[i])
        .accountsStrict({
          protocolState,
          vaultUsdc,
          vaultUsdt,
          vaultDai,
          vaultUsds,
          keeperOne: main.publicKey,
          keeperTwo: keeper2.publicKey,
        })
        .signers([main, keeper2])
        .rpc();
    }

    for (let i = 0; i < 4; i += 1) {
      const slot = await provider.connection.getSlot("confirmed");
      await program.methods
        .updateOracle(i, new BN(1_000_000), new BN(1_000), new BN(slot))
        .accountsStrict({
          protocolState,
          circuitBreaker,
          vaultUsdc,
          vaultUsdt,
          vaultDai,
          vaultUsds,
          keeperOne: main.publicKey,
          keeperTwo: keeper2.publicKey,
        })
        .signers([main, keeper2])
        .rpc();
    }

    const beforeUserPos = await program.account.userPosition.fetchNullable(userPosition);
    const beforeUsd = beforeUserPos ? Number(beforeUserPos.usdBalance) : 0;
    const beforeUserUsdc = await getAccount(provider.connection, userUsdcAta.address);
    const beforeVaultUsdc = await getAccount(provider.connection, vaultUsdcAta.address);

    await program.methods
      .mint(0, new BN(depositAmount))
      .accountsStrict({
        protocolState,
        circuitBreaker,
        vaultUsdc,
        vaultUsdt,
        vaultDai,
        vaultUsds,
        user: main.publicKey,
        userPosition,
        userCollateralAta: userUsdcAta.address,
        vaultCollateralAta: vaultUsdcAta.address,
        collateralMint: USDC_MINT,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
      })
      .signers([main])
      .rpc();

    const afterMintPos = await program.account.userPosition.fetch(userPosition);
    const mintedDelta = Number(afterMintPos.usdBalance) - beforeUsd;
    expect(mintedDelta).to.be.greaterThan(0);

    await program.methods
      .redeem(new BN(mintedDelta))
      .accountsStrict({
        protocolState,
        circuitBreaker,
        vaultUsdc,
        vaultUsdt,
        vaultDai,
        vaultUsds,
        user: main.publicKey,
        userPosition,
        userUsdcAta: userUsdcAta.address,
        userUsdtAta: userUsdtAta.address,
        userDaiAta: userDaiAta.address,
        userUsdsAta: userUsdsAta.address,
        vaultUsdcAta: vaultUsdcAta.address,
        vaultUsdtAta: vaultUsdtAta.address,
        vaultDaiAta: vaultDaiAta.address,
        vaultUsdsAta: vaultUsdsAta.address,
        usdcMint: USDC_MINT,
        usdtMint: USDT_MINT,
        daiMint: DAI_MINT,
        usdsMint: USDS_MINT,
        tokenProgram: TOKEN_PROGRAM_ID,
        associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      })
      .signers([main])
      .rpc();

    const afterRedeemPos = await program.account.userPosition.fetch(userPosition);
    const afterUserUsdc = await getAccount(provider.connection, userUsdcAta.address);
    const afterVaultUsdc = await getAccount(provider.connection, vaultUsdcAta.address);

    expect(Number(afterRedeemPos.usdBalance)).to.eq(beforeUsd);
    expect(Number(afterUserUsdc.amount)).to.be.closeTo(Number(beforeUserUsdc.amount), 1);
    expect(Number(afterVaultUsdc.amount)).to.be.closeTo(Number(beforeVaultUsdc.amount), 1);
  });
});
