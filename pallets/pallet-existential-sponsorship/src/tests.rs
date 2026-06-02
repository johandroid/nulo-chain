use crate::mock;
use crate::{
    AutoUnlockCondition, AutoUnlockMode, Error, Lease, SponsorshipProvider,
    mock::{
        ALT_SPONSOR, BENEFICIARY, CALLER, ExistentialSponsorship, LOW_BALANCE_SPONSOR,
        RuntimeOrigin, SPONSOR, Test, new_test_ext, sponsorship_hold_reason,
        storage_deposit_hold_reason,
    },
};
use frame::{
    deps::frame_support::traits::tokens::fungible::{
        Inspect as FungibleInspect, InspectHold, Mutate as FungibleMutate,
    },
    testing_prelude::*,
};

#[test]
fn sponsor_places_funds_on_hold_for_the_beneficiary() {
    new_test_ext().execute_with(|| {
        assert_ok!(ExistentialSponsorship::sponsor(
            RuntimeOrigin::signed(SPONSOR),
            BENEFICIARY,
            50,
            AutoUnlockMode::Disabled,
            Lease::None,
        ));

        let record = crate::Sponsorships::<Test>::get(BENEFICIARY).expect("record exists");
        assert_eq!(record.sponsor, SPONSOR);
        assert_eq!(record.amount, 50);
        assert_eq!(
            <mock::Balances as FungibleInspect<_>>::balance(&SPONSOR),
            445
        );
        assert_eq!(
            <mock::Balances as InspectHold<_>>::balance_on_hold(
                &sponsorship_hold_reason(),
                &BENEFICIARY,
            ),
            50
        );
        assert_eq!(
            <mock::Balances as InspectHold<_>>::balance_on_hold(
                &storage_deposit_hold_reason(),
                &SPONSOR,
            ),
            5
        );

        let ledger = crate::SponsorLedger::<Test>::get(SPONSOR);
        assert_eq!(ledger.active_amount, 50);
        assert_eq!(ledger.active_count, 1);
    });
}

#[test]
fn sponsor_keeps_a_zero_balance_beneficiary_alive_across_blocks() {
    new_test_ext().execute_with(|| {
        let dust_account = 42;

        assert_eq!(mock::System::providers(&BENEFICIARY), 0);
        assert_ok!(ExistentialSponsorship::sponsor(
            RuntimeOrigin::signed(SPONSOR),
            BENEFICIARY,
            20,
            AutoUnlockMode::Disabled,
            Lease::None,
        ));
        assert_ok!(mock::Balances::force_set_balance(
            RuntimeOrigin::root(),
            dust_account,
            9,
        ));

        for block in 2..=25 {
            mock::System::set_block_number(block);
            let _ = ExistentialSponsorship::on_initialize(block);
        }

        assert_eq!(mock::System::providers(&BENEFICIARY), 1);
        assert_eq!(
            <mock::Balances as FungibleInspect<_>>::balance(&BENEFICIARY),
            0
        );
        assert_eq!(
            <mock::Balances as InspectHold<_>>::balance_on_hold(
                &sponsorship_hold_reason(),
                &BENEFICIARY,
            ),
            20
        );
        assert_eq!(mock::System::providers(&dust_account), 0);
    });
}

#[test]
fn sponsor_minimum_keeps_a_zero_balance_beneficiary_alive_across_blocks() {
    new_test_ext().execute_with(|| {
        let minimum_beneficiary = 43;
        let dust_account = 44;

        assert_eq!(mock::System::providers(&minimum_beneficiary), 0);
        assert_ok!(ExistentialSponsorship::sponsor_minimum(
            RuntimeOrigin::signed(SPONSOR),
            minimum_beneficiary,
            AutoUnlockMode::Disabled,
            Lease::None,
        ));
        assert_ok!(mock::Balances::force_set_balance(
            RuntimeOrigin::root(),
            dust_account,
            9,
        ));

        for block in 2..=25 {
            mock::System::set_block_number(block);
            let _ = ExistentialSponsorship::on_initialize(block);
        }

        assert_eq!(mock::System::providers(&minimum_beneficiary), 1);
        assert_eq!(
            <mock::Balances as FungibleInspect<_>>::balance(&minimum_beneficiary),
            0
        );
        assert_eq!(
            <mock::Balances as InspectHold<_>>::balance_on_hold(
                &sponsorship_hold_reason(),
                &minimum_beneficiary,
            ),
            10
        );
        assert_eq!(mock::System::providers(&dust_account), 0);
    });
}

#[test]
fn unlock_returns_the_sponsored_funds_to_the_sponsor() {
    new_test_ext().execute_with(|| {
        assert_ok!(ExistentialSponsorship::sponsor(
            RuntimeOrigin::signed(SPONSOR),
            BENEFICIARY,
            50,
            AutoUnlockMode::Disabled,
            Lease::None,
        ));

        assert_ok!(ExistentialSponsorship::unlock(
            RuntimeOrigin::signed(SPONSOR),
            BENEFICIARY,
        ));

        assert!(!crate::Sponsorships::<Test>::contains_key(BENEFICIARY));
        assert_eq!(
            <mock::Balances as FungibleInspect<_>>::balance(&SPONSOR),
            500
        );
        assert_eq!(
            <mock::Balances as InspectHold<_>>::balance_on_hold(
                &sponsorship_hold_reason(),
                &BENEFICIARY,
            ),
            0
        );
        assert_eq!(
            <mock::Balances as InspectHold<_>>::balance_on_hold(
                &storage_deposit_hold_reason(),
                &SPONSOR,
            ),
            0
        );
    });
}

#[test]
fn rejects_a_second_active_sponsorship_for_the_same_beneficiary() {
    new_test_ext().execute_with(|| {
        assert_ok!(ExistentialSponsorship::sponsor(
            RuntimeOrigin::signed(SPONSOR),
            BENEFICIARY,
            20,
            AutoUnlockMode::Disabled,
            Lease::None,
        ));

        assert_noop!(
            ExistentialSponsorship::sponsor(
                RuntimeOrigin::signed(ALT_SPONSOR),
                BENEFICIARY,
                20,
                AutoUnlockMode::Disabled,
                Lease::None,
            ),
            Error::<Test>::AlreadySponsored
        );
    });
}

#[test]
fn enforces_the_sponsor_minimum_remaining_balance_rule() {
    new_test_ext().execute_with(|| {
        assert_noop!(
            ExistentialSponsorship::sponsor(
                RuntimeOrigin::signed(LOW_BALANCE_SPONSOR),
                BENEFICIARY,
                120,
                AutoUnlockMode::Disabled,
                Lease::None,
            ),
            Error::<Test>::SponsorMinRemainingViolation
        );
    });
}

#[test]
fn refresh_auto_unlocks_when_the_beneficiary_has_enough_free_balance() {
    new_test_ext().execute_with(|| {
        assert_ok!(ExistentialSponsorship::sponsor(
            RuntimeOrigin::signed(SPONSOR),
            BENEFICIARY,
            50,
            AutoUnlockMode::Enabled(AutoUnlockCondition::FreeGeSponsored),
            Lease::None,
        ));

        assert_ok!(<mock::Balances as FungibleMutate<_>>::mint_into(
            &BENEFICIARY,
            50
        ));
        assert_ok!(ExistentialSponsorship::refresh(
            RuntimeOrigin::signed(CALLER),
            BENEFICIARY,
        ));

        assert!(!crate::Sponsorships::<Test>::contains_key(BENEFICIARY));
        assert_eq!(
            <mock::Balances as FungibleInspect<_>>::balance(&BENEFICIARY),
            50
        );
        assert_eq!(
            <mock::Balances as FungibleInspect<_>>::balance(&SPONSOR),
            500
        );
    });
}

#[test]
fn expiring_sponsorships_are_reclaimed_in_on_initialize() {
    new_test_ext().execute_with(|| {
        assert_ok!(ExistentialSponsorship::sponsor(
            RuntimeOrigin::signed(SPONSOR),
            BENEFICIARY,
            25,
            AutoUnlockMode::Disabled,
            Lease::Until(2),
        ));

        mock::System::set_block_number(2);
        let _weight = ExistentialSponsorship::on_initialize(2);

        assert!(!crate::Sponsorships::<Test>::contains_key(BENEFICIARY));
        assert_eq!(
            <mock::Balances as FungibleInspect<_>>::balance(&SPONSOR),
            500
        );
    });
}

#[test]
fn enforces_the_per_block_creation_count_cap() {
    new_test_ext().execute_with(|| {
        assert_ok!(ExistentialSponsorship::sponsor(
            RuntimeOrigin::signed(SPONSOR),
            BENEFICIARY,
            10,
            AutoUnlockMode::Disabled,
            Lease::None,
        ));
        assert_ok!(ExistentialSponsorship::sponsor(
            RuntimeOrigin::signed(SPONSOR),
            6,
            10,
            AutoUnlockMode::Disabled,
            Lease::None,
        ));

        assert_noop!(
            ExistentialSponsorship::sponsor(
                RuntimeOrigin::signed(SPONSOR),
                7,
                10,
                AutoUnlockMode::Disabled,
                Lease::None,
            ),
            Error::<Test>::SponsorPerBlockCountExceeded
        );
    });
}

#[test]
fn provider_trait_exposes_state() {
    new_test_ext().execute_with(|| {
        assert_ok!(ExistentialSponsorship::sponsor(
            RuntimeOrigin::signed(SPONSOR),
            BENEFICIARY,
            10,
            AutoUnlockMode::Disabled,
            Lease::None,
        ));

        let view =
            <ExistentialSponsorship as SponsorshipProvider<_, _, _>>::sponsorship_of(&BENEFICIARY)
                .expect("view exists");
        assert_eq!(view.sponsor, SPONSOR);
        assert_eq!(
            <ExistentialSponsorship as SponsorshipProvider<_, _, _>>::sponsored_amount(
                &BENEFICIARY,
            ),
            10
        );
    });
}
