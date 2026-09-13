use super::*;

use crate::source::{huma, hylo};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use quasar_svm::{system_program, Account, Pubkey};
use solana_address::Address;
use std::str::FromStr;
use yield_tranche_market_core::{fixed::FIX_SCALE, utils::read_u128};

#[derive(serde::Deserialize)]
struct CliAccountFixture {
    pubkey: String,
    account: CliAccount,
}

#[derive(serde::Deserialize)]
struct CliAccount {
    lamports: u64,
    data: (String, String),
    owner: String,
    executable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SourceKind {
    Hylo,
    Huma,
}

#[derive(Clone)]
pub(super) struct SourceFixture {
    pub kind: SourceKind,
    pub source: u8,
    pub mint: Account,
    pub remaining: std::vec::Vec<Account>,
    pub expected_exchange_rate: u128,
}

fn load_account(json: &str) -> Account {
    let fixture: CliAccountFixture = serde_json::from_str(json).expect("valid CLI account JSON");
    assert_eq!(fixture.account.data.1, "base64");
    let address = Address::from_str(&fixture.pubkey).expect("valid fixture pubkey");
    let owner = Address::from_str(&fixture.account.owner).expect("valid fixture owner");
    Account {
        address: Pubkey::from(address.to_bytes()),
        lamports: fixture.account.lamports,
        data: STANDARD
            .decode(fixture.account.data.0)
            .expect("valid base64 fixture data"),
        owner: Pubkey::from(owner.to_bytes()),
        executable: fixture.account.executable,
    }
}

fn address(value: &str) -> Address {
    Address::from_str(value).unwrap()
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn read_u64(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
}

fn read_address(data: &[u8], offset: usize) -> Address {
    Address::from(<[u8; 32]>::try_from(&data[offset..offset + 32]).unwrap())
}

fn scaled_ratio(numerator: u128, denominator: u64) -> u128 {
    numerator
        .checked_mul(FIX_SCALE)
        .and_then(|value| value.checked_div(u128::from(denominator)))
        .unwrap()
}

fn empty_pda(address: Address) -> Account {
    Account {
        address: pubkey(address),
        lamports: 0,
        data: std::vec::Vec::new(),
        owner: system_program::ID,
        executable: false,
    }
}

fn huma_mode_assets(pool_state: &[u8], mode_config: Address) -> u128 {
    let count = read_u32(pool_state, huma::POOL_STATE_MODE_STATES_OFFSET) as usize;
    let states_start = huma::POOL_STATE_MODE_STATES_OFFSET + 4;
    let keys_length_offset = states_start + count * huma::MODE_STATE_LEN;
    let keys_count = read_u32(pool_state, keys_length_offset) as usize;
    assert_eq!(keys_count, count);
    let keys_start = keys_length_offset + 4;
    for index in 0..count {
        if read_address(pool_state, keys_start + index * 32) == mode_config {
            return read_u128(pool_state, states_start + index * huma::MODE_STATE_LEN).unwrap();
        }
    }
    panic!("fixture pool state does not contain the mode config")
}

pub(super) fn hylo_source() -> SourceFixture {
    let hylo_global = load_account(include_str!("fixtures/hylo.json"));
    let pool_config = load_account(include_str!("fixtures/hylo_pool_config.json"));
    let stablecoin_mint = load_account(include_str!("fixtures/hyusd_mint.json"));
    let pool_auth = empty_pda(address("5YrRAQag9BbJkauDtJkd1vsTquXT6N46oU8rJ66GDxHd"));
    let stablecoin_pool = load_account(include_str!("fixtures/hylo_stablecoin_pool.json"));
    let mint = load_account(include_str!("fixtures/ehyusd_mint.json"));

    assert_eq!(
        address_of(&hylo_global),
        address("9cd2sAfbBvKs4SX9YKo4dcjwP3TgTVQ8dT5koshGcDND")
    );
    assert_eq!(
        address_of_owner(&hylo_global),
        address("HYEXCHtHkBagdStcJCp3xbbb9B7sdMdWXFNj6mdsG4hn")
    );
    assert_eq!(
        address_of(&pool_config),
        address("2jk7miWrsTbt5hUSaCXPkEQPvuUMgbFLpgMzMQw3Z6ar")
    );
    assert_eq!(
        address_of_owner(&pool_config),
        address("HysTabVUfmQBFcmzu1ctRd1Y1fxd66RBpboy1bmtDSQQ")
    );
    assert_eq!(
        address_of(&stablecoin_mint),
        address("5YMkXAYccHSGnHn9nob9xEvv6Pvka9DZWH7nTbotTu9E")
    );
    assert_eq!(
        address_of(&mint),
        address("HnnGv3HrSqjRpgdFmx7vQGjntNEoex1SU4e9Lxcxuihz")
    );
    assert_eq!(
        &hylo_global.data[..hylo::HYLO_DISCRIMINATOR.len()],
        &hylo::HYLO_DISCRIMINATOR
    );
    assert_eq!(
        read_address(&hylo_global.data, hylo::HYLO_STABLECOIN_MINT_OFFSET),
        address_of(&stablecoin_mint)
    );

    let backing = u128::from(read_u64(
        &stablecoin_pool.data,
        hylo::TOKEN_ACCOUNT_AMOUNT_OFFSET,
    ));
    let supply = read_u64(&mint.data, hylo::MINT_SUPPLY_OFFSET);
    assert!(backing > 0);
    assert!(supply > 0);

    SourceFixture {
        kind: SourceKind::Hylo,
        source: 0,
        expected_exchange_rate: scaled_ratio(backing, supply),
        mint: mint.clone(),
        remaining: std::vec![
            hylo_global,
            pool_config,
            stablecoin_mint,
            pool_auth,
            stablecoin_pool,
            mint,
        ],
    }
}

pub(super) fn huma_source() -> SourceFixture {
    let pool_config = load_account(include_str!("fixtures/huma_pool_config.json"));
    let pool_state = load_account(include_str!("fixtures/huma_pool_state.json"));
    let mode_config = load_account(include_str!("fixtures/huma_mode_config.json"));
    let mint = load_account(include_str!("fixtures/pst_mint.json"));

    assert_eq!(
        address_of(&pool_config),
        address("28hFhD21Nka3stL27a8zZ4nRLgaDVxRYwJgeEVgeakzS")
    );
    assert_eq!(
        address_of(&pool_state),
        address("iFgP2EbzHUZzMjqbjaagJQ8zmn6as3Hw95aVUKm67od")
    );
    assert_eq!(
        address_of(&mode_config),
        address("3FhoMDyKzQqxtGxnz9DfysfoGQKvgDnSFjoDGgguDCQN")
    );
    assert_eq!(
        address_of(&mint),
        address("59obFNBzyTBGowrkif5uK7ojS58vsuWz3ZCvg6tfZAGw")
    );
    assert_eq!(
        address_of_owner(&pool_config),
        address("HumaXepHnjaRCpjYTokxY4UtaJcmx41prQ8cxGmFC5fn")
    );
    assert_eq!(
        &pool_config.data[..huma::POOL_CONFIG_DISCRIMINATOR.len()],
        &huma::POOL_CONFIG_DISCRIMINATOR
    );
    assert_eq!(
        &pool_state.data[..huma::POOL_STATE_DISCRIMINATOR.len()],
        &huma::POOL_STATE_DISCRIMINATOR
    );
    assert_eq!(
        &mode_config.data[..huma::MODE_CONFIG_DISCRIMINATOR.len()],
        &huma::MODE_CONFIG_DISCRIMINATOR
    );

    let assets = huma_mode_assets(&pool_state.data, address_of(&mode_config));
    let supply = read_u64(&mint.data, huma::MINT_SUPPLY_OFFSET);
    assert!(assets > 0);
    assert!(supply > 0);

    SourceFixture {
        kind: SourceKind::Huma,
        source: 1,
        expected_exchange_rate: scaled_ratio(assets, supply),
        mint: mint.clone(),
        remaining: std::vec![pool_config, pool_state, mode_config, mint],
    }
}

pub(super) fn remaining_metas(source: &SourceFixture) -> std::vec::Vec<AccountMeta> {
    source
        .remaining
        .iter()
        .map(|account| AccountMeta::new_readonly(address_of(account), false))
        .collect()
}

pub(super) fn patch_u64(account: &mut Account, offset: usize, value: u64) {
    account.data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn hylo_fixture_matches_captured_earn_pool_accounts() {
    let source = hylo_source();
    assert_eq!(source.kind, SourceKind::Hylo);
    assert_eq!(source.remaining.len(), hylo::ACCOUNT_COUNT);
    assert_eq!(
        &source.remaining[0].data[..hylo::HYLO_DISCRIMINATOR.len()],
        &hylo::HYLO_DISCRIMINATOR
    );
    assert_eq!(
        read_address(&source.remaining[0].data, hylo::HYLO_STABLECOIN_MINT_OFFSET),
        address_of(&source.remaining[2])
    );
    assert_eq!(address_of(&source.remaining[5]), address_of(&source.mint));
    assert!(source.expected_exchange_rate > 0);
    assert_mint_decimals(&source.mint, 6);
}

#[test]
fn huma_fixture_matches_captured_pool_and_pst_mint() {
    let source = huma_source();
    assert_eq!(source.kind, SourceKind::Huma);
    assert_eq!(source.remaining.len(), huma::ACCOUNT_COUNT);
    assert_eq!(
        &source.remaining[0].data[..huma::POOL_CONFIG_DISCRIMINATOR.len()],
        &huma::POOL_CONFIG_DISCRIMINATOR
    );
    assert_eq!(
        &source.remaining[1].data[..huma::POOL_STATE_DISCRIMINATOR.len()],
        &huma::POOL_STATE_DISCRIMINATOR
    );
    assert_eq!(address_of(&source.remaining[3]), address_of(&source.mint));
    assert!(source.expected_exchange_rate > 0);
    assert_mint_decimals(&source.mint, 6);
}
