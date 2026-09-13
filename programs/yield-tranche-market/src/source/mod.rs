pub(crate) mod huma;
pub(crate) mod hylo;

use quasar_lang::prelude::{AccountView, Address};
use yield_tranche_market_core::fixed::{Fix, FIX_SCALE};

use crate::{errors::YieldTrancheMarketError, utils::u128_mul_div};

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Source {
    Hylo,
    Huma,
}

impl Source {
    pub const fn required_account_count(self) -> usize {
        match self {
            Self::Hylo => hylo::ACCOUNT_COUNT,
            Self::Huma => huma::ACCOUNT_COUNT,
        }
    }

    pub fn validate(
        self,
        underlying_mint: &Address,
        accounts: &[AccountView],
    ) -> Result<(), YieldTrancheMarketError> {
        match self {
            Self::Hylo => hylo::validate(underlying_mint, accounts),
            Self::Huma => huma::validate(underlying_mint, accounts),
        }
    }

    pub fn exchange_rate(
        self,
        underlying_mint: &Address,
        accounts: &[AccountView],
    ) -> Result<Fix, YieldTrancheMarketError> {
        match self {
            Self::Hylo => hylo::exchange_rate(underlying_mint, accounts),
            Self::Huma => huma::exchange_rate(underlying_mint, accounts),
        }
    }
}

impl TryFrom<u8> for Source {
    type Error = YieldTrancheMarketError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Hylo),
            1 => Ok(Self::Huma),
            _ => Err(YieldTrancheMarketError::InvalidSource),
        }
    }
}

impl From<Source> for u8 {
    fn from(source: Source) -> Self {
        match source {
            Source::Hylo => 0,
            Source::Huma => 1,
        }
    }
}

pub(crate) fn require_account_count(
    accounts: &[AccountView],
    expected: usize,
) -> Result<(), YieldTrancheMarketError> {
    if accounts.len() == expected {
        Ok(())
    } else {
        Err(YieldTrancheMarketError::InvalidAccountCount)
    }
}

pub(crate) fn scaled_ratio(
    numerator: u128,
    denominator: u64,
) -> Result<Fix, YieldTrancheMarketError> {
    if numerator == 0 || denominator == 0 {
        return Err(YieldTrancheMarketError::InvalidSupply);
    }
    Ok(Fix::new(u128_mul_div(
        numerator,
        FIX_SCALE,
        u128::from(denominator),
    )?))
}
