use quasar_lang::{prelude::*, sysvars::Sysvar};
use quasar_spl::prelude::*;
use solana_math::{SafeConvert, SafeMath};
use yield_tranche_market_core::fixed::Fix;

use crate::{
    errors::YieldTrancheMarketError,
    events::Withdrawn,
    state::{nav, JuniorMint, Market, SeniorMint},
    utils::u128_mul_div,
    EventAuthority, YieldTrancheMarket,
};
use fix::aliases::si::Centi;

#[derive(Accounts)]
pub struct Withdraw {
    #[account(mut)]
    pub authority: Signer,
    #[account(
        mut,
        has_one(underlying_mint) @ YieldTrancheMarketError::InvalidUnderlyingMint,
    )]
    pub market: Account<Market>,
    pub underlying_mint: Account<Mint>,
    #[account(mut)]
    pub tranche_mint: Account<Mint>,
    #[account(
        mut,
        associated_token(
            mint = tranche_mint,
            authority = authority,
            token_program = token_program,
        )
    )]
    pub authority_receipt_token_account: Account<Token>,
    #[account(
        mut,
        associated_token(
            mint = underlying_mint,
            authority = market,
            token_program = token_program,
        )
    )]
    pub market_vault: Account<Token>,
    #[account(
        init(idempotent),
        payer = authority,
        associated_token(
            mint = underlying_mint,
            authority = authority,
            token_program = token_program,
        )
    )]
    pub authority_underlying_token_account: Account<Token>,
    pub system_program: Program<SystemProgram>,
    pub token_program: Program<TokenProgram>,
    pub associated_token_program: Program<AssociatedTokenProgram>,
    pub event_authority: EventAuthority,
    pub program: Program<YieldTrancheMarket>,
}

impl Withdraw {
    pub fn handler(
        &mut self,
        is_senior: bool,
        amount_in: u64,
        min_amount_out: u64,
    ) -> Result<(), ProgramError> {
        if is_senior {
            SeniorMint::validate_address(self.tranche_mint.address(), self.market.address())
                .map_err(|_| YieldTrancheMarketError::InvalidSeniorMint)?;
        } else {
            JuniorMint::validate_address(self.tranche_mint.address(), self.market.address())
                .map_err(|_| YieldTrancheMarketError::InvalidJuniorMint)?;
        }

        require!(amount_in > 0, YieldTrancheMarketError::InvalidAmount);

        let Clock { slot, .. } = Clock::get()?;
        let slot: u64 = slot.into();

        require!(
            !self.market.is_stale(slot),
            YieldTrancheMarketError::MarketStale
        );

        let exchange_rate = Fix::new(self.market.underlying_mint_exchange_rate.into());

        require!(
            exchange_rate.bits > 0,
            YieldTrancheMarketError::InvalidExchangeRate
        );

        let tranche_supply = self.tranche_mint.supply();

        require!(
            tranche_supply > 0,
            YieldTrancheMarketError::TrancheMintHasNoSupply
        );

        let effective_nav: u64 = if is_senior {
            self.market.senior_effective_nav
        } else {
            self.market.junior_effective_nav
        }
        .into();

        let effective_claim = Fix::new(
            // amount_in cannot be greater than receipt_supply
            u128_mul_div(
                u128::from(effective_nav),
                u128::from(amount_in),
                u128::from(tranche_supply.min(amount_in)),
            )?,
        );

        let amount_out = effective_claim
            .bits
            .safe_div(exchange_rate.bits)?
            .safe_to_u64()?;

        require!(
            amount_out >= min_amount_out,
            YieldTrancheMarketError::SlippageExceeded
        );

        // The transfer is constrained by the actual custody balance, rather
        // than a cached raw NAV, so Senior redemptions cannot overdraw custody.
        require!(
            amount_out <= self.market_vault.amount(),
            YieldTrancheMarketError::InsufficientVaultBalance
        );

        let raw_value = nav(amount_out, exchange_rate)?;

        self.market
            .withdraw(is_senior, raw_value, effective_claim)?;

        if !is_senior {
            let min_coverage: Fix =
                Centi::new(u128::from(u16::from(self.market.min_coverage))).convert();

            require!(
                self.market.coverage()? >= min_coverage,
                YieldTrancheMarketError::InsufficientCoverage
            );
        }

        self.token_program
            .burn(
                self.authority_receipt_token_account.to_account_view(),
                self.tranche_mint.to_account_view(),
                self.authority.to_account_view(),
                amount_in,
            )
            .invoke()?;

        self.token_program
            .transfer(
                self.market_vault.to_account_view(),
                self.authority_underlying_token_account.to_account_view(),
                self.market.to_account_view(),
                amount_out,
            )
            .invoke_signed(
                &Market::seeds(self.underlying_mint.address()).with_bump(self.market.bump),
            )?;

        emit_cpi!(Withdrawn {
            market: *self.market.address(),
            authority: *self.authority.address(),
            is_senior,
            amount_in,
            amount_out,
            slot
        })?;
        Ok(())
    }
}
