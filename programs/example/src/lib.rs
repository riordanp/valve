use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::instructions;

declare_id!("BUX2jkXcywqm6bFUWwqWREqMYjeVyBMwDryfLMAFMbTx");

#[program]
pub mod example {
    use super::*;

    pub fn test(ctx: Context<Test>, a: u32, b: u32) -> Result<()> {
        msg!("test");
        valve::verify(&ctx.accounts.instructions, &crate::id(), 111)?;
        msg!("{}", a + b);
        Ok(())
    }

    pub fn test_cpi(ctx: Context<TestCPI>, a: u32, b: u32) -> Result<()> {
        msg!("test_cpi");
        // TODO: verify called with CPI
        msg!("{}", a + b);
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(a: u32, b: u32)]
pub struct Test<'info> {
    /// CHECK: override
    /// TODO: would be nice to get an instructions sysvar instead
    #[account(address = instructions::ID)]
    pub instructions: UncheckedAccount<'info>,
}

#[derive(Accounts)]
#[instruction(a: u32, b: u32)]
pub struct TestCPI {}
