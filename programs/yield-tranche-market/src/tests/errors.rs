//! `ArithmeticOverflow` is not covered by a dedicated fixture.

use super::*;

use crate::source::hylo;

fn active_hylo() -> (QuasarSvm, MarketPdas, SourceFixture, std::vec::Vec<Account>) {
    let source = hylo_source();
    let pdas = market_pdas(address_of(&source.mint));
    let mut svm = setup();
    let accounts = do_active_market(&mut svm, &pdas, &source);
    (svm, pdas, source, accounts)
}

#[test]
fn rejects_create_market_with_invalid_admin() {
    let source = hylo_source();
    let pdas = market_pdas(address_of(&source.mint));
    let mut svm = setup();
    let mut accounts = do_create_config(&mut svm, &pdas);
    accounts.extend(market_creation_accounts(&pdas));
    let mut ix = create_market_ix(&pdas, &source);
    ix.accounts[0] = AccountMeta::new(STRANGER, true);
    let result = svm.process_instruction(&ix, &accounts);
    expect_custom_error(&result, client::YieldTrancheMarketError::InvalidAdmin);
}

#[test]
fn rejects_create_market_with_invalid_source() {
    let source = hylo_source();
    let pdas = market_pdas(address_of(&source.mint));
    let mut svm = setup();
    let mut accounts = do_create_config(&mut svm, &pdas);
    accounts.extend(market_creation_accounts(&pdas));
    let mut ix = create_market_ix(&pdas, &source);
    let disc_len = ix.data.len()
        - create_market_args(
            source.source,
            MIN_COVERAGE,
            JUNIOR_EXPOSURE_BETA,
            &point_curve(),
        )
        .len();
    ix.data[disc_len] = u8::MAX;
    let result = svm.process_instruction(&ix, &accounts);
    expect_custom_error(&result, client::YieldTrancheMarketError::InvalidSource);
}

#[test]
fn rejects_create_market_with_invalid_instruction_data() {
    let source = hylo_source();
    let pdas = market_pdas(address_of(&source.mint));
    let mut svm = setup();
    let mut accounts = do_create_config(&mut svm, &pdas);
    accounts.extend(market_creation_accounts(&pdas));
    let mut ix = create_market_ix(&pdas, &source);
    ix.data.truncate(
        ix.data.len().saturating_sub(
            create_market_args(
                source.source,
                MIN_COVERAGE,
                JUNIOR_EXPOSURE_BETA,
                &point_curve(),
            )
            .len(),
        ),
    );
    let result = svm.process_instruction(&ix, &accounts);
    expect_custom_error(
        &result,
        client::YieldTrancheMarketError::InvalidInstructionData,
    );
}

#[test]
fn rejects_create_market_with_invalid_coverage_configuration() {
    let source = hylo_source();
    let pdas = market_pdas(address_of(&source.mint));
    let mut svm = setup();
    let mut accounts = do_create_config(&mut svm, &pdas);
    accounts.extend(market_creation_accounts(&pdas));
    let ix = create_market_ix_with(&pdas, &source, 101, JUNIOR_EXPOSURE_BETA, &point_curve());
    let result = svm.process_instruction(&ix, &accounts);
    expect_custom_error(
        &result,
        client::YieldTrancheMarketError::InvalidCoverageConfiguration,
    );
}

#[test]
fn rejects_create_market_with_invalid_tranche_model() {
    let source = hylo_source();
    let pdas = market_pdas(address_of(&source.mint));
    let mut svm = setup();
    let mut accounts = do_create_config(&mut svm, &pdas);
    accounts.extend(market_creation_accounts(&pdas));
    let mut ix = create_market_ix(&pdas, &source);
    let disc_len = ix.data.len()
        - create_market_args(
            source.source,
            MIN_COVERAGE,
            JUNIOR_EXPOSURE_BETA,
            &point_curve(),
        )
        .len();
    ix.data.truncate(disc_len);
    ix.data.extend(create_market_args(
        source.source,
        MIN_COVERAGE,
        JUNIOR_EXPOSURE_BETA,
        &point_curve(),
    ));
    // Keep source/coverage/beta and replace the model with a one-point curve.
    ix.data.truncate(disc_len + 5);
    ix.data.extend_from_slice(&[0, 50, 0, 20, 0]);
    let result = svm.process_instruction(&ix, &accounts);
    expect_custom_error(
        &result,
        client::YieldTrancheMarketError::InvalidTrancheModel,
    );
}

#[test]
fn rejects_refresh_market_with_invalid_account_count() {
    let (mut svm, pdas, source, accounts) = active_hylo();
    let mut ix = refresh_market_ix(&pdas, &source);
    ix.accounts.pop();
    let result = svm.process_instruction(&ix, &accounts);
    expect_custom_error(
        &result,
        client::YieldTrancheMarketError::InvalidAccountCount,
    );
}

#[test]
fn rejects_refresh_market_with_invalid_account_owner() {
    let (mut svm, pdas, source, mut accounts) = active_hylo();
    let pool_config = address_of(&source.remaining[1]);
    patch_account(&mut accounts, &pool_config, |account| {
        account.owner = system_program::ID;
    });
    let result = svm.process_instruction(&refresh_market_ix(&pdas, &source), &accounts);
    expect_custom_error(
        &result,
        client::YieldTrancheMarketError::InvalidAccountOwner,
    );
}

#[test]
fn rejects_refresh_market_with_invalid_account_address() {
    let (mut svm, pdas, source, mut accounts) = active_hylo();
    let mut fake = source.remaining[0].clone();
    fake.address = pubkey(FAKE_SOURCE);
    replace_account(&mut accounts, fake);
    let mut ix = refresh_market_ix(&pdas, &source);
    let remaining_start = ix.accounts.len() - source.remaining.len();
    ix.accounts[remaining_start] = AccountMeta::new_readonly(FAKE_SOURCE, false);
    let result = svm.process_instruction(&ix, &accounts);
    expect_custom_error(
        &result,
        client::YieldTrancheMarketError::InvalidAccountAddress,
    );
}

#[test]
fn rejects_refresh_market_with_invalid_account_data() {
    let (mut svm, pdas, source, mut accounts) = active_hylo();
    let hylo = address_of(&source.remaining[0]);
    patch_account(&mut accounts, &hylo, |account| {
        account.data[..hylo::HYLO_DISCRIMINATOR.len()].fill(0);
    });
    let result = svm.process_instruction(&refresh_market_ix(&pdas, &source), &accounts);
    expect_custom_error(&result, client::YieldTrancheMarketError::InvalidAccountData);
}

#[test]
fn rejects_refresh_market_with_invalid_underlying_mint() {
    let (mut svm, pdas, source, mut accounts) = active_hylo();
    ensure_account(&mut accounts, mint_account(OTHER_MINT, 9));
    let mut ix = refresh_market_ix(&pdas, &source);
    ix.accounts[1] = AccountMeta::new_readonly(OTHER_MINT, false);
    let result = svm.process_instruction(&ix, &accounts);
    expect_custom_error(
        &result,
        client::YieldTrancheMarketError::InvalidUnderlyingMint,
    );
}

#[test]
fn rejects_refresh_market_with_invalid_supply() {
    let (mut svm, pdas, source, mut accounts) = active_hylo();
    let pool = address_of(&source.remaining[4]);
    patch_account(&mut accounts, &pool, |account| {
        fixtures::patch_u64(account, hylo::TOKEN_ACCOUNT_AMOUNT_OFFSET, 0);
    });
    let result = svm.process_instruction(&refresh_market_ix(&pdas, &source), &accounts);
    expect_custom_error(&result, client::YieldTrancheMarketError::InvalidSupply);
}

#[test]
fn rejects_deposit_with_invalid_amount() {
    let (mut svm, pdas, _source, accounts) = active_hylo();
    let accounts = with_trader_funds(&pdas, accounts);
    let result = svm.process_instruction(&deposit_ix(&pdas, false, 0, 1), &accounts);
    expect_custom_error(&result, client::YieldTrancheMarketError::InvalidAmount);
}

#[test]
fn rejects_deposit_when_market_is_stale() {
    let (mut svm, pdas, _source, accounts) = active_hylo();
    let accounts = with_trader_funds(&pdas, accounts);
    svm.sysvars.warp_to_slot(TEST_SLOT + 2);
    let result = svm.process_instruction(&deposit_ix(&pdas, false, DEPOSIT_AMOUNT, 1), &accounts);
    expect_custom_error(&result, client::YieldTrancheMarketError::MarketStale);
}

#[test]
fn rejects_deposit_when_slippage_exceeded() {
    let (mut svm, pdas, _source, accounts) = active_hylo();
    let accounts = with_trader_funds(&pdas, accounts);
    let result = svm.process_instruction(
        &deposit_ix(&pdas, false, DEPOSIT_AMOUNT, u64::MAX),
        &accounts,
    );
    expect_custom_error(&result, client::YieldTrancheMarketError::SlippageExceeded);
}

#[test]
fn rejects_senior_deposit_with_invalid_senior_mint() {
    let (mut svm, pdas, _source, accounts) = active_hylo();
    let mut accounts = with_trader_funds(&pdas, accounts);
    ensure_account(&mut accounts, token_account(TRADER, pdas.junior_mint, 0));
    let mut ix = deposit_ix(&pdas, true, DEPOSIT_AMOUNT, 1);
    ix.accounts[3] = AccountMeta::new(pdas.junior_mint, false);
    ix.accounts[6] = AccountMeta::new(pdas.trader_junior, false);
    let result = svm.process_instruction(&ix, &accounts);
    expect_custom_error(&result, client::YieldTrancheMarketError::InvalidSeniorMint);
}

#[test]
fn rejects_junior_deposit_with_invalid_junior_mint() {
    let (mut svm, pdas, _source, accounts) = active_hylo();
    let mut accounts = with_trader_funds(&pdas, accounts);
    ensure_account(&mut accounts, token_account(TRADER, pdas.senior_mint, 0));
    let mut ix = deposit_ix(&pdas, false, DEPOSIT_AMOUNT, 1);
    ix.accounts[3] = AccountMeta::new(pdas.senior_mint, false);
    ix.accounts[6] = AccountMeta::new(pdas.trader_senior, false);
    let result = svm.process_instruction(&ix, &accounts);
    expect_custom_error(&result, client::YieldTrancheMarketError::InvalidJuniorMint);
}

#[test]
fn rejects_senior_deposit_with_insufficient_coverage() {
    let source = hylo_source();
    let pdas = market_pdas(address_of(&source.mint));
    let mut svm = setup();
    let accounts = do_create_market_with(
        &mut svm,
        &pdas,
        &source,
        100,
        JUNIOR_EXPOSURE_BETA,
        &point_curve(),
    );
    let accounts = do_refresh_market(&mut svm, &pdas, &source, accounts);
    let accounts = with_trader_funds(&pdas, accounts);
    let result = svm.process_instruction(&deposit_ix(&pdas, true, DEPOSIT_AMOUNT, 1), &accounts);
    expect_custom_error(
        &result,
        client::YieldTrancheMarketError::InsufficientCoverage,
    );
}

#[test]
fn rejects_deposit_when_tranche_has_no_effective_nav() {
    let (mut svm, pdas, _source, accounts) = active_hylo();
    let mut accounts = do_deposit(&mut svm, &pdas, false, DEPOSIT_AMOUNT, accounts);
    patch_account(&mut accounts, &pdas.market, |account| {
        let mut market = decode_market(&account.data);
        market.junior_effective_nav = 0;
        account.data = encode_market(&market);
    });
    let result = svm.process_instruction(&deposit_ix(&pdas, false, DEPOSIT_AMOUNT, 1), &accounts);
    expect_custom_error(
        &result,
        client::YieldTrancheMarketError::TrancheHasNoEffectiveNav,
    );
}

#[test]
fn rejects_withdraw_when_tranche_mint_has_no_supply() {
    let (mut svm, pdas, _source, accounts) = active_hylo();
    let mut accounts = with_trader_funds(&pdas, accounts);
    replace_account(&mut accounts, token_account(TRADER, pdas.junior_mint, 1));
    let result = svm.process_instruction(&withdraw_ix(&pdas, false, 1, 1), &accounts);
    expect_custom_error(
        &result,
        client::YieldTrancheMarketError::TrancheMintHasNoSupply,
    );
}

#[test]
fn rejects_withdraw_with_invalid_exchange_rate() {
    let (mut svm, pdas, _source, accounts) = active_hylo();
    let mut accounts = do_deposit(&mut svm, &pdas, false, DEPOSIT_AMOUNT, accounts);
    patch_account(&mut accounts, &pdas.market, |account| {
        let mut market = decode_market(&account.data);
        market.underlying_mint_exchange_rate = 0;
        account.data = encode_market(&market);
    });
    let lp = token_amount(account_in(&accounts, &pdas.trader_junior));
    let result = svm.process_instruction(&withdraw_ix(&pdas, false, lp, 1), &accounts);
    expect_custom_error(
        &result,
        client::YieldTrancheMarketError::InvalidExchangeRate,
    );
}

#[test]
fn rejects_withdraw_when_withdrawal_exceeds_nav() {
    let (mut svm, pdas, _source, accounts) = active_hylo();
    let mut accounts = do_deposit(&mut svm, &pdas, false, DEPOSIT_AMOUNT, accounts);
    patch_account(&mut accounts, &pdas.market, |account| {
        let mut market = decode_market(&account.data);
        market.junior_raw_nav = 0;
        account.data = encode_market(&market);
    });
    let lp = token_amount(account_in(&accounts, &pdas.trader_junior));
    let result = svm.process_instruction(&withdraw_ix(&pdas, false, lp, 1), &accounts);
    expect_custom_error(
        &result,
        client::YieldTrancheMarketError::WithdrawalExceedsNav,
    );
}

#[test]
fn rejects_withdraw_when_vault_balance_is_insufficient() {
    let (mut svm, pdas, _source, accounts) = active_hylo();
    let mut accounts = do_deposit(&mut svm, &pdas, false, DEPOSIT_AMOUNT, accounts);
    replace_account(
        &mut accounts,
        token_account(pdas.market, pdas.underlying_mint, 0),
    );
    let lp = token_amount(account_in(&accounts, &pdas.trader_junior));
    let result = svm.process_instruction(&withdraw_ix(&pdas, false, lp, 1), &accounts);
    expect_custom_error(
        &result,
        client::YieldTrancheMarketError::InsufficientVaultBalance,
    );
}

#[test]
fn rejects_junior_withdraw_with_insufficient_coverage() {
    let source = hylo_source();
    let pdas = market_pdas(address_of(&source.mint));
    let mut svm = setup();
    let accounts = do_create_market_with(&mut svm, &pdas, &source, 50, 100, &point_curve());
    let accounts = do_refresh_market(&mut svm, &pdas, &source, accounts);
    let accounts = do_deposit(&mut svm, &pdas, false, DEPOSIT_AMOUNT, accounts);
    let accounts = do_deposit(&mut svm, &pdas, true, DEPOSIT_AMOUNT, accounts);
    let junior_lp = token_amount(account_in(&accounts, &pdas.trader_junior));
    let result = svm.process_instruction(&withdraw_ix(&pdas, false, junior_lp, 1), &accounts);
    expect_custom_error(
        &result,
        client::YieldTrancheMarketError::InsufficientCoverage,
    );
}
