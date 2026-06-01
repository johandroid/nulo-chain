#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod weights;

use frame::prelude::*;
use pallet_gas_transaction_payment::GasBurner;

pub use pallet::*;

type BalanceOf<T> =
    <<T as pallet::Config>::Currency as frame::deps::frame_support::traits::fungible::Inspect<
        <T as frame_system::Config>::AccountId,
    >>::Balance;

#[frame::pallet]
pub mod pallet {
    use super::{BalanceOf, GasBurner};
    use crate::weights::WeightInfo as _;
    use frame::{
        deps::sp_runtime::traits::{AccountIdConversion, Zero},
        prelude::*,
    };
    use sp_weights::WeightToFee;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Currency: frame::deps::frame_support::traits::fungible::Inspect<Self::AccountId>
            + frame::deps::frame_support::traits::fungible::Mutate<Self::AccountId>;

        type WeightToFee: WeightToFee<Balance = BalanceOf<Self>>;

        #[pallet::constant]
        type PalletId: Get<frame::deps::frame_support::PalletId>;

        #[pallet::constant]
        type MinPurchase: Get<Weight>;

        type WeightInfo: crate::weights::WeightInfo;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::storage]
    pub type GasCredits<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, Weight, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        GasPurchased {
            sponsor: T::AccountId,
            beneficiary: T::AccountId,
            purchased: Weight,
            cost: BalanceOf<T>,
            remaining: Weight,
        },
        GasSpent {
            who: T::AccountId,
            spent: Weight,
            remaining: Weight,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        GasAmountTooLow,
        CannotPurchaseGas,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::purchase())]
        pub fn purchase(
            origin: OriginFor<T>,
            beneficiary: T::AccountId,
            gas: Weight,
        ) -> DispatchResultWithPostInfo {
            let sponsor = ensure_signed(origin)?;
            Self::purchase_for(&sponsor, &beneficiary, gas)?;
            Ok(().into())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn account_id() -> T::AccountId {
            T::PalletId::get().into_account_truncating()
        }

        pub fn remaining_gas(who: &T::AccountId) -> Weight {
            GasCredits::<T>::get(who)
        }

        pub fn purchase_for(
            sponsor: &T::AccountId,
            beneficiary: &T::AccountId,
            gas: Weight,
        ) -> DispatchResult {
            ensure!(
                gas.all_gte(T::MinPurchase::get()),
                Error::<T>::GasAmountTooLow
            );

            let cost = T::WeightToFee::weight_to_fee(&gas);
            ensure!(!cost.is_zero(), Error::<T>::GasAmountTooLow);

            <T::Currency as frame::deps::frame_support::traits::fungible::Mutate<_>>::transfer(
                sponsor,
                &Self::account_id(),
                cost,
                Preservation::Preserve,
            )
            .map_err(|_| Error::<T>::CannotPurchaseGas)?;

            let remaining = GasCredits::<T>::mutate(beneficiary, |stored| {
                *stored = stored.saturating_add(gas);
                *stored
            });

            Self::deposit_event(Event::GasPurchased {
                sponsor: sponsor.clone(),
                beneficiary: beneficiary.clone(),
                purchased: gas,
                cost,
                remaining,
            });

            Ok(())
        }
    }

    impl<T: Config> GasBurner for Pallet<T> {
        type AccountId = T::AccountId;

        fn check_available_gas(who: &Self::AccountId, estimated: &Weight) -> Option<Weight> {
            GasCredits::<T>::get(who).checked_sub(estimated)
        }

        fn burn_gas(who: &Self::AccountId, expected: &Weight, used: &Weight) -> Weight {
            let remaining = GasCredits::<T>::mutate(who, |stored| {
                let next = stored.checked_sub(used).unwrap_or_default().max(*expected);
                *stored = next;
                next
            });

            Pallet::<T>::deposit_event(Event::GasSpent {
                who: who.clone(),
                spent: *used,
                remaining,
            });

            remaining
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
        pub const MinPurchase: Weight = Weight::from_parts(1, 0);
    }

    impl Config for Test {
        type RuntimeEvent = RuntimeEvent;
        type Currency = Balances;
        type WeightToFee = FixedFee<2, Balance>;
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
    fn purchase_converts_native_balance_into_weight_credit() {
        new_test_ext().execute_with(|| {
            assert_ok!(PrepaidGas::purchase(
                RuntimeOrigin::signed(SPONSOR),
                BENEFICIARY,
                Weight::from_parts(5, 0),
            ));

            assert_eq!(
                PrepaidGas::remaining_gas(&BENEFICIARY),
                Weight::from_parts(5, 0)
            );
            assert_eq!(Balances::free_balance(SPONSOR), 98);
            assert_eq!(Balances::free_balance(PrepaidGas::account_id()), 2);
        });
    }

    #[test]
    fn burner_consumes_only_used_weight() {
        new_test_ext().execute_with(|| {
            assert_ok!(PrepaidGas::purchase(
                RuntimeOrigin::signed(SPONSOR),
                BENEFICIARY,
                Weight::from_parts(5, 0),
            ));

            let leftover = <PrepaidGas as GasBurner>::check_available_gas(
                &BENEFICIARY,
                &Weight::from_parts(4, 0),
            )
            .expect("gas exists");
            let remaining = <PrepaidGas as GasBurner>::burn_gas(
                &BENEFICIARY,
                &leftover,
                &Weight::from_parts(3, 0),
            );

            assert_eq!(remaining, Weight::from_parts(2, 0));
            assert_eq!(
                PrepaidGas::remaining_gas(&BENEFICIARY),
                Weight::from_parts(2, 0)
            );
        });
    }

    #[test]
    fn rejects_purchase_below_minimum() {
        new_test_ext().execute_with(|| {
            assert_noop!(
                PrepaidGas::purchase(RuntimeOrigin::signed(SPONSOR), BENEFICIARY, Weight::zero(),),
                Error::<Test>::GasAmountTooLow
            );
        });
    }
}
