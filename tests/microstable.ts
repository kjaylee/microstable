import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { assert } from "chai";

import { Microstable } from "../target/types/microstable";

describe("microstable", () => {
  const provider = anchor.AnchorProvider.local();
  anchor.setProvider(provider);
  const program = anchor.workspace.Microstable as Program<Microstable>;

  it("builds initialize/update instruction payloads", async () => {
    const [globalStatePda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("global-state")],
      program.programId,
    );

    const assetInputs: any[] = [
      {
        mint: anchor.web3.Keypair.generate().publicKey,
        weight_bps: 4_000,
        weight_cap_bps: 5_500,
        risk_score: 200,
        oracle: anchor.web3.Keypair.generate().publicKey,
        vault: anchor.web3.Keypair.generate().publicKey,
      },
      {
        mint: anchor.web3.Keypair.generate().publicKey,
        weight_bps: 3_000,
        weight_cap_bps: 4_500,
        risk_score: 400,
        oracle: anchor.web3.Keypair.generate().publicKey,
        vault: anchor.web3.Keypair.generate().publicKey,
      },
      {
        mint: anchor.web3.Keypair.generate().publicKey,
        weight_bps: 2_000,
        weight_cap_bps: 4_500,
        risk_score: 300,
        oracle: anchor.web3.Keypair.generate().publicKey,
        vault: anchor.web3.Keypair.generate().publicKey,
      },
      {
        mint: anchor.web3.Keypair.generate().publicKey,
        weight_bps: 1_000,
        weight_cap_bps: 3_500,
        risk_score: 500,
        oracle: anchor.web3.Keypair.generate().publicKey,
        vault: anchor.web3.Keypair.generate().publicKey,
      },
    ];

    const initIx = await program.methods
      .initialize(new anchor.BN(11_000), 20, 20, assetInputs)
      .accounts({
        payer: provider.publicKey,
        authority: provider.publicKey,
        globalState: globalStatePda,
        basketConfig: anchor.web3.PublicKey.findProgramAddressSync(
          [Buffer.from("basket-config")],
          program.programId,
        )[0],
        circuitState: anchor.web3.PublicKey.findProgramAddressSync(
          [Buffer.from("circuit-state")],
          program.programId,
        )[0],
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .instruction();

    const applyIx = await program.methods
      .applyUpdate(new anchor.BN(12_000))
      .accounts({
        applier: provider.publicKey,
        globalState: globalStatePda,
        basketConfig: anchor.web3.PublicKey.findProgramAddressSync(
          [Buffer.from("basket-config")],
          program.programId,
        )[0],
        updateProposal: anchor.web3.PublicKey.findProgramAddressSync(
          [Buffer.from("update-proposal"), provider.publicKey.toBuffer()],
          program.programId,
        )[0],
      })
      .instruction();

    assert.equal(initIx.programId.toBase58(), program.programId.toBase58());
    assert.equal(applyIx.programId.toBase58(), program.programId.toBase58());
    assert.ok(initIx.data.length > 8);
    assert.ok(applyIx.data.length > 8);
  });
});
