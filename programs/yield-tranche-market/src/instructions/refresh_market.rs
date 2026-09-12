use crate::{
    errors::YieldTrancheMarketError, events::MarketRefreshed, state::Market,
    utils::RemainingAccountViews, EventAuthority, YieldTrancheMarket,
};
use quasar_lang::{prelude::*, sysvars::Sysvar};
use quasar_spl::prelude::*;

#[derive(Accounts)]
pub struct RefreshMarket {
    // permissionless
    #[account(
        mut,
        has_one(underlying_mint) @ YieldTrancheMarketError::InvalidUnderlyingMint
    )]
    pub market: Account<Market>,
    pub underlying_mint: Account<Mint>,
    pub event_authority: EventAuthority,
    pub program: Program<YieldTrancheMarket>,
}

impl RefreshMarket {
    pub fn handler(&mut self, remaining: RemainingAccounts<'_>) -> Result<(), ProgramError> {
        let accounts = RemainingAccountViews::from_remaining(remaining)?;
        let source = self.market.source()?;
        let exchange_rate =
            source.exchange_rate(self.underlying_mint.address(), accounts.as_slice())?;

        let Clock {
            slot,
            unix_timestamp,
            ..
        } = Clock::get()?;

        self.market
            .refresh(exchange_rate, slot.into(), Some(unix_timestamp.into()))?;

        emit_cpi!(MarketRefreshed {
            market: *self.market.address(),
            exchange_rate: exchange_rate.bits,
            slot: slot.into(),
        })?;

        Ok(())
    }
}
