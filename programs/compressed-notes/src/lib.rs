use anchor_lang::{
    prelude::*,
    solana_program::keccak,
};
use spl_account_compression::{
    Noop,
    program::SplAccountCompression,
    cpi::{
        accounts::{Initialize, Modify, VerifyLeaf},
        init_empty_merkle_tree, verify_leaf, replace_leaf, append,
    },
    wrap_application_data_v1,
};

// Replace with your program ID
declare_id!("YOUR_KEY_GOES_HERE");


/// A program that manages compressed notes using a Merkle tree for efficient storage and verification.
#[program]
pub mod compressed_notes {
    use super::*;

    /// Initializes a new Merkle tree for storing messages.
    ///
    /// This function creates a Merkle tree with the specified maximum depth and buffer size.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context containing the accounts required for initializing the tree.
    /// * `max_depth` - The maximum depth of the Merkle tree.
    /// * `max_buffer_size` - The maximum buffer size of the Merkle tree.
    pub fn create_messages_tree(
        ctx: Context<MessageAccounts>,
        max_depth: u32,
        max_buffer_size: u32,
    ) -> Result<()> {
        // Get the address for the Merkle tree account
        let merkle_tree = ctx.accounts.merkle_tree.key();

        // Define the seeds for PDA signing
        let signer_seeds: &[&[&[u8]]] = &[
            &[
                merkle_tree.as_ref(), // The address of the Merkle tree account as a seed
                &[*ctx.bumps.get("tree_authority").unwrap()], // The bump seed for the PDA
            ],
        ];

        // Create CPI context for `init_empty_merkle_tree` instruction
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.compression_program.to_account_info(), // The SPL account compression program
            Initialize {
                authority: ctx.accounts.tree_authority.to_account_info(), // The authority for the Merkle tree, using a PDA
                merkle_tree: ctx.accounts.merkle_tree.to_account_info(), // The Merkle tree account to be initialized
                noop: ctx.accounts.log_wrapper.to_account_info(), // The noop program to log data
            },
            signer_seeds // The seeds for PDA signing
        );

        // CPI to initialize an empty Merkle tree with the given max depth and buffer size
        init_empty_merkle_tree(cpi_ctx, max_depth, max_buffer_size)?;

        Ok(())
    }

    /// Appends a new message to the Merkle tree.
    ///
    /// This function hashes the message and adds it as a leaf node to the tree.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context containing the accounts required for appending the message.
    /// * `message` - The message to append to the Merkle tree.
    pub fn append_message(ctx: Context<MessageAccounts>, message: String) -> Result<()> {
        // Hash the message + sender's public key to create a leaf node
        let leaf_node = keccak::hashv(&[message.as_bytes(), ctx.accounts.sender.key().as_ref()]).to_bytes();

        // Create a new "MessageLog" using the leaf node hash, sender, recipient, and message
        let message_log = new_message_log(
            leaf_node.clone(),
            ctx.accounts.sender.key().clone(),
            ctx.accounts.recipient.key().clone(),
            message,
        );

        // Log the "MessageLog" data using the noop program
        wrap_application_data_v1(message_log.try_to_vec()?, &ctx.accounts.log_wrapper)?;

        // Get the Merkle tree account address
        let merkle_tree = ctx.accounts.merkle_tree.key();

        // Define the seeds for PDA signing
        let signer_seeds: &[&[&[u8]]] = &[
            &[
                merkle_tree.as_ref(), // The address of the Merkle tree account as a seed
                &[*ctx.bumps.get("tree_authority").unwrap()], // The bump seed for the PDA
            ],
        ];

        // Create a CPI context and append the leaf node to the Merkle tree
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.compression_program.to_account_info(), // The SPL account compression program
            Modify {
                authority: ctx.accounts.tree_authority.to_account_info(), // Authority for the Merkle tree, using a PDA
                merkle_tree: ctx.accounts.merkle_tree.to_account_info(), // The Merkle tree account to be modified
                noop: ctx.accounts.log_wrapper.to_account_info(), // The noop program to log data
            },
            signer_seeds, // The seeds for PDA signing
        );

        // CPI call to append the leaf node to the Merkle tree
        append(cpi_ctx, leaf_node)?;

        Ok(())
    }

    /// Updates an existing message in the Merkle tree.
    ///
    /// This function verifies the old message and replaces it with the new message in the tree.
    ///
    /// # Arguments
    ///
    /// * `ctx` - The context containing the accounts required for updating the message.
    /// * `index` - The index of the message in the tree.
    /// * `root` - The root of the Merkle tree.
    /// * `old_message` - The old message to be replaced.
    /// * `new_message` - The new message to replace the old message.
    pub fn update_message(
        ctx: Context<MessageAccounts>,
        index: u32,
        root: [u8; 32],
        old_message: String,
        new_message: String,
    ) -> Result<()> {
        // Hash the old message + sender's public key to create the old leaf node
        let old_leaf = keccak::hashv(&[old_message.as_bytes(), ctx.accounts.sender.key().as_ref()]).to_bytes();

        // Get the Merkle tree account address
        let merkle_tree = ctx.accounts.merkle_tree.key();

        // Define the seeds for PDA signing
        let signer_seeds: &[&[&[u8]]] = &[
            &[
                merkle_tree.as_ref(), // The address of the Merkle tree account as a seed
                &[*ctx.bumps.get("tree_authority").unwrap()], // The bump seed for the PDA
            ],
        ];

        // Verify the old leaf node in the Merkle tree
        {
            // If the old and new messages are the same, no update is needed
            if old_message == new_message {
                msg!("Messages are the same!");
                return Ok(());
            }

            // Create CPI context for verifying the leaf node
            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.compression_program.to_account_info(), // The SPL account compression program
                VerifyLeaf {
                    merkle_tree: ctx.accounts.merkle_tree.to_account_info(), // The Merkle tree account to be verified
                },
                signer_seeds, // The seeds for PDA signing
            );

            // Verify the old leaf node in the Merkle tree
            verify_leaf(cpi_ctx, root, old_leaf, index)?;
        }

        // Hash the new message + sender's public key to create the new leaf node
        let new_leaf = keccak::hashv(&[new_message.as_bytes(), ctx.accounts.sender.key().as_ref()]).to_bytes();

        // Log the new message for indexers using the noop program
        let message_log = new_message_log(
            new_leaf.clone(),
            ctx.accounts.sender.key().clone(),
            ctx.accounts.recipient.key().clone(),
            new_message,
        );
        wrap_application_data_v1(message_log.try_to_vec()?, &ctx.accounts.log_wrapper)?;

        // Replace the old leaf with the new leaf in the Merkle tree
        {
            // Create CPI context for replacing the leaf node
            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.compression_program.to_account_info(), // The SPL account compression program
                Modify {
                    authority: ctx.accounts.tree_authority.to_account_info(), // The authority for the Merkle tree, using a PDA
                    merkle_tree: ctx.accounts.merkle_tree.to_account_info(), // The Merkle tree account to be modified
                    noop: ctx.accounts.log_wrapper.to_account_info(), // The noop program to log data
                },
                signer_seeds, // The seeds for PDA signing
            );

            // Replace the old leaf node with the new one in the Merkle tree
            replace_leaf(cpi_ctx, root, old_leaf, new_leaf, index)?;
        }

        Ok(())
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
/// A struct representing a log entry in the Merkle tree for a message.
pub struct MessageLog {
    /// The leaf node hash generated from the message data.
    pub leaf_node: [u8; 32],
    /// The public key of the message's sender.
    pub from: Pubkey,
    /// The public key of the message's recipient.
    pub to: Pubkey,
    /// The content of the message.
    pub message: String,
}

/// Constructs a new message log from a given leaf node, sender, recipient, and message.
///
/// # Arguments
///
/// * `leaf_node` - A 32-byte array representing the hash of the message.
/// * `from` - The public key of the message's sender.
/// * `to` - The public key of the message's recipient.
/// * `message` - The message content.
///
/// # Returns
///
/// A new `MessageLog` struct containing the provided data.
pub fn new_message_log(leaf_node: [u8; 32], from: Pubkey, to: Pubkey, message: String) -> MessageLog {
    MessageLog { leaf_node, from, to, message }
}

/// Struct for holding the account information required for message operations.
#[derive(Accounts)]
pub struct MessageAccounts<'info> {
    /// The Merkle tree account.
    #[account(mut)]
    pub merkle_tree: AccountInfo<'info>,
    /// The authority for the Merkle tree.
    pub tree_authority: AccountInfo<'info>,
    /// The sender's account.
    pub sender: Signer<'info>,
    /// The recipient's account.
    pub recipient: AccountInfo<'info>,
    /// The compression program (Noop program).
    pub compression_program: Program<'info, SplAccountCompression>,
    /// The log wrapper account for logging data.
    pub log_wrapper: AccountInfo<'info>,
}