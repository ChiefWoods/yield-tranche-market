#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

mod constants;
mod errors;
mod events;
mod instructions;
mod source;
mod state;
mod utils;
use instructions::*;

declare_id!("Gws69WxCHbunX429o7Est1wfCjXW9tbe4YfJKJQuNzka");

#[program]
mod yield_tranche_market {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn create_config(ctx: Ctx<CreateConfig>) -> Result<(), ProgramError> {
        ctx.accounts.handler()
    }

    #[instruction(discriminator = 1)]
    pub fn create_market(ctx: Ctx<CreateMarket>) -> Result<(), ProgramError> {
        ctx.accounts.handler(&ctx.bumps, ctx.data)
    }

    #[instruction(discriminator = 2)]
    pub fn refresh_market(ctx: CtxWithRemaining<RefreshMarket>) -> Result<(), ProgramError> {
        ctx.accounts.handler(ctx.remaining_accounts())
    }

    #[instruction(discriminator = 3)]
    pub fn deposit(
        ctx: Ctx<Deposit>,
        is_senior: bool,
        amount_in: u64,
        min_amount_out: u64,
    ) -> Result<(), ProgramError> {
        ctx.accounts.handler(is_senior, amount_in, min_amount_out)
    }

    #[instruction(discriminator = 4)]
    pub fn withdraw(
        ctx: Ctx<Withdraw>,
        is_senior: bool,
        amount_in: u64,
        min_amount_out: u64,
    ) -> Result<(), ProgramError> {
        ctx.accounts.handler(is_senior, amount_in, min_amount_out)
    }
}

#[cfg(all(test, not(feature = "idl-build")))]
mod tests;
