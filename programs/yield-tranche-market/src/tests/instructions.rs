//! Successful instruction, lifecycle, and composed-route coverage.

use super::*;

use yield_tranche_market_core::tranche_model::TrancheModel;

#[test]
fn create_config_creates_admin_config() {
    let source = hylo_source();
    let pdas = market_pdas(address_of(&source.mint));
    let mut svm = setup();

    let initialized =
        svm.process_instruction(&create_config_ix(&pdas), &[empty_account(pdas.config)]);
    initialized.assert_success();

    let config_account = initialized.account(&pubkey(pdas.config)).unwrap();
    let config = decode_config(&config_account.data);
    assert_eq!(config.admin, ADMIN);
}

#[test]
fn create_market_initializes_mints_and_vaults() {
    for source in [fixtures::hylo_source(), fixtures::huma_source()] {
        for model in [point_curve(), guided_curve(), dynamic_leverage(), subsidy()] {
            let pdas = market_pdas(address_of(&source.mint));
            let mut svm = setup();
            let initialized =
                svm.process_instruction(&create_config_ix(&pdas), &[empty_account(pdas.config)]);
            initialized.assert_success();
            let mut accounts = initialized.accounts;
            accounts.extend(market_creation_accounts(&pdas));

            let created = svm.process_instruction(
                &create_market_ix_with(&pdas, &source, MIN_COVERAGE, JUNIOR_EXPOSURE_BETA, &model),
                &accounts,
            );
            created.assert_success();

            let market_account = created.account(&pubkey(pdas.market)).unwrap();
            let market = decode_market(&market_account.data);
            assert_eq!(market.underlying_mint, pdas.underlying_mint);
            assert_eq!(market.source, source.source);
            assert_eq!(market.senior_raw_nav, 0);
            assert_eq!(market.junior_raw_nav, 0);
            assert_eq!(market.senior_effective_nav, 0);
            assert_eq!(market.junior_effective_nav, 0);
            assert_eq!(market.senior_loss_balance, 0);
            assert_eq!(market.junior_loss_balance, 0);
            assert_eq!(market.underlying_mint_exchange_rate, 0);
            assert_eq!(market.min_coverage, MIN_COVERAGE);
            assert_eq!(market.junior_exposure_beta, JUNIOR_EXPOSURE_BETA);
            assert_eq!(market.last_refreshed_slot, 0);
            assert_eq!(market.bump, pdas.market_bump);

            let model_bytes: std::vec::Vec<u8> = market.tranche_model.iter().copied().collect();
            let decoded_model = TrancheModel::decode(&model_bytes).unwrap();
            match model {
                TrancheModel::UtilizationGuidedCurve(expected) => {
                    let TrancheModel::UtilizationGuidedCurve(actual) = decoded_model else {
                        panic!("created market did not persist the utilization-guided model");
                    };
                    assert_eq!(actual.target_utilization, expected.target_utilization);
                    assert_eq!(
                        actual.initial_junior_share_at_target,
                        expected.initial_junior_share_at_target
                    );
                    assert_eq!(
                        actual.current_junior_share_at_target,
                        expected.initial_junior_share_at_target
                    );
                    assert_eq!(actual.last_target_shift_ts, TEST_TIMESTAMP);
                }
                other => assert_eq!(decoded_model, other),
            }

            for mint_address in [pdas.senior_mint, pdas.junior_mint] {
                let mint =
                    Mint::unpack(&created.account(&pubkey(mint_address)).unwrap().data).unwrap();
                assert_eq!(mint.decimals, crate::constants::MINT_DECIMALS);
                assert_eq!(mint.supply, 0);
                assert_eq!(mint.mint_authority.unwrap(), pubkey(pdas.market));
                assert!(mint.freeze_authority.is_none());
            }
            let vault =
                TokenAccount::unpack(&created.account(&pubkey(pdas.market_vault)).unwrap().data)
                    .unwrap();
            assert_eq!(vault.owner, pubkey(pdas.market));
            assert_eq!(vault.mint, pubkey(pdas.underlying_mint));
            assert_eq!(vault.amount, 0);
        }
    }
}

#[test]
fn refresh_market_writes_source_exchange_rate() {
    for source in [fixtures::hylo_source(), fixtures::huma_source()] {
        let pdas = market_pdas(address_of(&source.mint));
        let mut svm = setup();
        let accounts = do_create_market(&mut svm, &pdas, &source);
        let before = decode_market(&account_in(&accounts, &pdas.market).data);
        let vault_before = token_amount(account_in(&accounts, &pdas.market_vault));
        let senior_supply_before = mint_supply(account_in(&accounts, &pdas.senior_mint));
        let junior_supply_before = mint_supply(account_in(&accounts, &pdas.junior_mint));

        svm.sysvars.warp_to_slot(TEST_SLOT + 2);
        svm.warp_to_timestamp(TEST_TIMESTAMP + 1);
        let refresh_slot = svm.sysvars.clock.slot;
        let refreshed = svm.process_instruction(&refresh_market_ix(&pdas, &source), &accounts);
        refreshed.assert_success();

        let after = decode_market(&refreshed.account(&pubkey(pdas.market)).unwrap().data);
        assert_eq!(
            after.underlying_mint_exchange_rate,
            source.expected_exchange_rate
        );
        assert_eq!(after.last_refreshed_slot, refresh_slot);
        assert_eq!(after.underlying_mint, before.underlying_mint);
        assert_eq!(after.source, before.source);
        assert_eq!(after.min_coverage, before.min_coverage);
        assert_eq!(after.junior_exposure_beta, before.junior_exposure_beta);
        assert_eq!(
            token_amount(refreshed.account(&pubkey(pdas.market_vault)).unwrap()),
            vault_before
        );
        assert_eq!(
            mint_supply(refreshed.account(&pubkey(pdas.senior_mint)).unwrap()),
            senior_supply_before
        );
        assert_eq!(
            mint_supply(refreshed.account(&pubkey(pdas.junior_mint)).unwrap()),
            junior_supply_before
        );
    }
}

#[test]
fn refresh_market_realizes_severe_losses_junior_first_then_senior() {
    let source = hylo_source();
    let pdas = market_pdas(address_of(&source.mint));
    let mut svm = setup();
    let accounts = do_active_market(&mut svm, &pdas, &source);
    let accounts = do_deposit(&mut svm, &pdas, false, DEPOSIT_AMOUNT, accounts);
    let mut accounts = do_deposit(&mut svm, &pdas, true, DEPOSIT_AMOUNT, accounts);

    let before = decode_market(&account_in(&accounts, &pdas.market).data);
    let backing_address = address_of(&source.remaining[4]);
    let backing_before = token_amount(account_in(&accounts, &backing_address));
    patch_account(&mut accounts, &backing_address, |account| {
        fixtures::patch_u64(
            account,
            crate::source::hylo::TOKEN_ACCOUNT_AMOUNT_OFFSET,
            backing_before / 4,
        );
    });

    svm.sysvars.warp_to_slot(TEST_SLOT + 1);
    let refreshed = svm.process_instruction(&refresh_market_ix(&pdas, &source), &accounts);
    refreshed.assert_success();

    let after = decode_market(&refreshed.account(&pubkey(pdas.market)).unwrap().data);
    let junior_loss = before.junior_raw_nav - after.junior_raw_nav;
    let senior_loss = before.senior_raw_nav - after.senior_raw_nav;
    let junior_absorption = before.junior_effective_nav - junior_loss;
    let senior_remaining_loss = senior_loss - junior_absorption;

    assert!(after.underlying_mint_exchange_rate < before.underlying_mint_exchange_rate);
    assert!(junior_loss > 0);
    assert!(senior_loss > 0);
    assert_eq!(after.junior_effective_nav, 0);
    assert_eq!(after.junior_loss_balance, junior_loss);
    assert_eq!(
        after.senior_effective_nav,
        before.senior_effective_nav - senior_remaining_loss
    );
    assert_eq!(after.senior_loss_balance, senior_remaining_loss);
}

#[test]
fn refresh_market_uses_recovery_to_repair_realized_losses_first() {
    let source = hylo_source();
    let pdas = market_pdas(address_of(&source.mint));
    let mut svm = setup();
    let accounts = do_active_market(&mut svm, &pdas, &source);
    let accounts = do_deposit(&mut svm, &pdas, false, DEPOSIT_AMOUNT, accounts);
    let mut accounts = do_deposit(&mut svm, &pdas, true, DEPOSIT_AMOUNT, accounts);

    let backing_address = address_of(&source.remaining[4]);
    let backing_before = token_amount(account_in(&accounts, &backing_address));
    patch_account(&mut accounts, &backing_address, |account| {
        fixtures::patch_u64(
            account,
            crate::source::hylo::TOKEN_ACCOUNT_AMOUNT_OFFSET,
            backing_before / 4,
        );
    });
    svm.sysvars.warp_to_slot(TEST_SLOT + 1);
    let loss = svm.process_instruction(&refresh_market_ix(&pdas, &source), &accounts);
    loss.assert_success();

    let loss_market = decode_market(&loss.account(&pubkey(pdas.market)).unwrap().data);
    let mut accounts = loss.accounts;
    patch_account(&mut accounts, &backing_address, |account| {
        fixtures::patch_u64(
            account,
            crate::source::hylo::TOKEN_ACCOUNT_AMOUNT_OFFSET,
            backing_before,
        );
    });
    svm.sysvars.warp_to_slot(TEST_SLOT + 2);
    let recovery = svm.process_instruction(&refresh_market_ix(&pdas, &source), &accounts);
    recovery.assert_success();

    let after = decode_market(&recovery.account(&pubkey(pdas.market)).unwrap().data);
    let junior_gain = after.junior_raw_nav - loss_market.junior_raw_nav;
    let senior_gain = after.senior_raw_nav - loss_market.senior_raw_nav;
    let senior_repair = senior_gain.min(loss_market.senior_loss_balance);
    let senior_residual = senior_gain - senior_repair;

    assert!(junior_gain > 0);
    assert!(senior_gain > 0);
    assert_eq!(
        after.junior_loss_balance,
        loss_market.junior_loss_balance - junior_gain.min(loss_market.junior_loss_balance)
    );
    assert_eq!(
        after.senior_loss_balance,
        loss_market.senior_loss_balance - senior_repair
    );
    assert_eq!(after.junior_loss_balance, 0);
    assert_eq!(after.senior_loss_balance, 0);
    assert_eq!(
        after.junior_effective_nav + after.senior_effective_nav,
        loss_market.junior_effective_nav
            + loss_market.senior_effective_nav
            + junior_gain
            + senior_residual
    );
}

#[test]
fn deposit_mints_junior_then_senior_lp_against_effective_nav() {
    let source = hylo_source();
    let pdas = market_pdas(address_of(&source.mint));
    let mut svm = setup();
    let accounts = do_active_market(&mut svm, &pdas, &source);
    let accounts = with_trader_funds(&pdas, accounts);

    let market_before = decode_market(&account_in(&accounts, &pdas.market).data);
    let trader_underlying_before = token_amount(account_in(&accounts, &pdas.trader_underlying));
    let vault_before = token_amount(account_in(&accounts, &pdas.market_vault));
    let junior_expected = expected_lp_out(
        DEPOSIT_AMOUNT,
        u128::from(market_before.underlying_mint_exchange_rate),
        mint_supply(account_in(&accounts, &pdas.junior_mint)),
        u64::from(market_before.junior_effective_nav),
    );

    let junior = svm.process_instruction(
        &deposit_ix(&pdas, false, DEPOSIT_AMOUNT, junior_expected),
        &accounts,
    );
    junior.assert_success();

    let market_after_junior = decode_market(&junior.account(&pubkey(pdas.market)).unwrap().data);
    assert_eq!(
        token_amount(junior.account(&pubkey(pdas.trader_underlying)).unwrap()),
        trader_underlying_before - DEPOSIT_AMOUNT
    );
    assert_eq!(
        token_amount(junior.account(&pubkey(pdas.market_vault)).unwrap()),
        vault_before + DEPOSIT_AMOUNT
    );
    assert_eq!(
        token_amount(junior.account(&pubkey(pdas.trader_junior)).unwrap()),
        junior_expected
    );
    assert_eq!(
        mint_supply(junior.account(&pubkey(pdas.junior_mint)).unwrap()),
        junior_expected
    );
    assert!(u64::from(market_after_junior.junior_raw_nav) > 0);
    assert_eq!(
        market_after_junior.junior_effective_nav,
        market_after_junior.junior_raw_nav
    );

    let trader_underlying_mid =
        token_amount(junior.account(&pubkey(pdas.trader_underlying)).unwrap());
    let vault_mid = token_amount(junior.account(&pubkey(pdas.market_vault)).unwrap());
    let senior_expected = expected_lp_out(
        DEPOSIT_AMOUNT,
        u128::from(market_after_junior.underlying_mint_exchange_rate),
        mint_supply(junior.account(&pubkey(pdas.senior_mint)).unwrap()),
        u64::from(market_after_junior.senior_effective_nav),
    );

    let senior = svm.process_instruction(
        &deposit_ix(&pdas, true, DEPOSIT_AMOUNT, senior_expected),
        &junior.accounts,
    );
    senior.assert_success();

    let market_after_senior = decode_market(&senior.account(&pubkey(pdas.market)).unwrap().data);
    assert_eq!(
        token_amount(senior.account(&pubkey(pdas.trader_underlying)).unwrap()),
        trader_underlying_mid - DEPOSIT_AMOUNT
    );
    assert_eq!(
        token_amount(senior.account(&pubkey(pdas.market_vault)).unwrap()),
        vault_mid + DEPOSIT_AMOUNT
    );
    assert_eq!(
        token_amount(senior.account(&pubkey(pdas.trader_senior)).unwrap()),
        senior_expected
    );
    assert_eq!(
        mint_supply(senior.account(&pubkey(pdas.senior_mint)).unwrap()),
        senior_expected
    );
    assert!(u64::from(market_after_senior.senior_raw_nav) > 0);
    assert_eq!(
        market_after_senior.senior_effective_nav,
        market_after_senior.senior_raw_nav
    );
    assert_eq!(
        mint_supply(senior.account(&pubkey(pdas.junior_mint)).unwrap()),
        junior_expected
    );
}

#[test]
fn withdraw_burns_lp_and_returns_underlying() {
    let source = hylo_source();
    let pdas = market_pdas(address_of(&source.mint));
    let mut svm = setup();
    let accounts = do_active_market(&mut svm, &pdas, &source);
    let accounts = do_deposit(&mut svm, &pdas, false, DEPOSIT_AMOUNT, accounts);

    let market_before = decode_market(&account_in(&accounts, &pdas.market).data);
    let trader_underlying_before = token_amount(account_in(&accounts, &pdas.trader_underlying));
    let trader_junior_before = token_amount(account_in(&accounts, &pdas.trader_junior));
    let vault_before = token_amount(account_in(&accounts, &pdas.market_vault));
    let junior_supply_before = mint_supply(account_in(&accounts, &pdas.junior_mint));
    let burn = trader_junior_before / 2;
    let expected_out = expected_withdraw_out(
        burn,
        u128::from(market_before.underlying_mint_exchange_rate),
        junior_supply_before,
        u64::from(market_before.junior_effective_nav),
    );

    let withdrawn =
        svm.process_instruction(&withdraw_ix(&pdas, false, burn, expected_out), &accounts);
    withdrawn.assert_success();

    let market_after = decode_market(&withdrawn.account(&pubkey(pdas.market)).unwrap().data);
    assert_eq!(
        token_amount(withdrawn.account(&pubkey(pdas.trader_underlying)).unwrap()),
        trader_underlying_before + expected_out
    );
    assert_eq!(
        token_amount(withdrawn.account(&pubkey(pdas.trader_junior)).unwrap()),
        trader_junior_before - burn
    );
    assert_eq!(
        token_amount(withdrawn.account(&pubkey(pdas.market_vault)).unwrap()),
        vault_before - expected_out
    );
    assert_eq!(
        mint_supply(withdrawn.account(&pubkey(pdas.junior_mint)).unwrap()),
        junior_supply_before - burn
    );
    assert!(u64::from(market_after.junior_raw_nav) < u64::from(market_before.junior_raw_nav));
    assert!(
        u64::from(market_after.junior_effective_nav)
            < u64::from(market_before.junior_effective_nav)
    );
}
