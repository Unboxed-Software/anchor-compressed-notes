import {
  SPL_NOOP_PROGRAM_ID,
  deserializeApplicationDataEvent,
} from "@solana/spl-account-compression"
import { Connection, PublicKey } from "@solana/web3.js"
import { bs58 } from "@coral-xyz/anchor/dist/cjs/utils/bytes"
import { deserialize } from "borsh"
import { keccak256 } from "js-sha3"

class NoteLog {
  leafNode: Uint8Array
  owner: PublicKey
  note: string

  constructor(properties: {
    leafNode: Uint8Array
    owner: Uint8Array
    note: string
  }) {
    this.leafNode = properties.leafNode
    this.owner = new PublicKey(properties.owner)
    this.note = properties.note
  }
}

// A map that describes the Note structure for Borsh deserialization
const NoteLogBorshSchema = new Map([
  [
    NoteLog,
    {
      kind: "struct",
      fields: [
        ["leafNode", [32]], // Array of 32 `u8`
        ["owner", [32]], // Pubkey
        ["note", "string"],
      ],
    },
  ],
])

export function getHash(note: string, owner: PublicKey) {
  const noteBuffer = Buffer.from(note)
  const publicKeyBuffer = Buffer.from(owner.toBytes())
  const concatenatedBuffer = Buffer.concat([noteBuffer, publicKeyBuffer])
  const concatenatedUint8Array = new Uint8Array(
    concatenatedBuffer.buffer,
    concatenatedBuffer.byteOffset,
    concatenatedBuffer.byteLength
  )
  return keccak256(concatenatedUint8Array)
}

export async function getNoteLog(connection: Connection, txSignature: string) {
  // Confirm the transaction, otherwise the getTransaction sometimes returns null
  const latestBlockHash = await connection.getLatestBlockhash()
  await connection.confirmTransaction({
    blockhash: latestBlockHash.blockhash,
    lastValidBlockHeight: latestBlockHash.lastValidBlockHeight,
    signature: txSignature,
  })

  // Get the transaction info using the tx signature
  const txInfo = await connection.getTransaction(txSignature, {
    maxSupportedTransactionVersion: 0,
  })

  // Get the inner instructions related to the program instruction at index 0
  // We only send one instruction in test transaction, so we can assume the first
  const innerIx = txInfo!.meta?.innerInstructions?.[0]?.instructions

  // Get the inner instructions that match the SPL_NOOP_PROGRAM_ID
  const noopInnerIx = innerIx.filter(
    (instruction) =>
      txInfo?.transaction.message.staticAccountKeys[
        instruction.programIdIndex
      ].toBase58() === SPL_NOOP_PROGRAM_ID.toBase58()
  )

  let noteLog: NoteLog
  for (let i = noopInnerIx.length - 1; i >= 0; i--) {
    try {
      // Try to decode and deserialize the instruction data
      const applicationDataEvent = deserializeApplicationDataEvent(
        Buffer.from(bs58.decode(noopInnerIx[i]?.data!))
      )

      // Get the application data
      const applicationData = applicationDataEvent.fields[0].applicationData

      // Deserialize the application data into NoteLog instance
      noteLog = deserialize(
        NoteLogBorshSchema,
        NoteLog,
        Buffer.from(applicationData)
      )

      if (noteLog !== undefined) {
        break
      }
    } catch (__) {}
  }

  return noteLog
}
