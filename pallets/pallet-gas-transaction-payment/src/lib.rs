#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod weights;

use codec::{Decode, DecodeWithMemTracking, Encode};
use core::{fmt, marker::PhantomData};
use frame::{
    deps::{
        frame_support::dispatch::DispatchInfo,
        sp_runtime::traits::{
            DispatchOriginOf, Implication, PostDispatchInfoOf, TransactionExtension,
        },
    },
    prelude::*,
};
use scale_info::{StaticTypeInfo, TypeInfo};

pub use pallet::*;
pub use weights::WeightInfo;

pub trait GasBurner {
    type AccountId;

    fn check_available_gas(who: &Self::AccountId, estimated: &Weight) -> Option<Weight>;

    fn burn_gas(who: &Self::AccountId, expected: &Weight, used: &Weight) -> Weight;
}

#[derive(Decode, DecodeWithMemTracking, Encode, Clone, Eq, PartialEq)]
pub struct ChargeTransactionPayment<T, S>(pub S, PhantomData<T>);

impl<T, S> ChargeTransactionPayment<T, S> {
    pub fn new(inner: S) -> Self {
        Self(inner, PhantomData)
    }
}

impl<T, S> From<S> for ChargeTransactionPayment<T, S> {
    fn from(inner: S) -> Self {
        Self::new(inner)
    }
}

impl<T: Config, S: TransactionExtension<T::RuntimeCall> + StaticTypeInfo> TypeInfo
    for ChargeTransactionPayment<T, S>
{
    type Identity = S;

    fn type_info() -> scale_info::Type {
        S::type_info()
    }
}

impl<T, S: Encode> fmt::Debug for ChargeTransactionPayment<T, S> {
    #[cfg(feature = "std")]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ChargeTransactionPayment<{:?}>", self.0.encode())
    }

    #[cfg(not(feature = "std"))]
    fn fmt(&self, _: &mut fmt::Formatter) -> fmt::Result {
        Ok(())
    }
}

#[derive(PartialEq)]
pub enum Pre<AccountId, P> {
    Burner(AccountId, Weight),
    Inner(P),
}

impl<AccountId, P> fmt::Debug for Pre<AccountId, P>
where
    AccountId: fmt::Debug,
    P: fmt::Debug,
{
    #[cfg(feature = "std")]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Pre::Burner(who, gas) => write!(f, "Pre::Burner({who:?}, {gas:?})"),
            Pre::Inner(inner) => write!(f, "Pre::Inner({inner:?})"),
        }
    }

    #[cfg(not(feature = "std"))]
    fn fmt(&self, _: &mut fmt::Formatter) -> fmt::Result {
        Ok(())
    }
}

#[frame::pallet]
pub mod pallet {
    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type WeightInfo: crate::weights::WeightInfo;

        type GasTank: GasBurner<AccountId = Self::AccountId>;
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        GasBurned {
            who: T::AccountId,
            remaining: Weight,
        },
    }
}

impl<T, S> TransactionExtension<T::RuntimeCall> for ChargeTransactionPayment<T, S>
where
    T: Config + Send + Sync,
    T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
    S: TransactionExtension<T::RuntimeCall> + StaticTypeInfo,
{
    const IDENTIFIER: &'static str = S::IDENTIFIER;
    type Implicit = S::Implicit;
    type Val = Option<S::Val>;
    type Pre = Pre<T::AccountId, S::Pre>;

    fn weight(&self, _: &T::RuntimeCall) -> Weight {
        T::WeightInfo::charge_transaction_payment()
    }

    fn validate(
        &self,
        origin: DispatchOriginOf<T::RuntimeCall>,
        call: &T::RuntimeCall,
        info: &DispatchInfoOf<T::RuntimeCall>,
        len: usize,
        self_implicit: Self::Implicit,
        inherited_implication: &impl Implication,
        source: TransactionSource,
    ) -> ValidateResult<Self::Val, T::RuntimeCall> {
        match origin
            .clone()
            .into()
            .map_err(|_| InvalidTransaction::BadSigner)?
        {
            frame_system::RawOrigin::Signed(ref who) => {
                if T::GasTank::check_available_gas(who, &info.call_weight).is_some() {
                    Ok((ValidTransaction::default(), None, origin))
                } else {
                    self.0
                        .validate(
                            origin,
                            call,
                            info,
                            len,
                            self_implicit,
                            inherited_implication,
                            source,
                        )
                        .map(|(valid, val, origin)| (valid, Some(val), origin))
                }
            }
            _ => self
                .0
                .validate(
                    origin,
                    call,
                    info,
                    len,
                    self_implicit,
                    inherited_implication,
                    source,
                )
                .map(|(valid, val, origin)| (valid, Some(val), origin)),
        }
    }

    fn prepare(
        self,
        val: Self::Val,
        origin: &DispatchOriginOf<T::RuntimeCall>,
        call: &T::RuntimeCall,
        info: &DispatchInfoOf<T::RuntimeCall>,
        len: usize,
    ) -> Result<Self::Pre, TransactionValidityError> {
        match origin
            .clone()
            .into()
            .map_err(|_| InvalidTransaction::BadSigner)?
        {
            frame_system::RawOrigin::Signed(who) => {
                let pre = if let Some(leftover) =
                    T::GasTank::check_available_gas(&who, &info.call_weight)
                {
                    Pre::Burner(who, leftover)
                } else {
                    self.0
                        .prepare(
                            val.expect("value was captured during validation; qed"),
                            origin,
                            call,
                            info,
                            len,
                        )
                        .map(Pre::Inner)?
                };

                Ok(pre)
            }
            _ => self
                .0
                .prepare(
                    val.expect("value was captured during validation; qed"),
                    origin,
                    call,
                    info,
                    len,
                )
                .map(Pre::Inner),
        }
    }

    fn post_dispatch_details(
        pre: Self::Pre,
        info: &DispatchInfoOf<T::RuntimeCall>,
        post_info: &PostDispatchInfoOf<T::RuntimeCall>,
        len: usize,
        result: &DispatchResult,
    ) -> Result<Weight, TransactionValidityError> {
        match pre {
            Pre::Inner(pre) => S::post_dispatch_details(pre, info, post_info, len, result),
            Pre::Burner(who, expected_remaining) => {
                if post_info.pays_fee == Pays::No {
                    return Ok(Weight::zero());
                }

                let used = post_info.actual_weight.unwrap_or(info.call_weight);
                let remaining = T::GasTank::burn_gas(&who, &expected_remaining, &used);
                Pallet::<T>::deposit_event(Event::GasBurned { who, remaining });
                Ok(used)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ChargeTransactionPayment, Config, GasBurner};
    use frame::{
        deps::{frame_support::derive_impl, sp_runtime},
        storage_alias,
        testing_prelude::*,
    };
    use frame_system::mocking::MockUncheckedExtrinsic;
    use pallet_transaction_payment::ChargeTransactionPayment as NativeChargeTransactionPayment;
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
        pub type GasTransactionPayment = crate;
        #[runtime::pallet_index(12)]
        pub type TransactionPayment = pallet_transaction_payment;
    }

    type AccountId = <Test as frame_system::Config>::AccountId;
    type Balance = <Test as pallet_balances::Config>::Balance;
    type TxExtensions = ChargeTransactionPayment<Test, NativeChargeTransactionPayment<Test>>;
    type UncheckedExtrinsic = MockUncheckedExtrinsic<Test, (), TxExtensions>;
    type Block = sp_runtime::generic::Block<
        sp_runtime::generic::Header<u64, BlakeTwo256>,
        UncheckedExtrinsic,
    >;

    #[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
    impl frame_system::Config for Test {
        type Block = Block;
        type AccountData = pallet_balances::AccountData<Balance>;
    }

    #[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
    impl pallet_balances::Config for Test {
        type AccountStore = System;
    }

    #[derive_impl(pallet_transaction_payment::config_preludes::TestDefaultConfig)]
    impl pallet_transaction_payment::Config for Test {
        type OnChargeTransaction = pallet_transaction_payment::FungibleAdapter<Balances, ()>;
        type WeightToFee = FixedFee<1, Balance>;
        type LengthToFee = FixedFee<0, Balance>;
    }

    #[storage_alias]
    type Tank = StorageMap<Prefix, Blake2_128, AccountId, Weight>;

    pub struct DummyGasBurner;

    impl GasBurner for DummyGasBurner {
        type AccountId = AccountId;

        fn check_available_gas(who: &Self::AccountId, estimated: &Weight) -> Option<Weight> {
            Tank::get(who).and_then(|remaining| remaining.checked_sub(estimated))
        }

        fn burn_gas(who: &Self::AccountId, expected: &Weight, used: &Weight) -> Weight {
            Tank::mutate(who, |remaining| {
                let next = remaining
                    .and_then(|current| current.checked_sub(used))
                    .unwrap_or_default()
                    .max(*expected);
                *remaining = Some(next);
                next
            })
        }
    }

    impl Config for Test {
        type RuntimeEvent = RuntimeEvent;
        type WeightInfo = ();
        type GasTank = DummyGasBurner;
    }

    const ALICE: AccountId = 1;
    const BOB: AccountId = 2;

    fn charge_tx() -> TxExtensions {
        TxExtensions::new(NativeChargeTransactionPayment::<Test>::from(0))
    }

    fn remark_call() -> RuntimeCall {
        RuntimeCall::System(frame_system::Call::remark {
            remark: b"hello".to_vec(),
        })
    }

    fn test_run(
        who: AccountId,
        call: &RuntimeCall,
        call_weight: Weight,
    ) -> <TxExtensions as sp_runtime::traits::DispatchTransaction<RuntimeCall>>::Result {
        let info = frame::deps::frame_support::dispatch::DispatchInfo {
            call_weight,
            ..Default::default()
        };

        charge_tx().test_run(
            RuntimeOrigin::signed(who),
            call,
            &info,
            call.encoded_size(),
            0,
            |_| Ok(().into()),
        )
    }

    fn new_test_ext(tank: Vec<(AccountId, Weight)>) -> TestExternalities {
        let mut ext = TestExternalities::new(Default::default());
        ext.execute_with(|| {
            for (who, gas) in tank {
                Tank::insert(who, gas);
            }
            System::set_block_number(1);
        });
        ext
    }

    #[test]
    fn gas_credit_skips_native_fee_withdrawal() {
        new_test_ext(vec![(ALICE, Weight::from_parts(3, 0))]).execute_with(|| {
            let call = remark_call();
            assert_ok!(test_run(ALICE, &call, Weight::from_parts(2, 0)));
            assert_eq!(Tank::get(ALICE), Some(Weight::from_parts(1, 0)));
        });
    }

    #[test]
    fn falls_back_to_native_transaction_payment() {
        new_test_ext(Vec::new()).execute_with(|| {
            let call = remark_call();
            assert_ok!(Balances::force_set_balance(RuntimeOrigin::root(), BOB, 3));
            assert_ok!(test_run(BOB, &call, Weight::from_parts(2, 0)));
            assert_eq!(Balances::free_balance(BOB), 1);
        });
    }

    #[test]
    fn fails_when_neither_gas_nor_balance_can_pay() {
        new_test_ext(vec![(ALICE, Weight::from_parts(1, 0))]).execute_with(|| {
            let call = remark_call();

            assert_noop!(
                test_run(ALICE, &call, Weight::from_parts(2, 0)),
                InvalidTransaction::Payment
            );
        });
    }
}
