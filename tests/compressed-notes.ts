import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { CompressedNotes } from "../target/types/compressed_notes";
import {
  Keypair,
  Transaction,
  PublicKey,
  sendAndConfirmTransaction,
  Connection,
} from "@solana/web3.js";
import {
  ValidDepthSizePair,
  createAllocTreeIx,
  SPL_ACCOUNT_COMPRESSION_PROGRAM_ID,
  SPL_NOOP_PROGRAM_ID,
  ConcurrentMerkleTreeAccount,
} from "@solana/spl-account-compression";
import { getHash, getNoteLog } from "./utils";
import { assert } from "chai";

describe("compressed-notes", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const connection = new Connection(
    provider.connection.rpcEndpoint,
    "confirmed",
  );

  const wallet = provider.wallet as anchor.Wallet;
  const program = anchor.workspace.CompressedNotes as Program<CompressedNotes>;

  // Generate a new keypair for the Merkle tree account
  const merkleTree = Keypair.generate();

  // Derive the PDA to use as the tree authority for the Merkle tree account
  const [treeAuthority] = PublicKey.findProgramAddressSync(
    [merkleTree.publicKey.toBuffer()],
    program.programId,
  );

  const firstNote = "hello world";
  const secondNote = "0".repeat(917);
  const updatedNote = "updated note";

  describe("Merkle Tree Operations", () => {
    it("creates a new note tree", async () => {
      const maxDepthSizePair: ValidDepthSizePair = {
        maxDepth: 3,
        maxBufferSize: 8,
      };
    
      const canopyDepth = 0;
    
      // Instruction to create a new account with the required space for the tree
      const allocTreeIx = await createAllocTreeIx(
        connection,
        merkleTree.publicKey,
        wallet.publicKey,
        maxDepthSizePair,
        canopyDepth,
      );
    
      // Instruction to initialize the tree through the Note program
      const ix = await program.methods
        .createNoteTree(
          maxDepthSizePair.maxDepth,
          maxDepthSizePair.maxBufferSize,
        )
        .accounts({
          owner: wallet.publicKey,
          merkleTree: merkleTree.publicKey,
          treeAuthority,
          logWrapper: SPL_NOOP_PROGRAM_ID,
          compressionProgram: SPL_ACCOUNT_COMPRESSION_PROGRAM_ID,
        })
        .instruction();
    
      const tx = new Transaction().add(allocTreeIx, ix);
      await sendAndConfirmTransaction(connection, tx, [
        wallet.payer,
        merkleTree,
      ]);

      // Fetch the Merkle tree account to confirm it's initialized
      const merkleTreeAccount = await ConcurrentMerkleTreeAccount.fromAccountAddress(
        connection,
        merkleTree.publicKey
      );
      assert(merkleTreeAccount, "Merkle tree should be initialized");
    });

    it("adds a note to the Merkle tree", async () => {
      const txSignature = await program.methods
        .appendNote(firstNote)
        .accounts({
          owner: wallet.publicKey,
          merkleTree: merkleTree.publicKey,
          treeAuthority,
          logWrapper: SPL_NOOP_PROGRAM_ID,
          compressionProgram: SPL_ACCOUNT_COMPRESSION_PROGRAM_ID,
        })
        .rpc();

      const noteLog = await getNoteLog(connection, txSignature);
      const hash = getHash(firstNote, wallet.publicKey);

      assert(
        hash === Buffer.from(noteLog.leafNode).toString("hex"),
        "Leaf node hash should match"
      );
      assert(firstNote === noteLog.note, "Note should match the appended note");
    });

    it("adds max size note to the Merkle tree", async () => {
      const txSignature = await program.methods
        .appendNote(secondNote)
        .accounts({
          owner: wallet.publicKey,
          merkleTree: merkleTree.publicKey,
          treeAuthority,
          logWrapper: SPL_NOOP_PROGRAM_ID,
          compressionProgram: SPL_ACCOUNT_COMPRESSION_PROGRAM_ID,
        })
        .rpc();

      const noteLog = await getNoteLog(connection, txSignature);
      const hash = getHash(secondNote, wallet.publicKey);

      assert(
        hash === Buffer.from(noteLog.leafNode).toString("hex"),
        "Leaf node hash should match"
      );
      assert(
        secondNote === noteLog.note,
        "Note should match the appended max size note"
      );
    });

    it("updates the first note in the Merkle tree", async () => {
      const merkleTreeAccount = await ConcurrentMerkleTreeAccount.fromAccountAddress(
        connection,
        merkleTree.publicKey
      );
      const root = merkleTreeAccount.getCurrentRoot();

      const txSignature = await program.methods
        .updateNote(0, root, firstNote, updatedNote)
        .accounts({
          owner: wallet.publicKey,
          merkleTree: merkleTree.publicKey,
          treeAuthority,
          logWrapper: SPL_NOOP_PROGRAM_ID,
          compressionProgram: SPL_ACCOUNT_COMPRESSION_PROGRAM_ID,
        })
        .rpc();

      const noteLog = await getNoteLog(connection, txSignature);
      const hash = getHash(updatedNote, wallet.publicKey);

      assert(
        hash === Buffer.from(noteLog.leafNode).toString("hex"),
        "Leaf node hash should match after update"
      );
      assert(
        updatedNote === noteLog.note,
        "Updated note should match the logged note"
      );
    });
  });
});