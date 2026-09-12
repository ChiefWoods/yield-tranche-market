use quasar_lang::{
    keys_eq,
    pda::find_program_address_const,
    prelude::{AccountView, Address},
};
use quasar_spl::{get_associated_token_address_with_program_const, SPL_TOKEN_ID};
use yield_tranche_market_core::{fixed::Fix, utils::read_u64};

use crate::{errors::YieldTrancheMarketError, utils::read_address};

use super::{require_account_count, scaled_ratio};

pub const ACCOUNT_COUNT: usize = 6;

// HysTabVUfmQBFcmzu1ctRd1Y1fxd66RBpboy1bmtDSQQ
pub const HYLO_PROGRAM: Address = Address::new_from_array([
    252, 76, 145, 200, 184, 154, 163, 121, 164, 148, 177, 58, 96, 128, 21, 37, 61, 78, 56, 24, 51,
    154, 155, 244, 236, 32, 127, 136, 39, 150, 113, 225,
]);

const HYLO_DISCRIMINATOR: [u8; 8] = [114, 161, 169, 210, 204, 175, 149, 174];
const HYLO_STABLECOIN_MINT_OFFSET: usize = 8 + 32 * 3;
const TOKEN_ACCOUNT_AMOUNT_OFFSET: usize = 64;
const MINT_SUPPLY_OFFSET: usize = 36;

pub(crate) fn validate(
    underlying_mint: &Address,
    accounts: &[AccountView],
) -> Result<(), YieldTrancheMarketError> {
    require_account_count(accounts, ACCOUNT_COUNT)?;
    let [hylo, pool_config, stablecoin_mint, pool_auth, stablecoin_pool, lp_token_mint] = accounts
    else {
        return Err(YieldTrancheMarketError::InvalidAccountCount);
    };

    for account in [hylo, pool_config] {
        if !account.owned_by(&HYLO_PROGRAM) {
            return Err(YieldTrancheMarketError::InvalidAccountOwner);
        }
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

    let expected_pool_config = find_program_address_const(&[b"pool_config"], &HYLO_PROGRAM).0;
    let expected_pool_auth = find_program_address_const(&[b"pool_auth"], &HYLO_PROGRAM).0;
    let expected_stablecoin_mint = find_program_address_const(&[b"hyUSD"], &HYLO_PROGRAM).0;
    let expected_lp_token_mint = find_program_address_const(&[b"staked_hyUSD"], &HYLO_PROGRAM).0;
    let expected_stablecoin_pool = get_associated_token_address_with_program_const(
        pool_auth.address(),
        stablecoin_mint.address(),
        &SPL_TOKEN_ID,
    )
    .0;
    if !keys_eq(pool_config.address(), &expected_pool_config)
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
