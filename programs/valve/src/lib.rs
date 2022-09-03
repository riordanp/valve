use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::clock::Clock;
use anchor_lang::solana_program::sysvar::instructions;
use bytemuck::bytes_of;
use std::mem::size_of;

declare_id!("9CPHrMBdzUkW6H21AWyh2aEFXmQfRwdANjbV163NobyZ");
pub const NUM_CALL_ACCOUNTS: usize = 3;

#[program]
pub mod valve {
    use anchor_lang::solana_program::{instruction::Instruction, program::invoke_signed};

    use super::*;

    pub fn initialize_policy(
        ctx: Context<InitializePolicy>,
        endpoint: u32,
        max_reqs: u32,
        period: u32,
    ) -> Result<()> {
        let policy = &mut ctx.accounts.policy;
        policy.program = *ctx.accounts.program.key;
        policy.endpoint = endpoint;
        policy.max_reqs = max_reqs;
        policy.period = period;
        policy.bump = *ctx.bumps.get("policy").unwrap();
        Ok(())
    }

    pub fn initialize_bucket(ctx: Context<InitializeBucket>) -> Result<()> {
        let bucket = &mut ctx.accounts.bucket;
        bucket.policy = ctx.accounts.policy.key();
        bucket.owner = *ctx.accounts.owner.key;
        bucket.tokens = ctx.accounts.policy.max_reqs;
        bucket.last_ts = Clock::get()?.unix_timestamp;
        Ok(())
    }

    pub fn check(ctx: Context<Check>) -> Result<()> {
        // TODO Check caller is bucket owner
        // TODO Check policy is correct for bucket
        let bucket = &mut ctx.accounts.bucket;
        let policy = &ctx.accounts.policy;

        let now_ts = Clock::get()?.unix_timestamp;
        let elapsed_secs: u32 = now_ts.saturating_sub(bucket.last_ts).try_into().unwrap();
        bucket.last_ts = now_ts;

        bucket.tokens = policy.max_reqs.min(
            bucket.tokens
                + ((policy.max_reqs as f32 / policy.period as f32) * elapsed_secs as f32) as u32,
        );

        if bucket.tokens > 0 {
            bucket.tokens = bucket.tokens - 1;
        } else {
            return err!(ValveError::TooManyRequests);
        }

        Ok(())
    }

    pub fn call(ctx: Context<Call>, ix: Vec<u8>) -> Result<()> {
        // TODO Check caller is bucket owner
        // TODO Check policy is correct for bucket
        let bucket = &mut ctx.accounts.bucket;
        let policy = &ctx.accounts.policy;

        let now_ts = Clock::get()?.unix_timestamp;
        let elapsed_secs: u32 = now_ts.saturating_sub(bucket.last_ts).try_into().unwrap();
        bucket.last_ts = now_ts;

        bucket.tokens = policy.max_reqs.min(
            bucket.tokens
                + ((policy.max_reqs as f32 / policy.period as f32) * elapsed_secs as f32) as u32,
        );

        if bucket.tokens > 0 {
            bucket.tokens = bucket.tokens - 1;

            let metas = ctx.accounts.to_account_metas(Some(false));
            let infos = ctx.accounts.to_account_infos();
            let (_, post_infos) = infos.split_at(NUM_CALL_ACCOUNTS);
            let (_, post_accts) = metas.split_at(NUM_CALL_ACCOUNTS);
            for acct in ctx.remaining_accounts {
                msg!("{}", acct.key());
            }
            let cpi = Instruction {
                program_id: ctx.accounts.policy.program.key(),
                accounts: post_accts.to_vec(),
                data: ix,
            };
            let nonce: u64 = ctx.accounts.policy.bump.into();
            let signer_key = ctx.accounts.policy.key();

            let signer_seeds = gen_signer_seeds(&nonce, &signer_key);
            invoke_signed(&cpi, post_infos, &[&signer_seeds])?;
        } else {
            return err!(ValveError::TooManyRequests);
        }

        Ok(())
    }

    // close_policy
    // - must not prevent orphaned buckets from closing
    // change policy
    // reclaim charges
    // close bucket
    // - tokens must be full to prevent init state attack, how do we know full without the policy? store max on bucket? then what about policy edits?
}

// TODO option to charge spammers
// - charge stored on policy
// - stored on the bucket balance since it's writable anyway, transferred to policy owner with another ix called async "reclaim_charges"
// - must prevent bucket account closing if balance > rent exemption, or move extra lamports to policy/policy owner on close
// - if charge is zero then don't charge
#[account]
pub struct Policy {
    program: Pubkey,
    endpoint: u32,
    max_reqs: u32,
    period: u32,
    bump: u8,
}

#[account]
pub struct Bucket {
    policy: Pubkey,
    owner: Pubkey,
    tokens: u32,
    last_ts: i64,
}

#[derive(Accounts)]
#[instruction(endpoint: u32, max_reqs: u32, period: u32)]
pub struct InitializePolicy<'info> {
    #[account(
        init,
        seeds = [b"Policy".as_ref(), program.key().as_ref(), &endpoint.to_le_bytes()],
        bump,
        payer = payer,
        space = 8 + size_of::<Policy>()
    )]
    pub policy: Account<'info, Policy>,
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: override
    pub program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

// Use init_if_needed on check instead?
#[derive(Accounts)]
pub struct InitializeBucket<'info> {
    #[account(
        init,
        seeds = [b"Bucket".as_ref(), policy.key().as_ref(), owner.key.as_ref()],
        bump,
        payer = owner,
        space = 8 + size_of::<Bucket>()
    )]
    pub bucket: Account<'info, Bucket>,
    #[account(mut)]
    pub owner: Signer<'info>,
    pub policy: Account<'info, Policy>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Check<'info> {
    #[account(mut)]
    pub bucket: Account<'info, Bucket>,
    pub owner: Signer<'info>,
    pub policy: Account<'info, Policy>,
}

#[derive(Accounts)]
pub struct Call<'info> {
    #[account(mut)]
    pub bucket: Account<'info, Bucket>,
    pub owner: Signer<'info>,
    pub policy: Account<'info, Policy>,
}

#[error_code]
pub enum ValveError {
    #[msg("Valve check required")]
    Unchecked,
    #[msg("Rate limit exceeded")]
    TooManyRequests,
}

pub fn verify(ixs: &AccountInfo, program: &Pubkey, endpoint: u32) -> Result<()> {
    let current_index = instructions::load_current_index_checked(ixs)?;
    let mut checked = false;
    msg!("verify for program {} endpoint {}", program, endpoint);
    for i in 0..current_index {
        let ix = match instructions::load_instruction_at_checked(i as usize, ixs) {
            Ok(ix) => ix,
            Err(ProgramError::InvalidArgument) => break, // past the last instruction
            Err(e) => return Err(e.into()),
        };
        if ix.program_id != crate::id() {
            continue;
        }

        // TODO why is Discriminator not implemented for Check
        // if ix.data[0..8] != valve::instruction::Check::discriminator() {
        //     continue;
        // }

        // check that it's for the correct endpoint and program
        let expected_policy_pk = Pubkey::find_program_address(
            &[
                b"Policy".as_ref(),
                program.as_ref(),
                &endpoint.to_le_bytes(),
            ],
            &crate::id(),
        )
        .0;

        require_keys_eq!(ix.accounts[2].pubkey, expected_policy_pk);
        checked = true;
    }
    if !checked {
        return err!(ValveError::Unchecked);
    }

    Ok(())
}

pub fn gen_signer_seeds<'a>(nonce: &'a u64, acc_pk: &'a Pubkey) -> [&'a [u8]; 2] {
    [acc_pk.as_ref(), bytes_of(nonce)]
}
