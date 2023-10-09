import * as anchor from "@coral-xyz/anchor"
import { Program } from "@coral-xyz/anchor"
import { CompressedNotes } from "../target/types/compressed_notes"
import {
  Keypair,
  Transaction,
  PublicKey,
  sendAndConfirmTransaction,
  Connection,
} from "@solana/web3.js"
import {
  ValidDepthSizePair,
  createAllocTreeIx,
  SPL_ACCOUNT_COMPRESSION_PROGRAM_ID,
  SPL_NOOP_PROGRAM_ID,
  ConcurrentMerkleTreeAccount,
} from "@solana/spl-account-compression"
import { getHash, getNoteLog } from "./utils"
import { assert } from "chai"

describe("compressed-notes", () => {
  const provider = anchor.AnchorProvider.env()
  anchor.setProvider(provider)
  const connection = new Connection(
    provider.connection.rpcEndpoint,
    "confirmed" // has to be confirmed for some of the methods below
  )

  const wallet = provider.wallet as anchor.Wallet
  const program = anchor.workspace.CompressedNotes as Program<CompressedNotes>

  // Generate a new keypair for the merkle tree account
  const merkleTree = Keypair.generate()

  const firstNote = "hello world"
  const secondNote = "0".repeat(917)
  const updatedNote = "updated note"

  // Derive the PDA to use as the tree authority for the merkle tree account
  // This is a PDA derived from the Note program, which allows the program to sign for appends instructions to the tree
  const [treeAuthority] = PublicKey.findProgramAddressSync(
    [merkleTree.publicKey.toBuffer()],
    program.programId
  )

  it("Create Note Tree", async () => {
    const maxDepthSizePair: ValidDepthSizePair = {
      maxDepth: 3,
      maxBufferSize: 8,
    }
    const canopyDepth = 0
    // instruction to create new account with required space for tree
    const allocTreeIx = await createAllocTreeIx(
      connection,
      merkleTree.publicKey,
      wallet.publicKey,
      maxDepthSizePair,
      canopyDepth
    )
    // instruction to initialize the tree through the Note program
    const ix = await program.methods
      .createNoteTree(maxDepthSizePair.maxDepth, maxDepthSizePair.maxBufferSize)
      .accounts({
        merkleTree: merkleTree.publicKey,
        treeAuthority: treeAuthority,
        logWrapper: SPL_NOOP_PROGRAM_ID,
        compressionProgram: SPL_ACCOUNT_COMPRESSION_PROGRAM_ID,
      })
      .instruction()
    const tx = new Transaction().add(allocTreeIx, ix)
    await sendAndConfirmTransaction(connection, tx, [wallet.payer, merkleTree])
  })

  it("Add Note", async () => {
    const txSignature = await program.methods
      .appendNote(firstNote)
      .accounts({
        merkleTree: merkleTree.publicKey,
        treeAuthority: treeAuthority,
        logWrapper: SPL_NOOP_PROGRAM_ID,
        compressionProgram: SPL_ACCOUNT_COMPRESSION_PROGRAM_ID,
      })
      .rpc()

    const noteLog = await getNoteLog(connection, txSignature)
    const hash = getHash(firstNote, provider.publicKey)

    assert(hash === Buffer.from(noteLog.leafNode).toString("hex"))
    assert(firstNote === noteLog.note)
  })

  it("Add Max Size Note", async () => {
    // Size of note is limited by max transaction size of 1232 bytes, minus additional data required for the instruction
    const txSignature = await program.methods
      .appendNote(secondNote)
      .accounts({
        merkleTree: merkleTree.publicKey,
        treeAuthority: treeAuthority,
        logWrapper: SPL_NOOP_PROGRAM_ID,
        compressionProgram: SPL_ACCOUNT_COMPRESSION_PROGRAM_ID,
      })
      .rpc()

    const noteLog = await getNoteLog(connection, txSignature)
    const hash = getHash(secondNote, provider.publicKey)

    assert(hash === Buffer.from(noteLog.leafNode).toString("hex"))
    assert(secondNote === noteLog.note)
  })

  it("Update First Note", async () => {
    const merkleTreeAccount =
      await ConcurrentMerkleTreeAccount.fromAccountAddress(
        connection,
        merkleTree.publicKey
      )

    const rootKey = merkleTreeAccount.tree.changeLogs[0].root
    const root = Array.from(rootKey.toBuffer())

    const txSignature = await program.methods
      .updateNote(0, root, firstNote, updatedNote)
      .accounts({
        merkleTree: merkleTree.publicKey,
        treeAuthority: treeAuthority,
        logWrapper: SPL_NOOP_PROGRAM_ID,
        compressionProgram: SPL_ACCOUNT_COMPRESSION_PROGRAM_ID,
      })
      .rpc()

    const noteLog = await getNoteLog(connection, txSignature)
    const hash = getHash(updatedNote, provider.publicKey)

    assert(hash === Buffer.from(noteLog.leafNode).toString("hex"))
    assert(updatedNote === noteLog.note)
  })
})
