import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { expect } from "chai";
import { PublicKey, SystemProgram } from "@solana/web3.js";
import BN from "bn.js";
import { Microstable } from "../target/types/microstable";

describe("microstable", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.microstable as Program<Microstable>;
  const keeper = provider.wallet.publicKey;

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
    [Buffer.from("user_position"), keeper.toBuffer()],
    program.programId
  );

  const vaultAccounts = {
    vaultUsdc,
    vaultUsdt,
    vaultDai,
    vaultUsds,
  };

  async function currentSlotBn(): Promise<BN> {
    const slot = await provider.connection.getSlot();
    return new BN(slot);
  }

  async function assertRejects(p: Promise<unknown>) {
    let rejected = false;
    try {
      await p;
    } catch {
      rejected = true;
    }
    expect(rejected).to.eq(true);
  }

  async function updateAllOracles(price = 1_000_000, confidence = 1_000) {
    for (let i = 0; i < 4; i += 1) {
      await program.methods
        .updateOracle(i, new BN(price), new BN(confidence), await currentSlotBn())
        .accountsStrict({
          protocolState,
          circuitBreaker,
          keeper,
          ...vaultAccounts,
        })
        .rpc();
    }
  }

  it("initialize + verify state", async () => {
    await program.methods
      .initialize()
      .accountsStrict({
        protocolState,
        circuitBreaker,
        authority: keeper,
        ...vaultAccounts,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const protocol = await program.account.protocolState.fetch(protocolState);
    expect(protocol.feeRate.toNumber()).to.eq(2_000);
    expect(protocol.crTarget.toNumber()).to.eq(1_200_000);
    expect(protocol.weights.map((w) => w.toNumber())).to.deep.eq([
      400_000,
      300_000,
      200_000,
      100_000,
    ]);
  });

  it("mint flow (deposit -> receive µSD)", async () => {
    await updateAllOracles();

    await program.methods
      .mint(0, new BN(1_000_000))
      .accountsStrict({
        protocolState,
        circuitBreaker,
        user: keeper,
        userPosition,
        ...vaultAccounts,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    const protocol = await program.account.protocolState.fetch(protocolState);
    const user = await program.account.userPosition.fetch(userPosition);
    expect(protocol.totalSupply.toNumber()).to.eq(831_666);
    expect(user.usdBalance.toNumber()).to.eq(831_666);
  });

  it("redeem flow (burn µSD -> receive collateral)", async () => {
    await program.methods
      .redeem(new BN(500_000))
      .accountsStrict({
        protocolState,
        circuitBreaker,
        user: keeper,
        userPosition,
        ...vaultAccounts,
      })
      .rpc();

    const protocol = await program.account.protocolState.fetch(protocolState);
    const user = await program.account.userPosition.fetch(userPosition);
    expect(protocol.totalSupply.toNumber()).to.eq(331_666);
    expect(user.usdBalance.toNumber()).to.eq(331_666);
    expect(user.collateralRedeemed[0].toNumber()).to.be.greaterThan(0);
  });

  it("rebalance with weight constraints", async () => {
    await program.methods
      .rebalance([
        new BN(390_000),
        new BN(310_000),
        new BN(200_000),
        new BN(100_000),
      ])
      .accountsStrict({
        protocolState,
        circuitBreaker,
        keeper,
        ...vaultAccounts,
      })
      .rpc();

    const protocol = await program.account.protocolState.fetch(protocolState);
    expect(protocol.weights.map((w) => w.toNumber())).to.deep.eq([
      390_000,
      310_000,
      200_000,
      100_000,
    ]);

    await assertRejects(
      program.methods
        .rebalance([
          new BN(550_000),
          new BN(250_000),
          new BN(120_000),
          new BN(80_000),
        ])
        .accountsStrict({
          protocolState,
          circuitBreaker,
          keeper,
          ...vaultAccounts,
        })
        .rpc()
    );
  });

  it("oracle update rejects stale data", async () => {
    const slot = await provider.connection.getSlot();

    await assertRejects(
      program.methods
        .updateOracle(0, new BN(1_000_000), new BN(1_000), new BN(slot - 500))
        .accountsStrict({
          protocolState,
          circuitBreaker,
          keeper,
          ...vaultAccounts,
        })
        .rpc()
    );
  });

  it("invariant enforcement rejects invalid weights", async () => {
    await assertRejects(
      program.methods
        .rebalance([
          new BN(400_000),
          new BN(300_000),
          new BN(200_000),
          new BN(50_000),
        ])
        .accountsStrict({
          protocolState,
          circuitBreaker,
          keeper,
          ...vaultAccounts,
        })
        .rpc()
    );
  });

  it("circuit breaker activation and recovery", async () => {
    await program.methods
      .updateOracle(0, new BN(970_000), new BN(1_000), await currentSlotBn())
      .accountsStrict({
        protocolState,
        circuitBreaker,
        keeper,
        ...vaultAccounts,
      })
      .rpc();

    await program.methods
      .activateCircuitBreaker(1, 0)
      .accountsStrict({
        protocolState,
        circuitBreaker,
        keeper,
        ...vaultAccounts,
      })
      .rpc();

    let cb = await program.account.circuitBreakerState.fetch(circuitBreaker);
    expect(cb.status[0]).to.not.eq(0);

    for (let i = 0; i < 6; i += 1) {
      await program.methods
        .updateOracle(0, new BN(1_000_000), new BN(1_000), await currentSlotBn())
        .accountsStrict({
          protocolState,
          circuitBreaker,
          keeper,
          ...vaultAccounts,
        })
        .rpc();
    }

    await program.methods
      .recoverCircuitBreaker(1)
      .accountsStrict({
        protocolState,
        circuitBreaker,
        keeper,
        ...vaultAccounts,
      })
      .rpc();

    cb = await program.account.circuitBreakerState.fetch(circuitBreaker);
    expect(cb.status[0]).to.eq(3); // Recovery
  });
});
