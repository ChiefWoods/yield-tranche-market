extern crate std;

mod errors;
mod fixtures;
mod instructions;

use fixtures::{huma_source, hylo_source, remaining_metas, SourceFixture};
use quasar_spl::get_associated_token_address_const;
use quasar_svm::{
    system_program,
    token::{self, Mint, TokenAccount},
    Account, ExecutionResult, Pubkey, QuasarSvm, SPL_ASSOCIATED_TOKEN_PROGRAM_ID,
    SPL_TOKEN_PROGRAM_ID,
};
use solana_address::Address;
use solana_instruction::{AccountMeta, Instruction};
use solana_program_pack::Pack;
use yield_tranche_market_client as client;
use yield_tranche_market_core::{
    fixed::{Fix, Percentage},
    tranche_model::{
        CurvePoint, DynamicLeverage, PointCurve, Subsidy, TrancheModel, UtilizationGuidedCurve,
    },
};

const TEST_SLOT: u64 = 100;
const TEST_TIMESTAMP: i64 = 1_800_000_000;
const USER_LAMPORTS: u64 = 100_000_000_000;
const USER_UNDERLYING: u64 = 10_000_000_000_000;
const DEPOSIT_AMOUNT: u64 = 1_000_000;
const MIN_COVERAGE: u16 = 0;
const JUNIOR_EXPOSURE_BETA: u16 = 0;

const ADMIN: Address = Address::new_from_array([1; 32]);
const TRADER: Address = Address::new_from_array([2; 32]);
const STRANGER: Address = Address::new_from_array([4; 32]);
const OTHER_MINT: Address = Address::new_from_array([8; 32]);
const FAKE_SOURCE: Address = Address::new_from_array([9; 32]);

fn program_id() -> Address {
    crate::ID
}

fn pubkey(address: Address) -> Pubkey {
    Pubkey::from(address.to_bytes())
}

fn address_of(account: &Account) -> Address {
    Address::from(account.address.to_bytes())
}

fn address_of_owner(account: &Account) -> Address {
    Address::from(account.owner.to_bytes())
}

fn empty_account(address: Address) -> Account {
    Account {
        address: pubkey(address),
        lamports: 0,
        data: std::vec::Vec::new(),
        owner: system_program::ID,
        executable: false,
    }
}

fn system_account(address: Address) -> Account {
    token::create_keyed_system_account(&pubkey(address), USER_LAMPORTS)
}

fn mint_account(address: Address, decimals: u8) -> Account {
    let mint = Mint {
        decimals,
        is_initialized: true,
        ..Mint::default()
    };
    token::create_keyed_mint_account(&pubkey(address), &mint)
}

fn setup() -> QuasarSvm {
    let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let elf = std::fs::read(manifest_dir.join("../../target/deploy/yield_tranche_market.so"))
        .expect("run `quasar build` before tests");
    let mut svm = QuasarSvm::new()
        .with_program(&pubkey(program_id()), &elf)
        .with_token_program()
        .with_compute_budget(1_400_000);
    svm.sysvars.clock.slot = TEST_SLOT;
    svm.warp_to_timestamp(TEST_TIMESTAMP);
    for account in [
        system_account(ADMIN),
        system_account(TRADER),
        system_account(STRANGER),
        mint_account(OTHER_MINT, 9),
        empty_account(FAKE_SOURCE),
    ] {
        svm.set_account(account);
    }
    svm.set_account(empty_account(event_authority()));
    for source in [hylo_source(), huma_source()] {
        svm.set_account(source.mint.clone());
        for account in source.remaining {
            svm.set_account(account);
        }
    }
    svm
}

#[derive(Clone, Debug)]
struct MarketPdas {
    config: Address,
    market: Address,
    underlying_mint: Address,
    senior_mint: Address,
    junior_mint: Address,
    market_vault: Address,
    trader_underlying: Address,
    trader_senior: Address,
    trader_junior: Address,
    event_authority: Address,
    _config_bump: u8,
    market_bump: u8,
}

fn event_authority() -> Address {
    Address::find_program_address(&[b"__event_authority"], &program_id()).0
}

fn ata(wallet: Address, mint: Address) -> Address {
    let derived = address_of(&token::create_keyed_associated_token_account(
        &pubkey(wallet),
        &pubkey(mint),
        0,
    ));
    assert_eq!(
        derived,
        get_associated_token_address_const(&wallet, &mint).0,
        "QuasarSvm and quasar-spl ATA derivation must agree",
    );
    derived
}

fn market_pdas(underlying_mint: Address) -> MarketPdas {
    let (config, config_bump) = client::find_config_address(&program_id());
    let (market, market_bump) = client::find_market_address(&underlying_mint, &program_id());
    let (senior_mint, _) = client::find_senior_mint_address(&market, &program_id());
    let (junior_mint, _) = client::find_junior_mint_address(&market, &program_id());
    MarketPdas {
        config,
        market,
        underlying_mint,
        senior_mint,
        junior_mint,
        market_vault: ata(market, underlying_mint),
        trader_underlying: ata(TRADER, underlying_mint),
        trader_senior: ata(TRADER, senior_mint),
        trader_junior: ata(TRADER, junior_mint),
        event_authority: event_authority(),
        _config_bump: config_bump,
        market_bump,
    }
}

fn token_account(wallet: Address, mint: Address, amount: u64) -> Account {
    token::create_keyed_associated_token_account(&pubkey(wallet), &pubkey(mint), amount)
}

fn token_amount(account: &Account) -> u64 {
    TokenAccount::unpack(&account.data).unwrap().amount
}

fn mint_supply(account: &Account) -> u64 {
    Mint::unpack(&account.data).unwrap().supply
}

fn assert_mint_decimals(account: &Account, expected: u8) {
    assert_eq!(Mint::unpack(&account.data).unwrap().decimals, expected);
}

fn account_in<'a>(accounts: &'a [Account], address: &Address) -> &'a Account {
    accounts
        .iter()
        .find(|account| address_of(account) == *address)
        .unwrap_or_else(|| panic!("account {address} absent from result"))
}

fn decode_config(data: &[u8]) -> client::state::Config {
    wincode::deserialize(data).unwrap()
}

fn decode_market(data: &[u8]) -> client::state::Market {
    wincode::deserialize(data).unwrap()
}

fn expect_custom_error(result: &ExecutionResult, error: client::YieldTrancheMarketError) {
    result.assert_error(quasar_svm::ProgramError::Custom(error as u32));
}

fn percentage(value: u16) -> Percentage {
    Percentage::new(value)
}

fn point_curve() -> TrancheModel {
    TrancheModel::PointCurve(PointCurve {
        points: [
            CurvePoint::new(percentage(50), percentage(20)),
            CurvePoint::new(percentage(90), percentage(45)),
            CurvePoint::new(percentage(100), percentage(70)),
        ],
    })
}

fn guided_curve() -> TrancheModel {
    TrancheModel::UtilizationGuidedCurve(UtilizationGuidedCurve {
        target_utilization: percentage(90),
        initial_junior_share_at_target: percentage(40),
        min_junior_share_at_target: percentage(10),
        max_target_shift_speed: Fix::new(10_000_000_000),
        zero_utilization_junior_share_discount: percentage(10),
        full_utilization_junior_share_premium: percentage(10),
        current_junior_share_at_target: percentage(40),
        last_target_shift_ts: 0,
    })
}

fn dynamic_leverage() -> TrancheModel {
    TrancheModel::DynamicLeverage(DynamicLeverage {
        target_junior_ratio: percentage(50),
        base_multiplier: percentage(200),
        max_multiplier: percentage(400),
    })
}

fn subsidy() -> TrancheModel {
    TrancheModel::Subsidy(Subsidy {
        senior_yield_to_junior_share: percentage(20),
    })
}

fn create_market_args(
    source: u8,
    min_coverage: u16,
    junior_exposure_beta: u16,
    model: &TrancheModel,
) -> std::vec::Vec<u8> {
    let mut data = std::vec![source];
    data.extend_from_slice(&min_coverage.to_le_bytes());
    data.extend_from_slice(&junior_exposure_beta.to_le_bytes());
    data.extend_from_slice(model.encode().unwrap().as_slice());
    data
}

fn append_create_market_args(
    mut ix: Instruction,
    source: &SourceFixture,
    min_coverage: u16,
    junior_exposure_beta: u16,
    model: &TrancheModel,
) -> Instruction {
    ix.data.extend(create_market_args(
        source.source,
        min_coverage,
        junior_exposure_beta,
        model,
    ));
    ix
}

fn create_config_ix(pdas: &MarketPdas) -> Instruction {
    client::Create_configInstruction {
        admin: ADMIN,
        config: pdas.config,
        system_program: Address::from(system_program::ID.to_bytes()),
    }
    .into()
}

fn create_market_ix(pdas: &MarketPdas, source: &SourceFixture) -> Instruction {
    create_market_ix_with(
        pdas,
        source,
        MIN_COVERAGE,
        JUNIOR_EXPOSURE_BETA,
        &point_curve(),
    )
}

fn create_market_ix_with(
    pdas: &MarketPdas,
    source: &SourceFixture,
    min_coverage: u16,
    junior_exposure_beta: u16,
    model: &TrancheModel,
) -> Instruction {
    let ix = client::Create_marketInstruction {
        admin: ADMIN,
        config: pdas.config,
        underlying_mint: pdas.underlying_mint,
        market: pdas.market,
        senior_mint: pdas.senior_mint,
        junior_mint: pdas.junior_mint,
        market_vault: pdas.market_vault,
        system_program: Address::from(system_program::ID.to_bytes()),
        token_program: Address::from(SPL_TOKEN_PROGRAM_ID.to_bytes()),
        associated_token_program: Address::from(SPL_ASSOCIATED_TOKEN_PROGRAM_ID.to_bytes()),
        event_authority: pdas.event_authority,
        program: program_id(),
    }
    .into();
    append_create_market_args(ix, source, min_coverage, junior_exposure_beta, model)
}

fn refresh_market_ix(pdas: &MarketPdas, source: &SourceFixture) -> Instruction {
    client::Refresh_marketInstruction {
        market: pdas.market,
        underlying_mint: pdas.underlying_mint,
        event_authority: pdas.event_authority,
        program: program_id(),
        remaining_accounts: remaining_metas(source),
    }
    .into()
}

fn deposit_ix(
    pdas: &MarketPdas,
    is_senior: bool,
    amount_in: u64,
    min_amount_out: u64,
) -> Instruction {
    let tranche_mint = if is_senior {
        pdas.senior_mint
    } else {
        pdas.junior_mint
    };
    client::DepositInstruction {
        authority: TRADER,
        market: pdas.market,
        underlying_mint: pdas.underlying_mint,
        tranche_mint,
        authority_underlying_token_account: pdas.trader_underlying,
        market_vault: pdas.market_vault,
        authority_tranche_token_account: ata(TRADER, tranche_mint),
        system_program: Address::from(system_program::ID.to_bytes()),
        token_program: Address::from(SPL_TOKEN_PROGRAM_ID.to_bytes()),
        associated_token_program: Address::from(SPL_ASSOCIATED_TOKEN_PROGRAM_ID.to_bytes()),
        event_authority: pdas.event_authority,
        program: program_id(),
        is_senior,
        amount_in,
        min_amount_out,
    }
    .into()
}

fn withdraw_ix(
    pdas: &MarketPdas,
    is_senior: bool,
    amount_in: u64,
    min_amount_out: u64,
) -> Instruction {
    let tranche_mint = if is_senior {
        pdas.senior_mint
    } else {
        pdas.junior_mint
    };
    client::WithdrawInstruction {
        authority: TRADER,
        market: pdas.market,
        underlying_mint: pdas.underlying_mint,
        tranche_mint,
        authority_receipt_token_account: ata(TRADER, tranche_mint),
        market_vault: pdas.market_vault,
        authority_underlying_token_account: pdas.trader_underlying,
        system_program: Address::from(system_program::ID.to_bytes()),
        token_program: Address::from(SPL_TOKEN_PROGRAM_ID.to_bytes()),
        associated_token_program: Address::from(SPL_ASSOCIATED_TOKEN_PROGRAM_ID.to_bytes()),
        event_authority: pdas.event_authority,
        program: program_id(),
        is_senior,
        amount_in,
        min_amount_out,
    }
    .into()
}

fn market_creation_accounts(pdas: &MarketPdas) -> std::vec::Vec<Account> {
    std::vec![
        empty_account(pdas.market),
        empty_account(pdas.senior_mint),
        empty_account(pdas.junior_mint),
        token_account(pdas.market, pdas.underlying_mint, 0),
    ]
}

fn do_create_config(svm: &mut QuasarSvm, pdas: &MarketPdas) -> std::vec::Vec<Account> {
    let initialized =
        svm.process_instruction(&create_config_ix(pdas), &[empty_account(pdas.config)]);
    initialized.assert_success();
    initialized.accounts
}

fn do_create_market(
    svm: &mut QuasarSvm,
    pdas: &MarketPdas,
    source: &SourceFixture,
) -> std::vec::Vec<Account> {
    do_create_market_with(
        svm,
        pdas,
        source,
        MIN_COVERAGE,
        JUNIOR_EXPOSURE_BETA,
        &point_curve(),
    )
}

fn do_create_market_with(
    svm: &mut QuasarSvm,
    pdas: &MarketPdas,
    source: &SourceFixture,
    min_coverage: u16,
    junior_exposure_beta: u16,
    model: &TrancheModel,
) -> std::vec::Vec<Account> {
    let mut accounts = do_create_config(svm, pdas);
    accounts.extend(market_creation_accounts(pdas));
    let created = svm.process_instruction(
        &create_market_ix_with(pdas, source, min_coverage, junior_exposure_beta, model),
        &accounts,
    );
    created.assert_success();
    created.accounts
}

fn do_refresh_market(
    svm: &mut QuasarSvm,
    pdas: &MarketPdas,
    source: &SourceFixture,
    accounts: std::vec::Vec<Account>,
) -> std::vec::Vec<Account> {
    let refreshed = svm.process_instruction(&refresh_market_ix(pdas, source), &accounts);
    refreshed.assert_success();
    refreshed.accounts
}

fn do_active_market(
    svm: &mut QuasarSvm,
    pdas: &MarketPdas,
    source: &SourceFixture,
) -> std::vec::Vec<Account> {
    let created = do_create_market(svm, pdas, source);
    do_refresh_market(svm, pdas, source, created)
}

fn ensure_account(accounts: &mut std::vec::Vec<Account>, account: Account) {
    if accounts
        .iter()
        .all(|existing| existing.address != account.address)
    {
        accounts.push(account);
    }
}

fn replace_account(accounts: &mut std::vec::Vec<Account>, account: Account) {
    if let Some(existing) = accounts
        .iter_mut()
        .find(|existing| existing.address == account.address)
    {
        *existing = account;
    } else {
        accounts.push(account);
    }
}

fn patch_account(accounts: &mut [Account], address: &Address, patch: impl FnOnce(&mut Account)) {
    let account = accounts
        .iter_mut()
        .find(|account| address_of(account) == *address)
        .unwrap_or_else(|| panic!("account {address} absent from snapshot"));
    patch(account);
}

fn encode_market(market: &client::state::Market) -> std::vec::Vec<u8> {
    wincode::serialize(market).unwrap()
}

fn with_trader_funds(
    pdas: &MarketPdas,
    mut accounts: std::vec::Vec<Account>,
) -> std::vec::Vec<Account> {
    ensure_account(
        &mut accounts,
        token_account(TRADER, pdas.underlying_mint, USER_UNDERLYING),
    );
    ensure_account(&mut accounts, token_account(TRADER, pdas.senior_mint, 0));
    ensure_account(&mut accounts, token_account(TRADER, pdas.junior_mint, 0));
    accounts
}

fn do_deposit(
    svm: &mut QuasarSvm,
    pdas: &MarketPdas,
    is_senior: bool,
    amount_in: u64,
    accounts: std::vec::Vec<Account>,
) -> std::vec::Vec<Account> {
    let accounts = with_trader_funds(pdas, accounts);
    let deposited = svm.process_instruction(&deposit_ix(pdas, is_senior, amount_in, 1), &accounts);
    deposited.assert_success();
    deposited.accounts
}

fn expected_lp_out(amount_in: u64, exchange_rate: u128, supply: u64, effective_nav: u64) -> u64 {
    let raw_value = crate::state::nav(amount_in, Fix::new(exchange_rate)).unwrap();
    if supply == 0 {
        u64::try_from(raw_value.bits).unwrap()
    } else {
        u64::try_from(
            crate::utils::u128_mul_div(
                raw_value.bits,
                u128::from(supply),
                u128::from(effective_nav),
            )
            .unwrap(),
        )
        .unwrap()
    }
}

fn expected_withdraw_out(
    amount_in: u64,
    exchange_rate: u128,
    supply: u64,
    effective_nav: u64,
) -> u64 {
    let claim = crate::utils::u128_mul_div(
        u128::from(effective_nav),
        u128::from(amount_in),
        u128::from(supply.min(amount_in)),
    )
    .unwrap();
    u64::try_from(claim / exchange_rate).unwrap()
}
