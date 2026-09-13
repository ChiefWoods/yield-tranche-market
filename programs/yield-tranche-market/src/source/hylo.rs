use quasar_lang::{
    keys_eq,
    prelude::{AccountView, Address},
};
use quasar_spl::{ATA_PROGRAM_ID, SPL_TOKEN_ID};
use yield_tranche_market_core::{fixed::Fix, utils::read_u64};

use crate::{errors::YieldTrancheMarketError, utils::read_address};

use super::{require_account_count, scaled_ratio};

pub const ACCOUNT_COUNT: usize = 6;

// HysTabVUfmQBFcmzu1ctRd1Y1fxd66RBpboy1bmtDSQQ
pub const HYLO_PROGRAM: Address = Address::new_from_array([
    252, 76, 145, 200, 184, 154, 163, 121, 164, 148, 177, 58, 96, 128, 21, 37, 61, 78, 56, 24, 51,
    154, 155, 244, 236, 32, 127, 136, 39, 150, 113, 225,
]);

// HYEXCHtHkBagdStcJCp3xbbb9B7sdMdWXFNj6mdsG4hn
pub const HYLO_EXCHANGE_PROGRAM: Address = Address::new_from_array([
    245, 187, 72, 160, 4, 116, 48, 134, 197, 164, 152, 189, 233, 219, 27, 124, 201, 65, 103, 243,
    58, 82, 140, 90, 13, 150, 83, 40, 223, 158, 124, 33,
]);

pub(crate) const HYLO_DISCRIMINATOR: [u8; 8] = [114, 161, 169, 210, 204, 175, 149, 174];
pub(crate) const HYLO_STABLECOIN_MINT_OFFSET: usize = 8 + 32 * 3;
pub(crate) const TOKEN_ACCOUNT_AMOUNT_OFFSET: usize = 64;
pub(crate) const MINT_SUPPLY_OFFSET: usize = 36;

pub(crate) fn validate(
    underlying_mint: &Address,
    accounts: &[AccountView],
) -> Result<(), YieldTrancheMarketError> {
    require_account_count(accounts, ACCOUNT_COUNT)?;
    let [hylo, pool_config, stablecoin_mint, pool_auth, stablecoin_pool, lp_token_mint] = accounts
    else {
        return Err(YieldTrancheMarketError::InvalidAccountCount);
    };

    if !hylo.owned_by(&HYLO_EXCHANGE_PROGRAM) || !pool_config.owned_by(&HYLO_PROGRAM) {
        return Err(YieldTrancheMarketError::InvalidAccountOwner);
    }
    for account in [stablecoin_mint, stablecoin_pool, lp_token_mint] {
        if !account.owned_by(&SPL_TOKEN_ID) {
            return Err(YieldTrancheMarketError::InvalidAccountOwner);
        }
    }
    if !keys_eq(lp_token_mint.address(), underlying_mint) {
        return Err(YieldTrancheMarketError::InvalidUnderlyingMint);
    }
    let hylo_data = unsafe { hylo.borrow_unchecked() };
    if hylo_data.get(..8) != Some(&HYLO_DISCRIMINATOR) {
        return Err(YieldTrancheMarketError::InvalidAccountData);
    }
    if !keys_eq(
        &read_address(hylo_data, HYLO_STABLECOIN_MINT_OFFSET)
            .ok_or(YieldTrancheMarketError::InvalidAccountData)?,
        stablecoin_mint.address(),
    ) {
        return Err(YieldTrancheMarketError::InvalidAccountAddress);
    }

    let expected_hylo = Address::find_program_address(&[b"hylo"], &HYLO_EXCHANGE_PROGRAM).0;
    let expected_pool_config = Address::find_program_address(&[b"pool_config"], &HYLO_PROGRAM).0;
    let expected_pool_auth = Address::find_program_address(&[b"pool_auth"], &HYLO_PROGRAM).0;
    let expected_stablecoin_mint =
        Address::find_program_address(&[b"hyUSD"], &HYLO_EXCHANGE_PROGRAM).0;
    let expected_lp_token_mint = Address::find_program_address(&[b"staked_hyUSD"], &HYLO_PROGRAM).0;
    let expected_stablecoin_pool = Address::find_program_address(
        &[
            pool_auth.address().as_ref(),
            SPL_TOKEN_ID.as_ref(),
            stablecoin_mint.address().as_ref(),
        ],
        &ATA_PROGRAM_ID,
    )
    .0;
    if !keys_eq(hylo.address(), &expected_hylo)
        || !keys_eq(pool_config.address(), &expected_pool_config)
        || !keys_eq(pool_auth.address(), &expected_pool_auth)
        || !keys_eq(stablecoin_mint.address(), &expected_stablecoin_mint)
        || !keys_eq(stablecoin_pool.address(), &expected_stablecoin_pool)
        || !keys_eq(lp_token_mint.address(), &expected_lp_token_mint)
    {
        return Err(YieldTrancheMarketError::InvalidAccountAddress);
    }
    Ok(())
}

pub(crate) fn exchange_rate(
    underlying_mint: &Address,
    accounts: &[AccountView],
) -> Result<Fix, YieldTrancheMarketError> {
    validate(underlying_mint, accounts)?;
    let backing = u128::from(
        read_u64(
            unsafe { accounts[4].borrow_unchecked() },
            TOKEN_ACCOUNT_AMOUNT_OFFSET,
        )
        .ok_or(YieldTrancheMarketError::InvalidAccountData)?,
    );
    let supply = read_u64(
        unsafe { accounts[5].borrow_unchecked() },
        MINT_SUPPLY_OFFSET,
    )
    .ok_or(YieldTrancheMarketError::InvalidAccountData)?;
    scaled_ratio(backing, supply)
}
