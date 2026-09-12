use crate::{errors::YieldTrancheMarketError, source::Source};
use fix::aliases::si::{Centi, Unit};
use quasar_lang::{prelude::*, sysvars::Sysvar};
use solana_math::{
    impls::safe_fix_math::{SafeFixAddSub, SafeFixMulDiv},
    SafeMath,
};
use solmath::{fp_div, fp_mul};
use yield_tranche_market_core::{
    fixed::{Fix, FIX_SCALE},
    tranche_model::{TrancheModel, MAX_TRANCHE_MODEL_BYTES},
};

fn u64_to_stored_fix(bits: impl Into<u64>) -> Fix {
    Fix::new(u128::from(bits.into()))
}

fn stored_fix_to_u64(value: Fix) -> Result<u64, YieldTrancheMarketError> {
    u64::try_from(value.bits).map_err(|_| YieldTrancheMarketError::ArithmeticOverflow)
}

pub(crate) fn is_stale_at_slot(
    last_refreshed_slot: u64,
    current_slot: u64,
    tolerance: u64,
) -> bool {
    current_slot < last_refreshed_slot
        || current_slot.saturating_sub(last_refreshed_slot) > tolerance
}

pub fn nav(amount: u64, exchange_rate: Fix) -> Result<Fix, YieldTrancheMarketError> {
    Unit::new(u128::from(amount))
        .safe_mul(exchange_rate)
        .map(|value| value.convert())
        .map_err(Into::into)
}

#[account(discriminator = 2, set_inner)]
#[seeds(b"market", underlying_mint: Address)]
pub struct Market {
    pub underlying_mint: Address,
    pub source: u8,
    pub senior_raw_nav: u64,
    pub junior_raw_nav: u64,
    pub senior_effective_nav: u64,
    pub junior_effective_nav: u64,
    pub senior_loss_balance: u64,
    pub junior_loss_balance: u64,
    pub underlying_mint_exchange_rate: u128,
    pub min_coverage: u16,
    pub junior_exposure_beta: u16,
    pub last_refreshed_slot: u64,
    pub bump: u8,
    pub tranche_model: Vec<u8, MAX_TRANCHE_MODEL_BYTES>,
}

impl Market {
    const STALENESS_TOLERANCE: u8 = 1;

    pub fn is_stale(&self, current_slot: u64) -> bool {
        is_stale_at_slot(
            self.last_refreshed_slot.into(),
            current_slot,
            Self::STALENESS_TOLERANCE.into(),
        )
    }

    pub fn source(&self) -> Result<Source, YieldTrancheMarketError> {
        Source::try_from(self.source)
    }

    fn get_senior_raw_nav(&self) -> Fix {
        u64_to_stored_fix(self.senior_raw_nav)
    }

    fn get_junior_raw_nav(&self) -> Fix {
        u64_to_stored_fix(self.junior_raw_nav)
    }

    fn get_senior_effective_nav(&self) -> Fix {
        u64_to_stored_fix(self.senior_effective_nav)
    }

    fn get_junior_effective_nav(&self) -> Fix {
        u64_to_stored_fix(self.junior_effective_nav)
    }

    fn get_senior_loss_balance(&self) -> Fix {
        u64_to_stored_fix(self.senior_loss_balance)
    }

    fn get_junior_loss_balance(&self) -> Fix {
        u64_to_stored_fix(self.junior_loss_balance)
    }

    fn set_senior_raw_nav(&mut self, value: Fix) -> Result<(), YieldTrancheMarketError> {
        self.senior_raw_nav = stored_fix_to_u64(value)?.into();
        Ok(())
    }

    fn set_junior_raw_nav(&mut self, value: Fix) -> Result<(), YieldTrancheMarketError> {
        self.junior_raw_nav = stored_fix_to_u64(value)?.into();
        Ok(())
    }

    fn set_senior_effective_nav(&mut self, value: Fix) -> Result<(), YieldTrancheMarketError> {
        self.senior_effective_nav = stored_fix_to_u64(value)?.into();
        Ok(())
    }

    fn set_junior_effective_nav(&mut self, value: Fix) -> Result<(), YieldTrancheMarketError> {
        self.junior_effective_nav = stored_fix_to_u64(value)?.into();
        Ok(())
    }

    fn set_senior_loss_balance(&mut self, value: Fix) -> Result<(), YieldTrancheMarketError> {
        self.senior_loss_balance = stored_fix_to_u64(value)?.into();
        Ok(())
    }

    fn set_junior_loss_balance(&mut self, value: Fix) -> Result<(), YieldTrancheMarketError> {
        self.junior_loss_balance = stored_fix_to_u64(value)?.into();
        Ok(())
    }

    fn get_junior_exposure_beta(&self) -> Fix {
        Centi::new(u128::from(u16::from(self.junior_exposure_beta))).convert()
    }

    fn new_nav(old_nav: Fix, new_rate: Fix, old_rate: Fix) -> Result<Fix, YieldTrancheMarketError> {
        if old_rate.bits == 0 {
            return Ok(old_nav);
        }
        old_nav
            .safe_mul(new_rate)?
            .safe_div(old_rate)
            .map_err(Into::into)
    }

    /// coverage = junior_effective_nav / protected_exposure
    pub fn coverage(&self) -> Result<Fix, YieldTrancheMarketError> {
        let protected_exposure =
            self.protected_exposure(self.get_senior_raw_nav(), self.get_junior_raw_nav())?;
        if protected_exposure.bits == 0 {
            return Ok(Fix::default());
        }

        Ok(Fix::new(fp_div(
            self.get_junior_effective_nav().bits,
            protected_exposure.bits,
        )?))
    }

    /// protected_exposure = senior_raw_nav + ceil(junior_raw_nav * junior_exposure_beta)
    fn protected_exposure(
        &self,
        senior_raw_nav: Fix,
        junior_raw_nav: Fix,
    ) -> Result<Fix, YieldTrancheMarketError> {
        let junior = junior_raw_nav
            .safe_mul(self.get_junior_exposure_beta())?
            // ceil
            .bits
            // `fix`'s rounding helpers require `muldiv`, which currently
            // has no `u128` implementation. Keep the scale conversion checked.
            .safe_add(FIX_SCALE - 1)?
            .safe_div(FIX_SCALE)?;

        senior_raw_nav
            .safe_add(Fix::new(junior))
            .map_err(Into::into)
    }

    pub fn deposit(&mut self, is_senior: bool, value: Fix) -> Result<(), YieldTrancheMarketError> {
        if is_senior {
            self.set_senior_raw_nav(self.get_senior_raw_nav().safe_add(value)?)?;
            self.set_senior_effective_nav(self.get_senior_effective_nav().safe_add(value)?)
        } else {
            self.set_junior_raw_nav(self.get_junior_raw_nav().safe_add(value)?)?;
            self.set_junior_effective_nav(self.get_junior_effective_nav().safe_add(value)?)
        }
    }

    pub fn withdraw(
        &mut self,
        is_senior: bool,
        raw_value: Fix,
        effective_value: Fix,
    ) -> Result<(), YieldTrancheMarketError> {
        let (raw_nav, effective_nav) = if is_senior {
            (
                self.get_senior_raw_nav().bits,
                self.get_senior_effective_nav().bits,
            )
        } else {
            (
                self.get_junior_raw_nav().bits,
                self.get_junior_effective_nav().bits,
            )
        };

        if raw_value.bits > raw_nav || effective_value.bits > effective_nav {
            return Err(YieldTrancheMarketError::WithdrawalExceedsNav);
        }

        let new_raw_nav = Fix::new(raw_nav - raw_value.bits);
        let new_effective_nav = Fix::new(effective_nav - effective_value.bits);

        if is_senior {
            self.set_senior_raw_nav(new_raw_nav)?;
            self.set_senior_effective_nav(new_effective_nav)
        } else {
            self.set_junior_raw_nav(new_raw_nav)?;
            self.set_junior_effective_nav(new_effective_nav)
        }
    }

    /// TranchModel type does not change here, only its contents
    fn mutate_tranche_model(&mut self, encoded: &[u8]) -> Result<(), YieldTrancheMarketError> {
        if encoded.len() != self.tranche_model().len() {
            return Err(YieldTrancheMarketError::InvalidTrancheModel);
        }
        let offset = self.__view.data_len() - encoded.len();
        unsafe {
            core::ptr::copy_nonoverlapping(
                encoded.as_ptr(),
                self.__view.data_mut_ptr().add(offset),
                encoded.len(),
            );
        }
        Ok(())
    }

    pub fn refresh(
        &mut self,
        exchange_rate: Fix,
        slot: u64,
        now_ts: Option<i64>,
    ) -> Result<(), YieldTrancheMarketError> {
        let old_rate = Fix::new(self.underlying_mint_exchange_rate.into());
        let senior = Self::new_nav(self.get_senior_raw_nav(), exchange_rate, old_rate)?;
        let junior = Self::new_nav(self.get_junior_raw_nav(), exchange_rate, old_rate)?;
        let mut tranche_model = TrancheModel::decode(self.tranche_model())?;

        let min_coverage: Fix = Centi::new(u128::from(u16::from(self.min_coverage))).convert();

        let old_senior = self.get_senior_raw_nav().bits;
        let old_junior = self.get_junior_raw_nav().bits;
        let new_senior = senior.bits;
        let new_junior = junior.bits;

        let mut junior_effective_nav = self.get_junior_effective_nav();
        let mut senior_effective_nav = self.get_senior_effective_nav();
        let mut junior_loss_balance = self.get_junior_loss_balance();
        let mut senior_loss_balance = self.get_senior_loss_balance();

        // safe operations not used due to operator
        if new_junior < old_junior {
            let loss = old_junior - new_junior;
            let absorbed = loss.min(junior_effective_nav.bits);
            junior_effective_nav.bits -= absorbed;
            junior_loss_balance = junior_loss_balance.safe_add(Fix::new(loss))?;
        } else if new_junior > old_junior {
            let gain = new_junior - old_junior;
            let repair = gain.min(junior_loss_balance.bits);
            junior_loss_balance.bits -= repair;
            junior_effective_nav = junior_effective_nav.safe_add(Fix::new(gain))?;
        }

        // safe operations not used due to operator
        if new_senior < old_senior {
            let loss = old_senior - new_senior;
            let junior_absorbed = loss.min(junior_effective_nav.bits);
            junior_effective_nav.bits -= junior_absorbed;
            let remaining = loss - junior_absorbed;
            let senior_absorbed = remaining.min(senior_effective_nav.bits);
            senior_effective_nav.bits -= senior_absorbed;
            senior_loss_balance = senior_loss_balance.safe_add(Fix::new(remaining))?;
        } else if new_senior > old_senior {
            let gain = new_senior - old_senior;
            let senior_repair = gain.min(senior_loss_balance.bits);
            senior_loss_balance.bits -= senior_repair;
            let residual = gain - senior_repair;

            let util = if senior.bits == 0 {
                Fix::default()
            } else if junior_effective_nav.bits == 0 {
                Fix::new(FIX_SCALE)
            } else {
                let protected = self.protected_exposure(senior, junior)?;

                Fix::new(
                    fp_mul(min_coverage.bits, protected.bits)
                        .map_err(YieldTrancheMarketError::from)?
                        / junior_effective_nav.bits,
                )
            };

            let junior_share = match &mut tranche_model {
                TrancheModel::UtilizationGuidedCurve(_) => tranche_model.advance_and_quote(
                    util,
                    now_ts.unwrap_or(
                        Clock::get()
                            .map_err(|_| YieldTrancheMarketError::CannotReadSysvar)?
                            .unix_timestamp
                            .into(),
                    ),
                    senior_loss_balance.bits == 0 && junior_loss_balance.bits == 0,
                )?,
                _ => tranche_model.junior_share(util, junior_effective_nav, senior)?,
            };

            let junior_gain =
                fp_mul(residual, junior_share.bits).map_err(YieldTrancheMarketError::from)?;

            junior_effective_nav = junior_effective_nav.safe_add(Fix::new(junior_gain))?;
            senior_effective_nav =
                senior_effective_nav.safe_add(Fix::new(residual - junior_gain))?;
        }

        self.set_senior_raw_nav(senior)?;
        self.set_junior_raw_nav(junior)?;
        self.set_senior_effective_nav(senior_effective_nav)?;
        self.set_junior_effective_nav(junior_effective_nav)?;
        self.set_senior_loss_balance(senior_loss_balance)?;
        self.set_junior_loss_balance(junior_loss_balance)?;
        self.underlying_mint_exchange_rate = exchange_rate.bits.into();
        self.last_refreshed_slot = slot.into();
        self.mutate_tranche_model(tranche_model.encode()?.as_slice())
    }
}

#[derive(Seeds)]
#[seeds(b"senior_mint", market: Address)]
pub struct SeniorMint;

impl SeniorMint {
    pub fn validate_address(
        senior_mint: &Address,
        market: &Address,
    ) -> Result<(), YieldTrancheMarketError> {
        Self::seeds(market)
            .verify_existing(senior_mint, &crate::ID)
            .map_err(|_| YieldTrancheMarketError::InvalidSeniorMint)?;

        Ok(())
    }
}

#[derive(Seeds)]
#[seeds(b"junior_mint", market: Address)]
pub struct JuniorMint;

impl JuniorMint {
    pub fn validate_address(
        junior_mint: &Address,
        market: &Address,
    ) -> Result<(), YieldTrancheMarketError> {
        Self::seeds(market)
            .verify_existing(junior_mint, &crate::ID)
            .map_err(|_| YieldTrancheMarketError::InvalidJuniorMint)?;

        Ok(())
    }
}
