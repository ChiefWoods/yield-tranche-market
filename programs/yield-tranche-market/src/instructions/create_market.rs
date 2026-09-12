use quasar_lang::{prelude::*, sysvars::Sysvar as SysvarTrait};
use quasar_spl::prelude::*;
use yield_tranche_market_core::{
    tranche_model::TrancheModel,
    utils::{read_u16, read_u8},
};

use crate::{
    constants::MINT_DECIMALS,
    errors::YieldTrancheMarketError,
    events::MarketCreated,
    source::Source,
    state::{Config, JuniorMint, Market, MarketInner, SeniorMint},
    utils::ix_bytes,
    EventAuthority, YieldTrancheMarket,
};

#[derive(Accounts)]
pub struct CreateMarket {
    #[account(mut)]
    pub admin: Signer,
    #[account(
        address = Config::seeds(),
        has_one(admin) @ YieldTrancheMarketError::InvalidAdmin
    )]
    pub config: Account<Config>,
    pub underlying_mint: Account<Mint>,
    #[account(
        init,
        payer = admin,
        address = Market::seeds(underlying_mint.address())
    )]
    pub market: Account<Market>,
    #[account(
        init,
        payer = admin,
        mint(
            decimals = MINT_DECIMALS,
            authority = market,
            token_program = token_program
        ),
        address = SeniorMint::seeds(market.address())
    )]
    pub senior_mint: Account<Mint>,
    #[account(
        init,
        payer = admin,
        mint(
            decimals = MINT_DECIMALS,
            authority = market,
            token_program = token_program
        ),
        address = JuniorMint::seeds(market.address())
    )]
    pub junior_mint: Account<Mint>,
    #[account(
        init(idempotent),
        payer = admin,
        associated_token(
            mint = underlying_mint,
            authority = market,
            token_program = token_program,
        )
    )]
    pub market_vault: Account<Token>,
    pub system_program: Program<SystemProgram>,
    pub token_program: Program<TokenProgram>,
    pub associated_token_program: Program<AssociatedTokenProgram>,
    pub event_authority: EventAuthority,
    pub program: Program<YieldTrancheMarket>,
}

struct CreateMarketArgs {
    source: Source,
    min_coverage: u16,
    junior_exposure_beta: u16,
    tranche_model: TrancheModel,
}

/// Compact tail after the instruction discriminator:
///
/// ```text
/// source:               u8
/// min_coverage:         u16 LE
/// junior_exposure_beta: u16 LE
/// tranche_model:        remaining bytes (`TrancheModel::decode`)
/// ```
fn parse_args(data: &[u8]) -> Result<CreateMarketArgs, YieldTrancheMarketError> {
    Ok(CreateMarketArgs {
        source: Source::try_from(ix_bytes(read_u8(data, 0))?)?,
        min_coverage: ix_bytes(read_u16(data, 1))?,
        junior_exposure_beta: ix_bytes(read_u16(data, 3))?,
        tranche_model: TrancheModel::decode(
            data.get(5..)
                .ok_or(YieldTrancheMarketError::InvalidInstructionData)?,
        )?,
    })
}

impl CreateMarket {
    pub fn handler(&mut self, bumps: &CreateMarketBumps, data: &[u8]) -> Result<(), ProgramError> {
        let CreateMarketArgs {
            source,
            min_coverage,
            junior_exposure_beta,
            mut tranche_model,
        } = parse_args(data)?;

        require!(
            min_coverage <= 100,
            YieldTrancheMarketError::InvalidCoverageConfiguration
        );
        require!(
            junior_exposure_beta <= 100,
            YieldTrancheMarketError::InvalidCoverageConfiguration
        );

        let Clock {
            slot,
            unix_timestamp,
            ..
        } = Clock::get()?;

        tranche_model
            .initialize(unix_timestamp.into())
            .map_err(YieldTrancheMarketError::from)?;

        let tranche_model_bytes = tranche_model
            .encode()
            .map_err(YieldTrancheMarketError::from)?;
        let rent = Rent::get()?;

        self.market.set_inner(
            MarketInner {
                underlying_mint: *self.underlying_mint.address(),
                source: source.into(),
                senior_raw_nav: 0,
                junior_raw_nav: 0,
                senior_effective_nav: 0,
                junior_effective_nav: 0,
                senior_loss_balance: 0,
                junior_loss_balance: 0,
                underlying_mint_exchange_rate: 0,
                min_coverage,
                junior_exposure_beta,
                last_refreshed_slot: 0,
                bump: bumps.market,
                tranche_model: tranche_model_bytes.as_slice(),
            },
            self.admin.to_account_view(),
            rent.lamports_per_byte(),
            rent.exemption_threshold_raw(),
        )?;

        emit_cpi!(MarketCreated {
            market: *self.market.address(),
            underlying_mint: *self.underlying_mint.address(),
            senior_mint: *self.senior_mint.address(),
            junior_mint: *self.junior_mint.address(),
            min_coverage,
            junior_exposure_beta,
            slot: slot.into(),
        })?;

        Ok(())
    }
}
