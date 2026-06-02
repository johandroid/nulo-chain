#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod weights;

use core::{fmt, marker::PhantomData};
use frame::{
    deps::{
        frame_support::{
            dispatch::{DispatchInfo, PostDispatchInfo},
            traits::{
                Imbalance, OnUnbalanced,
                tokens::{
                    Fortitude, Precision, Preservation, WithdrawConsequence,
                    fungible::{Balanced, Credit, Inspect},
                },
            },
        },
        sp_runtime::traits::{PostDispatchInfoOf, Zero},
    },
    prelude::*,
};
use pallet_transaction_payment::{OnChargeTransaction, TxCreditHold};

pub use pallet::*;
pub use weights::WeightInfo;

type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
type RuntimeCallOf<T> = <T as frame_system::Config>::RuntimeCall;

pub trait PrepaidFee<AccountId, Balance> {
    fn account_id() -> AccountId;
    fn credit(who: &AccountId) -> Balance;
    fn withdraw_credit(who: &AccountId, amount: Balance) -> Result<(), TransactionValidityError>;
    fn refund_credit(who: &AccountId, amount: Balance) -> Result<(), TransactionValidityError>;
}

pub struct PrepaidFeeAdapter<F, P, OU>(PhantomData<(F, P, OU)>);

pub enum LiquidityInfo<AccountId, F>
where
    F: Balanced<AccountId>,
{
    Native {
        who: AccountId,
        fee_credit: Option<Credit<AccountId, F>>,
        tip_credit: Option<Credit<AccountId, F>>,
    },
    Prepaid {
        who: AccountId,
        paid_inclusion_fee: <F as Inspect<AccountId>>::Balance,
        fee_credit: Credit<AccountId, F>,
        tip_credit: Option<Credit<AccountId, F>>,
    },
    NoCharge,
}

impl<AccountId, F> Default for LiquidityInfo<AccountId, F>
where
    F: Balanced<AccountId>,
{
    fn default() -> Self {
        Self::NoCharge
    }
}

impl<AccountId, F> fmt::Debug for LiquidityInfo<AccountId, F>
where
    AccountId: fmt::Debug,
    F: Balanced<AccountId>,
    <F as Inspect<AccountId>>::Balance: fmt::Debug,
{
    #[cfg(feature = "std")]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Native { who, .. } => write!(f, "Native {{ who: {who:?} }}"),
            Self::Prepaid {
                who,
                paid_inclusion_fee,
                ..
            } => write!(
                f,
                "Prepaid {{ who: {who:?}, paid_inclusion_fee: {paid_inclusion_fee:?} }}"
            ),
            Self::NoCharge => f.write_str("NoCharge"),
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
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        PrepaidFeePaid {
            who: AccountIdOf<T>,
            actual_fee: u128,
            tip: u128,
            remaining: u128,
        },
    }
}

impl<T, F, P, OU> TxCreditHold<T> for PrepaidFeeAdapter<F, P, OU>
where
    T: pallet_transaction_payment::Config,
{
    type Credit = ();
}

impl<T, F, P, OU> OnChargeTransaction<T> for PrepaidFeeAdapter<F, P, OU>
where
    T: pallet_transaction_payment::Config + pallet::Config,
    RuntimeCallOf<T>:
        Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo> + GetDispatchInfo,
    F: Balanced<AccountIdOf<T>> + 'static,
    P: PrepaidFee<AccountIdOf<T>, <F as Inspect<AccountIdOf<T>>>::Balance>,
    OU: OnUnbalanced<Credit<AccountIdOf<T>, F>>,
{
    type Balance = <F as Inspect<AccountIdOf<T>>>::Balance;
    type LiquidityInfo = LiquidityInfo<AccountIdOf<T>, F>;

    fn withdraw_fee(
        who: &AccountIdOf<T>,
        _call: &RuntimeCallOf<T>,
        _dispatch_info: &DispatchInfoOf<RuntimeCallOf<T>>,
        fee_with_tip: Self::Balance,
        tip: Self::Balance,
    ) -> Result<Self::LiquidityInfo, TransactionValidityError> {
        if fee_with_tip.is_zero() {
            return Ok(LiquidityInfo::NoCharge);
        }

        let inclusion_fee = fee_with_tip.saturating_sub(tip);
        if !inclusion_fee.is_zero() && P::credit(who) >= inclusion_fee {
            Self::withdraw_tip::<T>(who, tip)?;
            P::withdraw_credit(who, inclusion_fee)?;
            let fee_credit = match F::withdraw(
                &P::account_id(),
                inclusion_fee,
                Precision::Exact,
                Preservation::Expendable,
                Fortitude::Polite,
            ) {
                Ok(credit) => credit,
                Err(_) => {
                    let _ = P::refund_credit(who, inclusion_fee);
                    return Err(InvalidTransaction::Payment.into());
                }
            };
            let tip_credit = Self::native_credit::<T>(who, tip)?;

            return Ok(LiquidityInfo::Prepaid {
                who: who.clone(),
                paid_inclusion_fee: inclusion_fee,
                fee_credit,
                tip_credit,
            });
        }

        let credit = F::withdraw(
            who,
            fee_with_tip,
            Precision::Exact,
            Preservation::Preserve,
            Fortitude::Polite,
        )
        .map_err(|_| InvalidTransaction::Payment)?;
        let (tip_credit, fee_credit) = credit.split(tip);

        Ok(LiquidityInfo::Native {
            who: who.clone(),
            fee_credit: Some(fee_credit),
            tip_credit: Some(tip_credit),
        })
    }

    fn can_withdraw_fee(
        who: &AccountIdOf<T>,
        _call: &RuntimeCallOf<T>,
        _dispatch_info: &DispatchInfoOf<RuntimeCallOf<T>>,
        fee_with_tip: Self::Balance,
        tip: Self::Balance,
    ) -> Result<(), TransactionValidityError> {
        if fee_with_tip.is_zero() {
            return Ok(());
        }

        let inclusion_fee = fee_with_tip.saturating_sub(tip);
        if !inclusion_fee.is_zero() && P::credit(who) >= inclusion_fee {
            return Self::can_withdraw_native::<T>(who, tip);
        }

        Self::can_withdraw_native::<T>(who, fee_with_tip)
    }

    fn correct_and_deposit_fee(
        _who: &AccountIdOf<T>,
        _dispatch_info: &DispatchInfoOf<RuntimeCallOf<T>>,
        _post_info: &PostDispatchInfoOf<RuntimeCallOf<T>>,
        corrected_fee_with_tip: Self::Balance,
        tip: Self::Balance,
        liquidity_info: Self::LiquidityInfo,
    ) -> Result<(), TransactionValidityError> {
        let corrected_fee = corrected_fee_with_tip.saturating_sub(tip);

        match liquidity_info {
            LiquidityInfo::NoCharge => Ok(()),
            LiquidityInfo::Native {
                who,
                fee_credit,
                tip_credit,
            } => {
                Self::settle_native_fee::<T>(&who, fee_credit, corrected_fee, tip_credit);
                Ok(())
            }
            LiquidityInfo::Prepaid {
                who,
                paid_inclusion_fee,
                fee_credit,
                tip_credit,
            } => {
                Self::settle_prepaid_fee::<T>(&who, fee_credit, paid_inclusion_fee, corrected_fee)?;
                OU::on_unbalanceds(tip_credit.into_iter());
                Pallet::<T>::deposit_event(Event::PrepaidFeePaid {
                    who: who.clone(),
                    actual_fee: corrected_fee.saturated_into::<u128>(),
                    tip: tip.saturated_into::<u128>(),
                    remaining: P::credit(&who).saturated_into::<u128>(),
                });
                Ok(())
            }
        }
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn endow_account(who: &AccountIdOf<T>, amount: Self::Balance) {
        let _ = F::deposit(who, amount, Precision::BestEffort);
    }

    #[cfg(feature = "runtime-benchmarks")]
    fn minimum_balance() -> Self::Balance {
        F::minimum_balance()
    }
}

impl<F, P, OU> PrepaidFeeAdapter<F, P, OU> {
    fn can_withdraw_native<T>(
        who: &AccountIdOf<T>,
        amount: <F as Inspect<AccountIdOf<T>>>::Balance,
    ) -> Result<(), TransactionValidityError>
    where
        T: pallet_transaction_payment::Config + pallet::Config,
        RuntimeCallOf<T>:
            Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo> + GetDispatchInfo,
        F: Balanced<AccountIdOf<T>> + 'static,
        P: PrepaidFee<AccountIdOf<T>, <F as Inspect<AccountIdOf<T>>>::Balance>,
        OU: OnUnbalanced<Credit<AccountIdOf<T>, F>>,
    {
        if amount.is_zero() {
            return Ok(());
        }

        match F::can_withdraw(who, amount) {
            WithdrawConsequence::Success => Ok(()),
            _ => Err(InvalidTransaction::Payment.into()),
        }
    }

    fn withdraw_tip<T>(
        who: &AccountIdOf<T>,
        tip: <F as Inspect<AccountIdOf<T>>>::Balance,
    ) -> Result<(), TransactionValidityError>
    where
        T: pallet_transaction_payment::Config + pallet::Config,
        RuntimeCallOf<T>:
            Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo> + GetDispatchInfo,
        F: Balanced<AccountIdOf<T>> + 'static,
        P: PrepaidFee<AccountIdOf<T>, <F as Inspect<AccountIdOf<T>>>::Balance>,
        OU: OnUnbalanced<Credit<AccountIdOf<T>, F>>,
    {
        Self::can_withdraw_native::<T>(who, tip)
    }

    fn native_credit<T>(
        who: &AccountIdOf<T>,
        amount: <F as Inspect<AccountIdOf<T>>>::Balance,
    ) -> Result<Option<Credit<AccountIdOf<T>, F>>, TransactionValidityError>
    where
        T: pallet_transaction_payment::Config + pallet::Config,
        RuntimeCallOf<T>:
            Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo> + GetDispatchInfo,
        F: Balanced<AccountIdOf<T>> + 'static,
        P: PrepaidFee<AccountIdOf<T>, <F as Inspect<AccountIdOf<T>>>::Balance>,
        OU: OnUnbalanced<Credit<AccountIdOf<T>, F>>,
    {
        if amount.is_zero() {
            return Ok(None);
        }

        F::withdraw(
            who,
            amount,
            Precision::Exact,
            Preservation::Preserve,
            Fortitude::Polite,
        )
        .map(Some)
        .map_err(|_| InvalidTransaction::Payment.into())
    }

    fn settle_native_fee<T>(
        who: &AccountIdOf<T>,
        fee_credit: Option<Credit<AccountIdOf<T>, F>>,
        corrected_fee: <F as Inspect<AccountIdOf<T>>>::Balance,
        tip_credit: Option<Credit<AccountIdOf<T>, F>>,
    ) where
        T: pallet_transaction_payment::Config + pallet::Config,
        RuntimeCallOf<T>:
            Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo> + GetDispatchInfo,
        F: Balanced<AccountIdOf<T>> + 'static,
        P: PrepaidFee<AccountIdOf<T>, <F as Inspect<AccountIdOf<T>>>::Balance>,
        OU: OnUnbalanced<Credit<AccountIdOf<T>, F>>,
    {
        if let Some(fee_credit) = fee_credit {
            let (mut paid_fee, refund_credit) = fee_credit.split(corrected_fee);
            Self::refund_native_fee::<T>(who, &mut paid_fee, refund_credit);
            OU::on_unbalanceds(Some(paid_fee).into_iter().chain(tip_credit));
        } else {
            OU::on_unbalanceds(tip_credit.into_iter());
        }
    }

    fn refund_native_fee<T>(
        who: &AccountIdOf<T>,
        paid_fee: &mut Credit<AccountIdOf<T>, F>,
        refund_credit: Credit<AccountIdOf<T>, F>,
    ) where
        T: pallet_transaction_payment::Config + pallet::Config,
        RuntimeCallOf<T>:
            Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo> + GetDispatchInfo,
        F: Balanced<AccountIdOf<T>> + 'static,
        P: PrepaidFee<AccountIdOf<T>, <F as Inspect<AccountIdOf<T>>>::Balance>,
        OU: OnUnbalanced<Credit<AccountIdOf<T>, F>>,
    {
        if refund_credit.peek().is_zero() {
            return;
        }

        if frame_system::Pallet::<T>::account_exists(who) {
            if let Err(not_refunded) = F::resolve(who, refund_credit) {
                paid_fee.subsume(not_refunded);
            }
        } else {
            paid_fee.subsume(refund_credit);
        }
    }

    fn settle_prepaid_fee<T>(
        who: &AccountIdOf<T>,
        fee_credit: Credit<AccountIdOf<T>, F>,
        paid_inclusion_fee: <F as Inspect<AccountIdOf<T>>>::Balance,
        corrected_fee: <F as Inspect<AccountIdOf<T>>>::Balance,
    ) -> Result<(), TransactionValidityError>
    where
        T: pallet_transaction_payment::Config + pallet::Config,
        RuntimeCallOf<T>:
            Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo> + GetDispatchInfo,
        F: Balanced<AccountIdOf<T>> + 'static,
        P: PrepaidFee<AccountIdOf<T>, <F as Inspect<AccountIdOf<T>>>::Balance>,
        OU: OnUnbalanced<Credit<AccountIdOf<T>, F>>,
    {
        let fee_to_keep = corrected_fee.min(paid_inclusion_fee);
        let (mut paid_fee, refund_credit) = fee_credit.split(fee_to_keep);
        let refund = refund_credit.peek();

        if !refund.is_zero() {
            match F::resolve(&P::account_id(), refund_credit) {
                Ok(()) => P::refund_credit(who, refund)?,
                Err(not_refunded) => paid_fee.subsume(not_refunded),
            }
        }

        OU::on_unbalanceds(Some(paid_fee).into_iter());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{PrepaidFee, PrepaidFeeAdapter};
    use frame::{
        deps::{frame_support::derive_impl, sp_runtime},
        storage_alias,
        testing_prelude::*,
    };
    use frame_system::mocking::MockUncheckedExtrinsic;
    use pallet_transaction_payment::ChargeTransactionPayment;
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
    type TxExtensions = ChargeTransactionPayment<Test>;
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
        type OnChargeTransaction = PrepaidFeeAdapter<Balances, DummyPrepaid, ()>;
        type WeightToFee = FixedFee<1, Balance>;
        type LengthToFee = FixedFee<0, Balance>;
    }

    impl crate::Config for Test {
        type RuntimeEvent = RuntimeEvent;
        type WeightInfo = ();
    }

    #[storage_alias]
    type Tank = StorageMap<Prefix, Blake2_128, AccountId, Balance>;

    pub struct DummyPrepaid;

    impl PrepaidFee<AccountId, Balance> for DummyPrepaid {
        fn account_id() -> AccountId {
            GAS_ACCOUNT
        }

        fn credit(who: &AccountId) -> Balance {
            Tank::get(who).unwrap_or_default()
        }

        fn withdraw_credit(
            who: &AccountId,
            amount: Balance,
        ) -> Result<(), TransactionValidityError> {
            Tank::try_mutate(who, |stored| {
                let current = stored.unwrap_or_default();
                if current < amount {
                    return Err(InvalidTransaction::Payment.into());
                }
                *stored = Some(current - amount);
                Ok(())
            })
        }

        fn refund_credit(who: &AccountId, amount: Balance) -> Result<(), TransactionValidityError> {
            Tank::mutate(who, |stored| {
                *stored = Some(stored.unwrap_or_default().saturating_add(amount));
            });
            Ok(())
        }
    }

    const ALICE: AccountId = 1;
    const BOB: AccountId = 2;
    const GAS_ACCOUNT: AccountId = 99;

    fn charge_tx(tip: Balance) -> TxExtensions {
        TxExtensions::from(tip)
    }

    fn remark_call() -> RuntimeCall {
        RuntimeCall::System(frame_system::Call::remark {
            remark: b"hello".to_vec(),
        })
    }

    fn test_run(
        who: AccountId,
        tip: Balance,
        call_weight: Weight,
        actual_weight: Option<Weight>,
    ) -> <TxExtensions as sp_runtime::traits::DispatchTransaction<RuntimeCall>>::Result {
        let call = remark_call();
        let info = frame::deps::frame_support::dispatch::DispatchInfo {
            call_weight,
            ..Default::default()
        };

        charge_tx(tip).test_run(
            RuntimeOrigin::signed(who),
            &call,
            &info,
            call.encoded_size(),
            0,
            |_| {
                Ok(PostDispatchInfo {
                    actual_weight,
                    pays_fee: Pays::Yes,
                })
            },
        )
    }

    fn new_test_ext(
        prepaid: Vec<(AccountId, Balance)>,
        balances: Vec<(AccountId, Balance)>,
    ) -> TestExternalities {
        let mut ext = TestExternalities::new(Default::default());
        ext.execute_with(|| {
            let total_prepaid = prepaid
                .iter()
                .fold(0u64, |total, (_, amount)| total.saturating_add(*amount));
            if !total_prepaid.is_zero() {
                assert_ok!(Balances::force_set_balance(
                    RuntimeOrigin::root(),
                    GAS_ACCOUNT,
                    total_prepaid,
                ));
            }
            for (who, amount) in balances {
                assert_ok!(Balances::force_set_balance(
                    RuntimeOrigin::root(),
                    who,
                    amount,
                ));
            }
            for (who, amount) in prepaid {
                Tank::insert(who, amount);
            }
            System::set_block_number(1);
        });
        ext
    }

    #[test]
    fn prepaid_credit_pays_inclusion_fee_without_native_balance() {
        new_test_ext(vec![(ALICE, 5)], Vec::new()).execute_with(|| {
            assert_ok!(test_run(ALICE, 0, Weight::from_parts(2, 0), None));
            assert_eq!(Tank::get(ALICE), Some(3));
            assert_eq!(Balances::free_balance(GAS_ACCOUNT), 3);
            assert_eq!(Balances::free_balance(ALICE), 0);
        });
    }

    #[test]
    fn prepaid_credit_refunds_overestimated_fee() {
        new_test_ext(vec![(ALICE, 5)], Vec::new()).execute_with(|| {
            assert_ok!(test_run(
                ALICE,
                0,
                Weight::from_parts(4, 0),
                Some(Weight::from_parts(2, 0)),
            ));
            assert_eq!(Tank::get(ALICE), Some(3));
            assert_eq!(Balances::free_balance(GAS_ACCOUNT), 3);
        });
    }

    #[test]
    fn falls_back_to_native_transaction_payment() {
        new_test_ext(Vec::new(), vec![(BOB, 3)]).execute_with(|| {
            assert_ok!(test_run(BOB, 0, Weight::from_parts(2, 0), None));
            assert_eq!(Balances::free_balance(BOB), 1);
        });
    }

    #[test]
    fn prepaid_pays_fee_and_native_balance_pays_tip() {
        new_test_ext(vec![(ALICE, 5)], vec![(ALICE, 2)]).execute_with(|| {
            assert_ok!(test_run(ALICE, 1, Weight::from_parts(2, 0), None));
            assert_eq!(Tank::get(ALICE), Some(3));
            assert_eq!(Balances::free_balance(GAS_ACCOUNT), 3);
            assert_eq!(Balances::free_balance(ALICE), 1);
        });
    }

    #[test]
    fn fails_when_neither_prepaid_credit_nor_balance_can_pay() {
        new_test_ext(vec![(ALICE, 1)], Vec::new()).execute_with(|| {
            assert_noop!(
                test_run(ALICE, 0, Weight::from_parts(2, 0), None),
                InvalidTransaction::Payment
            );
        });
    }
}
