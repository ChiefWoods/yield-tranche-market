use core::mem::MaybeUninit;

use quasar_lang::prelude::*;
use solana_math::SafeMath;

use crate::errors::YieldTrancheMarketError;

pub fn ix_bytes<T>(value: Option<T>) -> Result<T, YieldTrancheMarketError> {
    value.ok_or(YieldTrancheMarketError::InvalidInstructionData)
}

pub fn read_address(data: &[u8], offset: usize) -> Option<Address> {
    let bytes: [u8; 32] = data.get(offset..offset + 32)?.try_into().ok()?;
    Some(Address::from(bytes))
}

pub fn u128_mul_div(
    value: u128,
    multiplier: u128,
    divisor: u128,
) -> Result<u128, YieldTrancheMarketError> {
    Ok(value.safe_mul(multiplier)?.safe_div(divisor)?)
}

// Keep this in sync with the largest Source variant (`Hylo`).
const MAX_REMAINING_SOURCE_ACCOUNTS: usize = 6;

pub struct RemainingAccountViews {
    views: [MaybeUninit<AccountView>; MAX_REMAINING_SOURCE_ACCOUNTS],
    len: usize,
}

impl RemainingAccountViews {
    pub fn from_remaining(
        remaining: RemainingAccounts<'_>,
    ) -> Result<Self, YieldTrancheMarketError> {
        let mut views: [MaybeUninit<AccountView>; MAX_REMAINING_SOURCE_ACCOUNTS] =
            unsafe { MaybeUninit::uninit().assume_init() };
        let mut len = 0usize;

        for account in remaining.iter() {
            if len >= MAX_REMAINING_SOURCE_ACCOUNTS {
                return Err(YieldTrancheMarketError::InvalidAccountCount);
            }
            let account = account.map_err(|_| YieldTrancheMarketError::InvalidAccountCount)?;
            views[len].write(unsafe { core::ptr::read(account.as_account_view_unchecked()) });
            len += 1;
        }

        Ok(Self { views, len })
    }

    pub fn as_slice(&self) -> &[AccountView] {
        unsafe { core::slice::from_raw_parts(self.views.as_ptr().cast::<AccountView>(), self.len) }
    }
}
