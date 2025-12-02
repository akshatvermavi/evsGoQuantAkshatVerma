import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { EphemeralVault } from "../target/types/ephemeral_vault";
import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";

// Basic Anchor test skeleton to demonstrate create_vault + approve_delegate flow.

describe("ephemeral_vault", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.EphemeralVault as Program<EphemeralVault>;

  it("can create a vault and approve delegate", async () => {
    const parent = Keypair.generate();
    const ephemeral = Keypair.generate();

    // Airdrop some SOL to parent
    const sig = await provider.connection.requestAirdrop(parent.publicKey, 1_000_000_000);
    await provider.connection.confirmTransaction(sig);

    const [vaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), parent.publicKey.toBuffer(), ephemeral.publicKey.toBuffer()],
      program.programId
    );

    const sessionDuration = new anchor.BN(3600);
    const maxDeposit = new anchor.BN(500_000_000);

    await program.methods
      .createVault(sessionDuration, maxDeposit, ephemeral.publicKey)
      .accounts({
        parent: parent.publicKey,
        ephemeralWallet: ephemeral.publicKey,
        vault: vaultPda,
        systemProgram: SystemProgram.programId,
      })
      .signers([parent])
      .rpc();

    const [delegationPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("delegation"), vaultPda.toBuffer()],
      program.programId
    );

    await program.methods
      .approveDelegate(ephemeral.publicKey)
      .accounts({
        vault: vaultPda,
        parent: parent.publicKey,
        delegation: delegationPda,
        systemProgram: SystemProgram.programId,
      })
      .signers([parent])
      .rpc();

    const vaultAccount = await program.account.ephemeralVault.fetch(vaultPda);
    expect(vaultAccount.isActive).toBe(true);
    expect(vaultAccount.maxDeposit.toNumber()).toBe(500_000_000);
  });
});
