use anchor_lang::{
    prelude::*, 
    solana_program::keccak
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

declare_id!("2CCvZS82NbYtLDuQgegGJB9pspjMizama2tQQy8Vu6Ps");

#[error_code]
pub enum CompressedNotesError {
    #[msg("Invalid tree depth")]
    InvalidTreeDepth,
    #[msg("Invalid buffer size")]
    InvalidBufferSize,
    #[msg("Notes are identical")]
    IdenticalNotes,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct NoteLog {
    leaf_node: [u8; 32],  // The leaf node hash
    owner: Pubkey,        // Pubkey of the note owner
    note: String,         // The note message
}

impl NoteLog {
    // Constructs a new note from given leaf node and message
    pub fn new(leaf_node: [u8; 32], owner: Pubkey, note: String) -> Self {
        Self { leaf_node, owner, note }
    }
}

#[program]
pub mod compressed_notes {
    use super::*;

    // Instruction for creating a new note tree.
    pub fn create_note_tree(
        ctx: Context<NoteAccounts>,
        max_depth: u32,       // Max depth of the merkle tree
        max_buffer_size: u32, // Max buffer size of the merkle tree
    ) -> Result<()> {
        // Input validation
        if max_depth < 3 || max_depth > 30 {
            return Err(CompressedNotesError::InvalidTreeDepth.into());
        }
        if max_buffer_size < 8 || max_buffer_size > 1024 {
            return Err(CompressedNotesError::InvalidBufferSize.into());
        }

        // Get the address for the merkle tree account
        let merkle_tree = ctx.accounts.merkle_tree.key();

        // Define the seeds for pda signing
        let signer_seeds: &[&[&[u8]]] = &[&[
            merkle_tree.as_ref(), // The address of the merkle tree account as a seed
            &[*ctx.bumps.get("tree_authority").unwrap()], // The bump seed for the pda
        ]];

        // Create cpi context for init_empty_merkle_tree instruction.
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.compression_program.to_account_info(), // The spl account compression program
            Initialize {
                authority: ctx.accounts.tree_authority.to_account_info(), // The authority for the merkle tree, using a PDA
                merkle_tree: ctx.accounts.merkle_tree.to_account_info(), // The merkle tree account to be initialized
                noop: ctx.accounts.log_wrapper.to_account_info(), // The noop program to log data
            },
            signer_seeds, // The seeds for pda signing
        );

        // CPI to initialize an empty merkle tree with given max depth and buffer size
        init_empty_merkle_tree(cpi_ctx, max_depth, max_buffer_size)?;
        Ok(())
    }

    // Instruction for appending a note to a tree.
    pub fn append_note(ctx: Context<NoteAccounts>, note: String) -> Result<()> {
        // Hash the "note message" which will be stored as leaf node in the merkle tree
        let leaf_node =
            keccak::hashv(&[note.as_bytes(), ctx.accounts.owner.key().as_ref()]).to_bytes();
        // Create a new "note log" using the leaf node hash and note.
        let note_log = NoteLog::new(leaf_node.clone(), ctx.accounts.owner.key().clone(), note);
        // Log the "note log" data using noop program
        wrap_application_data_v1(note_log.try_to_vec()?, &ctx.accounts.log_wrapper)?;
        // Get the address for the merkle tree account
        let merkle_tree = ctx.accounts.merkle_tree.key();
        // Define the seeds for pda signing
        let signer_seeds: &[&[&[u8]]] = &[&[
            merkle_tree.as_ref(), // The address of the merkle tree account as a seed
            &[*ctx.bumps.get("tree_authority").unwrap()], // The bump seed for the pda
        ]];
        // Create a new cpi context and append the leaf node to the merkle tree.
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.compression_program.to_account_info(), // The spl account compression program
            Modify {
                authority: ctx.accounts.tree_authority.to_account_info(), // The authority for the merkle tree, using a PDA
                merkle_tree: ctx.accounts.merkle_tree.to_account_info(), // The merkle tree account to be modified
                noop: ctx.accounts.log_wrapper.to_account_info(), // The noop program to log data
            },
            signer_seeds, // The seeds for pda signing
        );
        // CPI to append the leaf node to the merkle tree
        append(cpi_ctx, leaf_node)?;
        Ok(())
    }

    pub fn update_note(
        ctx: Context<NoteAccounts>,
        index: u32,
        root: [u8; 32],
        old_note: String,
        new_note: String,
    ) -> Result<()> {
        if old_note == new_note {
            return Err(CompressedNotesError::IdenticalNotes.into());
        }

        let old_leaf =
            keccak::hashv(&[old_note.as_bytes(), ctx.accounts.owner.key().as_ref()]).to_bytes();

        let merkle_tree = ctx.accounts.merkle_tree.key();

        // Define the seeds for pda signing
        let signer_seeds: &[&[&[u8]]] = &[&[
            merkle_tree.as_ref(), // The address of the merkle tree account as a seed
            &[*ctx.bumps.get("tree_authority").unwrap()], // The bump seed for the pda
        ]];

        // Verify Leaf
        {
            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.compression_program.to_account_info(), // The spl account compression program
                VerifyLeaf {
                    merkle_tree: ctx.accounts.merkle_tree.to_account_info(), // The merkle tree account to be modified
                },
                signer_seeds, // The seeds for pda signing
            );
            // Verify or Fails
            verify_leaf(cpi_ctx, root, old_leaf, index)?;
        }

        let new_leaf =
            keccak::hashv(&[new_note.as_bytes(), ctx.accounts.owner.key().as_ref()]).to_bytes();

        // Log out for indexers
        let note_log = NoteLog::new(new_leaf.clone(), ctx.accounts.owner.key().clone(), new_note);
        // Log the "note log" data using noop program
        wrap_application_data_v1(note_log.try_to_vec()?, &ctx.accounts.log_wrapper)?;

        // replace leaf
        {
            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.compression_program.to_account_info(), // The spl account compression program
                Modify {
                    authority: ctx.accounts.tree_authority.to_account_info(), // The authority for the merkle tree, using a PDA
                    merkle_tree: ctx.accounts.merkle_tree.to_account_info(), // The merkle tree account to be modified
                    noop: ctx.accounts.log_wrapper.to_account_info(), // The noop program to log data
                },
                signer_seeds, // The seeds for pda signing
            );
            // CPI to append the leaf node to the merkle tree
            replace_leaf(cpi_ctx, root, old_leaf, new_leaf, index)?;
        }

        Ok(())
    }
}

#[derive(Accounts)]
pub struct NoteAccounts<'info> {
    // The payer for the transaction
    #[account(mut)]
    pub owner: Signer<'info>,

    // The pda authority for the merkle tree, only used for signing
    #[account(
        seeds = [merkle_tree.key().as_ref()],
        bump,
    )]
    pub tree_authority: SystemAccount<'info>,

    // The merkle tree account
    /// CHECK: This account is validated by the spl account compression program
    #[account(mut)]
    pub merkle_tree: UncheckedAccount<'info>,

    // The noop program to log data
    pub log_wrapper: Program<'info, Noop>,

    // The spl account compression program
    pub compression_program: Program<'info, SplAccountCompression>,
}