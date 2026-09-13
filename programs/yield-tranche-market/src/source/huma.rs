use quasar_lang::{
    keys_eq,
    prelude::{AccountView, Address},
};
use quasar_spl::SPL_TOKEN_ID;
use solana_math::SafeMath;
use yield_tranche_market_core::{
    fixed::Fix,
    utils::{read_u128, read_u32, read_u64},
};

use crate::{errors::YieldTrancheMarketError, utils::read_address};

use super::{require_account_count, scaled_ratio};

pub const ACCOUNT_COUNT: usize = 4;

// HumaXepHnjaRCpjYTokxY4UtaJcmx41prQ8cxGmFC5fn
pub const HUMA_PROGRAM: Address = Address::new_from_array([
    251, 63, 152, 244, 219, 92, 3, 53, 121, 175, 5, 30, 117, 61, 23, 252, 123, 36, 148, 117, 221,
    5, 37, 183, 42, 44, 197, 113, 154, 208, 119, 177,
]);

pub(crate) const POOL_CONFIG_DISCRIMINATOR: [u8; 8] = [26, 108, 14, 123, 116, 230, 129, 43];
pub(crate) const POOL_STATE_DISCRIMINATOR: [u8; 8] = [247, 237, 227, 245, 215, 195, 222, 70];
pub(crate) const MODE_CONFIG_DISCRIMINATOR: [u8; 8] = [249, 180, 144, 225, 126, 159, 202, 209];
const MODE_CONFIG_ID_OFFSET: usize = 8 + 2;
const POOL_CONFIG_POOL_ID_OFFSET: usize = 8 + 1 + 32 * 4 + 1;
pub(crate) const POOL_STATE_MODE_STATES_OFFSET: usize = 8 + 1 + 1 + 16;
pub(crate) const MODE_STATE_LEN: usize = 216;
pub(crate) const MINT_SUPPLY_OFFSET: usize = 36;

pub(crate) fn validate(
    underlying_mint: &Address,
    accounts: &[AccountView],
) -> Result<(), YieldTrancheMarketError> {
    require_account_count(accounts, ACCOUNT_COUNT)?;
    let [pool_config, pool_state, mode_config, mode_mint] = accounts else {
        return Err(YieldTrancheMarketError::InvalidAccountCount);
    };
    for account in [pool_config, pool_state, mode_config] {
        if !account.owned_by(&HUMA_PROGRAM) {
            return Err(YieldTrancheMarketError::InvalidAccountOwner);
        }
    }
    if !mode_mint.owned_by(&SPL_TOKEN_ID) {
        return Err(YieldTrancheMarketError::InvalidAccountOwner);
    }
    if !keys_eq(mode_mint.address(), underlying_mint) {
        return Err(YieldTrancheMarketError::InvalidUnderlyingMint);
    }
    let pool_config_data = unsafe { pool_config.borrow_unchecked() };
    let pool_state_data = unsafe { pool_state.borrow_unchecked() };
    let mode_config_data = unsafe { mode_config.borrow_unchecked() };
    validate_discriminator(pool_config_data, POOL_CONFIG_DISCRIMINATOR)?;
    validate_discriminator(pool_state_data, POOL_STATE_DISCRIMINATOR)?;
    validate_discriminator(mode_config_data, MODE_CONFIG_DISCRIMINATOR)?;

    let pool_id = read_address(pool_config_data, POOL_CONFIG_POOL_ID_OFFSET)
        .ok_or(YieldTrancheMarketError::InvalidAccountData)?;
    let expected_pool_config =
        Address::find_program_address(&[b"pool_config", pool_id.as_ref()], &HUMA_PROGRAM).0;
    let expected_pool_state = Address::find_program_address(
        &[b"pool_state", pool_config.address().as_ref()],
        &HUMA_PROGRAM,
    )
    .0;
    let mode_id = read_address(mode_config_data, MODE_CONFIG_ID_OFFSET)
        .ok_or(YieldTrancheMarketError::InvalidAccountData)?;
    let expected_mode_config = Address::find_program_address(
        &[
            b"mode_config",
            pool_config.address().as_ref(),
            mode_id.as_ref(),
        ],
        &HUMA_PROGRAM,
    )
    .0;
    let expected_mode_mint = Address::find_program_address(
        &[
            b"mode_mint",
            pool_config.address().as_ref(),
            mode_config.address().as_ref(),
        ],
        &HUMA_PROGRAM,
    )
    .0;
    if !keys_eq(pool_config.address(), &expected_pool_config)
        || !keys_eq(pool_state.address(), &expected_pool_state)
        || !keys_eq(mode_config.address(), &expected_mode_config)
        || !keys_eq(mode_mint.address(), &expected_mode_mint)
    {
        return Err(YieldTrancheMarketError::InvalidAccountAddress);
    }
    mode_state_assets(pool_state_data, mode_config.address()).map(|_| ())
}

pub(crate) fn exchange_rate(
    underlying_mint: &Address,
    accounts: &[AccountView],
) -> Result<Fix, YieldTrancheMarketError> {
    validate(underlying_mint, accounts)?;
    let assets = mode_state_assets(
        unsafe { accounts[1].borrow_unchecked() },
        accounts[2].address(),
    )?;
    let supply = read_u64(
        unsafe { accounts[3].borrow_unchecked() },
        MINT_SUPPLY_OFFSET,
    )
    .ok_or(YieldTrancheMarketError::InvalidAccountData)?;
    scaled_ratio(assets, supply)
}

fn validate_discriminator(data: &[u8], expected: [u8; 8]) -> Result<(), YieldTrancheMarketError> {
    if data.get(..8) == Some(&expected) {
        Ok(())
    } else {
        Err(YieldTrancheMarketError::InvalidAccountData)
    }
}

fn mode_state_assets(data: &[u8], mode_config: &Address) -> Result<u128, YieldTrancheMarketError> {
    let count = read_u32(data, POOL_STATE_MODE_STATES_OFFSET)
        .ok_or(YieldTrancheMarketError::InvalidAccountData)? as usize;
    let states_start = POOL_STATE_MODE_STATES_OFFSET + 4;
    let keys_length_offset = offset(states_start, count, MODE_STATE_LEN)?;
    let keys_count = read_u32(data, keys_length_offset)
        .ok_or(YieldTrancheMarketError::InvalidAccountData)? as usize;
    if keys_count != count {
        return Err(YieldTrancheMarketError::InvalidAccountData);
    }
    let keys_start = keys_length_offset + 4;
    for index in 0..count {
        let key_offset = offset(keys_start, index, 32)?;
        if keys_eq(
            &read_address(data, key_offset).ok_or(YieldTrancheMarketError::InvalidAccountData)?,
            mode_config,
        ) {
            let state_offset = offset(states_start, index, MODE_STATE_LEN)?;
            return read_u128(data, state_offset)
                .ok_or(YieldTrancheMarketError::InvalidAccountData);
        }
    }
    Err(YieldTrancheMarketError::InvalidAccountAddress)
}

fn offset(base: usize, index: usize, stride: usize) -> Result<usize, YieldTrancheMarketError> {
    Ok((base as u64).safe_add((index as u64).safe_mul(stride as u64)?)? as usize)
}
