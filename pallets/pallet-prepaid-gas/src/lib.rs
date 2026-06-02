#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod weights;

use frame::prelude::*;
use pallet_gas_transaction_payment::PrepaidFee;

pub use pallet::*;

type BalanceOf<T> =
    <<T as pallet::Config>::Currency as frame::deps::frame_support::traits::fungible::Inspect<
        <T as frame_system::Config>::AccountId,
    >>::Balance;

#[frame::pallet]
pub mod pallet {
    use super::{BalanceOf, PrepaidFee};
    use crate::weights::WeightInfo as _;
    use frame::{
        deps::{
            frame_support::transactional,
            sp_runtime::traits::{AccountIdConversion, Zero},
        },
        prelude::*,
    };

    #[pallet::config]
    pub trait Config: frame_system::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Currency: frame::deps::frame_support::traits::fungible::Inspect<Self::AccountId>
            + frame::deps::frame_support::traits::fungible::Mutate<Self::AccountId>;

        #[pallet::constant]
        type PalletId: Get<frame::deps::frame_support::PalletId>;

        #[pallet::constant]
        type MinPurchase: Get<BalanceOf<Self>>;

        type WeightInfo: crate::weights::WeightInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type PrepaidCredits<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BalanceOf<T>, ValueQuery>;

    #[pallet::storage]
    pub type PalletAccountInitialized<T: Config> = StorageValue<_, bool, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        PrepaidPurchased {
            sponsor: T::AccountId,
            beneficiary: T::AccountId,
            amount: BalanceOf<T>,
            remaining: BalanceOf<T>,
        },
        PrepaidFeeWithdrawn {
            who: T::AccountId,
            amount: BalanceOf<T>,
            remaining: BalanceOf<T>,
        },
        PrepaidFeeRefunded {
            who: T::AccountId,
            amount: BalanceOf<T>,
            remaining: BalanceOf<T>,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        PurchaseAmountTooLow,
        CannotPurchaseCredit,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::purchase())]
        #[transactional]
        pub fn purchase(
            origin: OriginFor<T>,
            beneficiary: T::AccountId,
            amount: BalanceOf<T>,
        ) -> DispatchResultWithPostInfo {
            let sponsor = ensure_signed(origin)?;
            Self::purchase_for(&sponsor, &beneficiary, amount)?;
            Ok(().into())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn account_id() -> T::AccountId {
            T::PalletId::get().into_account_truncating()
        }

        pub fn remaining_credit(who: &T::AccountId) -> BalanceOf<T> {
            PrepaidCredits::<T>::get(who)
        }

        #[transactional]
        pub fn purchase_for(
            sponsor: &T::AccountId,
            beneficiary: &T::AccountId,
            amount: BalanceOf<T>,
        ) -> DispatchResult {
            ensure!(
                !amount.is_zero() && amount >= T::MinPurchase::get(),
                Error::<T>::PurchaseAmountTooLow
            );

            Self::ensure_pallet_account();

            <T::Currency as frame::deps::frame_support::traits::fungible::Mutate<_>>::transfer(
                sponsor,
                &Self::account_id(),
                amount,
                Preservation::Preserve,
            )
            .map_err(|_| Error::<T>::CannotPurchaseCredit)?;

            let remaining = PrepaidCredits::<T>::mutate(beneficiary, |stored| {
                *stored = stored.saturating_add(amount);
                *stored
            });

            Self::deposit_event(Event::PrepaidPurchased {
                sponsor: sponsor.clone(),
                beneficiary: beneficiary.clone(),
                amount,
                remaining,
            });

            Ok(())
        }

        fn ensure_pallet_account() {
            if !PalletAccountInitialized::<T>::get() {
                let _ = frame_system::Pallet::<T>::inc_providers(&Self::account_id());
                PalletAccountInitialized::<T>::put(true);
            }
        }
    }

    impl<T: Config> PrepaidFee<T::AccountId, BalanceOf<T>> for Pallet<T> {
        fn account_id() -> T::AccountId {
            Pallet::<T>::account_id()
        }

        fn credit(who: &T::AccountId) -> BalanceOf<T> {
            Pallet::<T>::remaining_credit(who)
        }

        fn withdraw_credit(
            who: &T::AccountId,
            amount: BalanceOf<T>,
        ) -> Result<(), TransactionValidityError> {
            if amount.is_zero() {
                return Ok(());
            }

            let remaining = PrepaidCredits::<T>::try_mutate(
                who,
                |stored| -> Result<BalanceOf<T>, TransactionValidityError> {
                    if *stored < amount {
                        return Err(InvalidTransaction::Payment.into());
                    }
                    *stored = stored.saturating_sub(amount);
                    Ok(*stored)
                },
            )?;

            Pallet::<T>::deposit_event(Event::PrepaidFeeWithdrawn {
                who: who.clone(),
                amount,
                remaining,
            });

            Ok(())
        }

        fn refund_credit(
            who: &T::AccountId,
            amount: BalanceOf<T>,
        ) -> Result<(), TransactionValidityError> {
            if amount.is_zero() {
                return Ok(());
            }

            let remaining = PrepaidCredits::<T>::mutate(who, |stored| {
                *stored = stored.saturating_add(amount);
                *stored
            });

            Pallet::<T>::deposit_event(Event::PrepaidFeeRefunded {
                who: who.clone(),
                amount,
                remaining,
            });

            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::pallet::Error;
    use super::*;
    use frame::{
        deps::frame_support::{PalletId, derive_impl},
        testing_prelude::*,
    };
    use pallet_gas_transaction_payment::PrepaidFee;
    use polkadot_sdk::*;

    #[frame_construct_runtime]
    mod test_runtime {
        #[runtime::runtime]
        #[runtime::derive(
            RuntimeCall,
            RuntimeEvent,
            RuntimeError,
            RuntimeOrigin,
            RuntimeTask,
            RuntimeHoldReason,
            RuntimeFreezeReason
        )]
        pub struct Test;

        #[runtime::pallet_index(0)]
        pub type System = frame_system;
        #[runtime::pallet_index(10)]
        pub type Balances = pallet_balances;
        #[runtime::pallet_index(11)]
        pub type PrepaidGas = crate;
    }

    type AccountId = <Test as frame_system::Config>::AccountId;
    type Balance = <Test as pallet_balances::Config>::Balance;

    #[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
    impl frame_system::Config for Test {
        type Block = MockBlock<Test>;
        type AccountData = pallet_balances::AccountData<Balance>;
    }

    #[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
    impl pallet_balances::Config for Test {
        type AccountStore = System;
    }

    parameter_types! {
        pub const GasPalletId: PalletId = PalletId(*b"py/gas!!");
        pub const MinPurchase: Balance = 1;
    }

    impl Config for Test {
        type RuntimeEvent = RuntimeEvent;
        type Currency = Balances;
        type PalletId = GasPalletId;
        type MinPurchase = MinPurchase;
        type WeightInfo = ();
    }

    const SPONSOR: AccountId = 1;
    const BENEFICIARY: AccountId = 2;

    fn new_test_ext() -> TestState {
        let mut storage = frame_system::GenesisConfig::<Test>::default()
            .build_storage()
            .unwrap();

        pallet_balances::GenesisConfig::<Test> {
            balances: vec![(SPONSOR, 100)],
            ..Default::default()
        }
        .assimilate_storage(&mut storage)
        .unwrap();

        let mut ext: TestState = storage.into();
        ext.execute_with(|| System::set_block_number(1));
        ext
    }

    #[test]
    fn purchase_locks_native_balance_as_prepaid_credit() {
        new_test_ext().execute_with(|| {
            assert_ok!(PrepaidGas::purchase(
                RuntimeOrigin::signed(SPONSOR),
                BENEFICIARY,
                5,
            ));

            assert_eq!(PrepaidGas::remaining_credit(&BENEFICIARY), 5);
            assert_eq!(Balances::free_balance(SPONSOR), 95);
            assert_eq!(Balances::free_balance(PrepaidGas::account_id()), 5);
        });
    }

    #[test]
    fn prepaid_fee_withdrawal_and_refund_update_credit_only() {
        new_test_ext().execute_with(|| {
            assert_ok!(PrepaidGas::purchase(
                RuntimeOrigin::signed(SPONSOR),
                BENEFICIARY,
                5,
            ));

            assert_ok!(<PrepaidGas as PrepaidFee<_, _>>::withdraw_credit(
                &BENEFICIARY,
                4,
            ));
            assert_eq!(PrepaidGas::remaining_credit(&BENEFICIARY), 1);

            assert_ok!(<PrepaidGas as PrepaidFee<_, _>>::refund_credit(
                &BENEFICIARY,
                2,
            ));
            assert_eq!(PrepaidGas::remaining_credit(&BENEFICIARY), 3);
            assert_eq!(Balances::free_balance(BENEFICIARY), 0);
        });
    }

    #[test]
    fn rejects_zero_purchase() {
        new_test_ext().execute_with(|| {
            assert_noop!(
                PrepaidGas::purchase(RuntimeOrigin::signed(SPONSOR), BENEFICIARY, 0,),
                Error::<Test>::PurchaseAmountTooLow
            );
        });
    }
}
