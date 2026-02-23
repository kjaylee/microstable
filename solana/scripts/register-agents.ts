import * as anchor from "@coral-xyz/anchor";
import { PublicKey, Keypair, Connection, SystemProgram } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";

async function main() {
  const connection = new Connection("https://api.devnet.solana.com", "confirmed");
  const idlPath = path.resolve(__dirname, "../target/idl/microstable.json");
  const idl = JSON.parse(fs.readFileSync(idlPath, "utf8"));
  const programId = new PublicKey("BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3");

  // Load keypairs
  const loadKeypair = (p: string) =>
    Keypair.fromSecretKey(new Uint8Array(JSON.parse(fs.readFileSync(p, "utf8"))));

  const keypairs = [
    { name: "keeper1 (authority)", kp: loadKeypair(path.resolve(process.env.HOME!, ".config/solana/devnet-keypair.json")) },
    { name: "keeper2", kp: loadKeypair("/tmp/keeper2.json") },
    { name: "keeper3", kp: loadKeypair("/tmp/keeper3.json") },
  ];

  // Agent roles: 0=Optimizer, 1=Monitor, 2=Auditor, 3=Liquidator
  const roles = [
    { optimizer: {} },
    { monitor: {} },
    { auditor: {} },
  ];

  for (let i = 0; i < keypairs.length; i++) {
    const { name, kp } = keypairs[i];
    const role = roles[i];
    const stakeAmount = new anchor.BN(100_000); // 0.0001 SOL

    const wallet = new anchor.Wallet(kp);
    const provider = new anchor.AnchorProvider(connection, wallet, {
      commitment: "confirmed",
    });
    const program = new anchor.Program(idl, provider);

    // Derive PDAs
    const [agentRecord] = PublicKey.findProgramAddressSync(
      [Buffer.from("agent"), kp.publicKey.toBuffer()],
      programId
    );
    const [agentEscrow] = PublicKey.findProgramAddressSync(
      [Buffer.from("agent_escrow")],
      programId
    );

    console.log(`\nRegistering ${name}: ${kp.publicKey.toBase58()}`);
    console.log(`  AgentRecord PDA: ${agentRecord.toBase58()}`);
    console.log(`  Role: ${Object.keys(role)[0]}`);
    console.log(`  Stake: ${stakeAmount.toString()} lamports`);

    // Check if already registered
    const existing = await connection.getAccountInfo(agentRecord);
    if (existing) {
      console.log(`  ⏭️  Already registered (${existing.data.length} bytes), skipping`);
      continue;
    }

    try {
      const tx = await program.methods
        .registerAgent(role, stakeAmount)
        .accounts({
          agent: kp.publicKey,
          agentRecord: agentRecord,
          agentEscrow: agentEscrow,
          systemProgram: SystemProgram.programId,
        })
        .rpc();

      console.log(`  ✅ Registered! TX: ${tx}`);
    } catch (err: any) {
      console.error(`  ❌ Failed: ${err.message}`);
      if (err.logs) {
        err.logs.forEach((l: string) => console.error(`    ${l}`));
      }
    }
  }

  console.log("\n🎉 Agent registration complete!");
}

main().catch(console.error);
