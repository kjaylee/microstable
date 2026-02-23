import * as anchor from "@coral-xyz/anchor";
import { PublicKey, Keypair, Connection, SystemProgram } from "@solana/web3.js";
import * as fs from "fs";
import * as path from "path";

async function main() {
  // Load authority keypair
  const authorityPath = path.resolve(
    process.env.HOME!,
    ".config/solana/devnet-keypair.json"
  );
  const authoritySecret = JSON.parse(fs.readFileSync(authorityPath, "utf8"));
  const authority = Keypair.fromSecretKey(new Uint8Array(authoritySecret));

  // Load keeper keypairs
  const k2Secret = JSON.parse(fs.readFileSync("/tmp/keeper2.json", "utf8"));
  const k3Secret = JSON.parse(fs.readFileSync("/tmp/keeper3.json", "utf8"));
  const k2 = Keypair.fromSecretKey(new Uint8Array(k2Secret));
  const k3 = Keypair.fromSecretKey(new Uint8Array(k3Secret));

  const newKeeperSet = [authority.publicKey, k2.publicKey, k3.publicKey];

  console.log("Authority:", authority.publicKey.toBase58());
  console.log("Keeper Set:");
  newKeeperSet.forEach((k, i) => console.log(`  [${i}]: ${k.toBase58()}`));

  // Setup
  const connection = new Connection("https://api.devnet.solana.com", "confirmed");
  const wallet = new anchor.Wallet(authority);
  const provider = new anchor.AnchorProvider(connection, wallet, {
    commitment: "confirmed",
  });

  const idlPath = path.resolve(__dirname, "../target/idl/microstable.json");
  const idl = JSON.parse(fs.readFileSync(idlPath, "utf8"));
  const programId = new PublicKey("BSdLEPVKq1bxdLGx9HR2XSStdYhFeU3SdFGC2i4i2ps3");
  const program = new anchor.Program(idl, provider);

  // Derive PDAs
  const [protocolState] = PublicKey.findProgramAddressSync(
    [Buffer.from("protocol_state")],
    programId
  );
  const [circuitBreaker] = PublicKey.findProgramAddressSync(
    [Buffer.from("circuit_breaker")],
    programId
  );

  console.log("\nProtocol State PDA:", protocolState.toBase58());
  console.log("Circuit Breaker PDA:", circuitBreaker.toBase58());

  try {
    const tx = await program.methods
      .devnetForceReinit(newKeeperSet)
      .accounts({
        protocolState: protocolState,
        circuitBreaker: circuitBreaker,
        authority: authority.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .rpc();

    console.log("\n✅ devnet_force_reinit SUCCESS");
    console.log("Signature:", tx);
  } catch (err: any) {
    console.error("\n❌ devnet_force_reinit FAILED:", err.message || err);
    if (err.logs) {
      console.error("Logs:", err.logs.join("\n"));
    }
    process.exit(1);
  }

  // Verify
  console.log("\nVerifying on-chain state...");
  const accountInfo = await connection.getAccountInfo(protocolState);
  if (accountInfo) {
    console.log("Account data length:", accountInfo.data.length, "bytes");
    const offset = 8 + 32 + 8 + 8 + 8 + 8 + 8 + 8; // 88
    for (let i = 0; i < 3; i++) {
      const pk = new PublicKey(
        accountInfo.data.slice(offset + i * 32, offset + (i + 1) * 32)
      );
      const match = pk.equals(newKeeperSet[i]) ? "✅" : "❌";
      console.log(`  keeper_set[${i}]: ${pk.toBase58()} ${match}`);
    }
  }
}

main().catch(console.error);
