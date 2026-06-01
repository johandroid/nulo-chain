#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;

use frame::deps::{
    codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen},
    frame_support::traits::tokens::fungible::Inspect as FungibleInspect,
    frame_system,
    scale_info::TypeInfo,
    sp_runtime::traits::Zero,
};

#[derive(
    Clone,
    Debug,
    Encode,
    Decode,
    DecodeWithMemTracking,
    Eq,
    PartialEq,
    TypeInfo,
    MaxEncodedLen,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum AutoUnlockCondition {
    SpendableGeSponsored,
    FreeGeSponsored,
}

#[derive(
    Clone,
    Debug,
    Encode,
    Decode,
    DecodeWithMemTracking,
    Eq,
    PartialEq,
    TypeInfo,
    MaxEncodedLen,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum AutoUnlockMode {
    Disabled,
    Enabled(AutoUnlockCondition),
}

#[derive(
    Clone,
    Debug,
    Encode,
    Decode,
    DecodeWithMemTracking,
    Eq,
    PartialEq,
    TypeInfo,
    MaxEncodedLen,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum Lease<BlockNumber> {
    None,
    Until(BlockNumber),
}

#[derive(
    Clone,
    Debug,
    Encode,
    Decode,
    DecodeWithMemTracking,
    Eq,
    PartialEq,
    TypeInfo,
    MaxEncodedLen,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum UnlockReason {
    Manual,
    Force,
}

#[derive(
    Clone,
    Debug,
    Default,
    Encode,
    Decode,
    DecodeWithMemTracking,
    Eq,
    PartialEq,
    TypeInfo,
    MaxEncodedLen,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct SponsorState<Balance, BlockNumber> {
    pub active_amount: Balance,
    pub active_count: u32,
    pub last_block: BlockNumber,
    pub sponsored_amount_in_block: Balance,
    pub new_sponsorships_in_block: u32,
}

#[derive(
    Clone,
    Debug,
    Encode,
    Decode,
    DecodeWithMemTracking,
    Eq,
    PartialEq,
    TypeInfo,
    MaxEncodedLen,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct SponsorshipView<AccountId, Balance, BlockNumber> {
    pub sponsor: AccountId,
    pub beneficiary: AccountId,
    pub amount: Balance,
    pub auto_unlock: AutoUnlockMode,
    pub lease: Lease<BlockNumber>,
    pub created_at: BlockNumber,
}

pub trait SponsorshipProvider<AccountId, Balance, BlockNumber> {
    fn sponsorship_of(
        beneficiary: &AccountId,
    ) -> Option<SponsorshipView<AccountId, Balance, BlockNumber>>;

    fn is_sponsored(beneficiary: &AccountId) -> bool;

    fn sponsored_amount(beneficiary: &AccountId) -> Balance;

    fn sponsor_of(beneficiary: &AccountId) -> Option<AccountId>;

    fn create_sponsorship(
        sponsor: &AccountId,
        beneficiary: &AccountId,
        amount: Balance,
        auto_unlock: AutoUnlockMode,
        lease: Lease<BlockNumber>,
    ) -> frame::deps::sp_runtime::DispatchResult;

    fn unlock_sponsorship(
        sponsor: &AccountId,
        beneficiary: &AccountId,
    ) -> frame::deps::sp_runtime::DispatchResult;

    fn try_auto_unlock(beneficiary: &AccountId) -> frame::deps::sp_runtime::DispatchResult;
}

pub type BalanceOf<T> = <<T as pallet::Config>::Currency as FungibleInspect<
    <T as frame_system::Config>::AccountId,
>>::Balance;

#[frame::pallet]
pub mod pallet {
    use alloc::vec::Vec;

    use super::{
        AutoUnlockCondition, AutoUnlockMode, BalanceOf, Lease, SponsorState, UnlockReason,
    };
    use crate::weights::WeightInfo as _;
    use frame::{
        deps::{
            frame_support::{
                BoundedVec, ensure,
                traits::{
                    EnsureOrigin, Get,
                    tokens::{
                        Fortitude, Precision, Preservation, Restriction,
                        fungible::{
                            Inspect as FungibleInspect, InspectHold, Mutate as FungibleMutate,
                            MutateHold,
                        },
                    },
                },
                transactional,
            },
            sp_runtime::traits::{Saturating, Zero},
        },
        prelude::*,
    };

    const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        #[allow(deprecated)]
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        type Currency: FungibleInspect<Self::AccountId>
            + FungibleMutate<Self::AccountId>
            + InspectHold<Self::AccountId, Reason = Self::RuntimeHoldReason>
            + MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;

        type RuntimeHoldReason: From<HoldReason>;

        type WeightInfo: crate::weights::WeightInfo;

        #[pallet::constant]
        type MinSponsorship: Get<BalanceOf<Self>>;

        #[pallet::constant]
        type SponsorMinRemaining: Get<BalanceOf<Self>>;

        #[pallet::constant]
        type MaxTotalSponsoredPerSponsor: Get<BalanceOf<Self>>;

        #[pallet::constant]
        type MaxSponsoredAccountsPerSponsor: Get<u32>;

        #[pallet::constant]
        type MaxSponsoredAmountPerBlock: Get<BalanceOf<Self>>;

        #[pallet::constant]
        type MaxNewSponsorshipsPerBlock: Get<u32>;

        #[pallet::constant]
        type MaxLeaseDuration: Get<BlockNumberFor<Self>>;

        #[pallet::constant]
        type MaxQueueProcessingPerBlock: Get<u32>;

        #[pallet::constant]
        type SponsorshipStorageDeposit: Get<BalanceOf<Self>>;

        type ForceOrigin: EnsureOrigin<Self::RuntimeOrigin>;
    }

    #[pallet::pallet]
    #[pallet::storage_version(STORAGE_VERSION)]
    pub struct Pallet<T>(_);

    #[derive(
        Encode,
        Decode,
        MaxEncodedLen,
        TypeInfo,
        CloneNoBound,
        EqNoBound,
        PartialEqNoBound,
        RuntimeDebugNoBound,
    )]
    #[scale_info(skip_type_params(T))]
    pub struct SponsorshipRecord<T: Config> {
        pub sponsor: T::AccountId,
        pub amount: BalanceOf<T>,
        pub auto_unlock: AutoUnlockMode,
        pub lease: Lease<BlockNumberFor<T>>,
        pub created_at: BlockNumberFor<T>,
    }

    #[pallet::composite_enum]
    pub enum HoldReason {
        Sponsorship,
        StorageDeposit,
    }

    #[pallet::storage]
    pub type Sponsorships<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, SponsorshipRecord<T>, OptionQuery>;

    #[pallet::storage]
    pub type SponsorLedger<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        T::AccountId,
        SponsorState<BalanceOf<T>, BlockNumberFor<T>>,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type StorageDepositHeld<T: Config> =
        StorageMap<_, Blake2_128Concat, T::AccountId, BalanceOf<T>, ValueQuery>;

    #[pallet::storage]
    pub type Expirations<T: Config> = StorageMap<
        _,
        Blake2_128Concat,
        BlockNumberFor<T>,
        BoundedVec<T::AccountId, T::MaxQueueProcessingPerBlock>,
        ValueQuery,
    >;

    #[pallet::storage]
    pub type AutoUnlockQueue<T: Config> =
        StorageValue<_, BoundedVec<T::AccountId, T::MaxQueueProcessingPerBlock>, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        Sponsored {
            sponsor: T::AccountId,
            beneficiary: T::AccountId,
            amount: BalanceOf<T>,
            auto_unlock: AutoUnlockMode,
            lease: Lease<BlockNumberFor<T>>,
        },
        Unlocked {
            sponsor: T::AccountId,
            beneficiary: T::AccountId,
            amount: BalanceOf<T>,
            reason: UnlockReason,
        },
        PolicyUpdated {
            sponsor: T::AccountId,
            beneficiary: T::AccountId,
            auto_unlock: AutoUnlockMode,
            lease: Lease<BlockNumberFor<T>>,
        },
        Expired {
            sponsor: T::AccountId,
            beneficiary: T::AccountId,
            amount: BalanceOf<T>,
        },
        AutoUnlocked {
            sponsor: T::AccountId,
            beneficiary: T::AccountId,
            amount: BalanceOf<T>,
            condition: AutoUnlockCondition,
        },
    }

    #[pallet::error]
    pub enum Error<T> {
        AlreadySponsored,
        NotSponsored,
        NotSponsor,
        AmountTooLow,
        LeaseTooLong,
        LeaseInPast,
        SponsorCapExceeded,
        SponsorCountCapExceeded,
        SponsorPerBlockCapExceeded,
        SponsorPerBlockCountExceeded,
        SponsorMinRemainingViolation,
        HoldUnavailable,
        CannotTransferAndHold,
        CannotTransferOnHold,
        StorageDepositFailed,
        QueueFull,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(now: BlockNumberFor<T>) -> Weight {
            let expiries = Self::process_expirations(now);
            let auto_unlocks = Self::process_auto_unlock_queue();

            T::WeightInfo::on_initialize_process_expiries(expiries).saturating_add(
                T::WeightInfo::on_initialize_process_auto_unlock(auto_unlocks),
            )
        }
    }

    #[pallet::genesis_config]
    pub struct GenesisConfig<T: Config> {
        pub initial_sponsorships: Vec<(
            T::AccountId,
            T::AccountId,
            BalanceOf<T>,
            AutoUnlockMode,
            Lease<BlockNumberFor<T>>,
        )>,
    }

    impl<T: Config> Default for GenesisConfig<T> {
        fn default() -> Self {
            Self {
                initial_sponsorships: Vec::new(),
            }
        }
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            for (sponsor, beneficiary, amount, auto_unlock, lease) in &self.initial_sponsorships {
                Pallet::<T>::do_sponsor(
                    sponsor,
                    beneficiary,
                    *amount,
                    auto_unlock.clone(),
                    lease.clone(),
                    false,
                )
                .expect("genesis sponsorship configuration must be valid");
            }
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::sponsor())]
        pub fn sponsor(
            origin: OriginFor<T>,
            beneficiary: T::AccountId,
            amount: BalanceOf<T>,
            auto_unlock: AutoUnlockMode,
            lease: Lease<BlockNumberFor<T>>,
        ) -> DispatchResultWithPostInfo {
            let sponsor = ensure_signed(origin)?;
            Self::do_sponsor(&sponsor, &beneficiary, amount, auto_unlock, lease, true)?;
            Ok(().into())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(T::WeightInfo::sponsor_minimum())]
        pub fn sponsor_minimum(
            origin: OriginFor<T>,
            beneficiary: T::AccountId,
            auto_unlock: AutoUnlockMode,
            lease: Lease<BlockNumberFor<T>>,
        ) -> DispatchResultWithPostInfo {
            let sponsor = ensure_signed(origin)?;
            Self::do_sponsor(
                &sponsor,
                &beneficiary,
                Self::minimum_sponsorship(),
                auto_unlock,
                lease,
                true,
            )?;
            Ok(().into())
        }

        #[pallet::call_index(2)]
        #[pallet::weight(T::WeightInfo::unlock())]
        pub fn unlock(
            origin: OriginFor<T>,
            beneficiary: T::AccountId,
        ) -> DispatchResultWithPostInfo {
            let caller = ensure_signed(origin)?;
            let record = Sponsorships::<T>::get(&beneficiary).ok_or(Error::<T>::NotSponsored)?;
            ensure!(caller == record.sponsor, Error::<T>::NotSponsor);

            let record = Self::revoke_sponsorship(&beneficiary)?;
            Self::deposit_event(Event::Unlocked {
                sponsor: record.sponsor,
                beneficiary,
                amount: record.amount,
                reason: UnlockReason::Manual,
            });

            Ok(().into())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(T::WeightInfo::set_policy())]
        #[transactional]
        pub fn set_policy(
            origin: OriginFor<T>,
            beneficiary: T::AccountId,
            auto_unlock: AutoUnlockMode,
            lease: Lease<BlockNumberFor<T>>,
        ) -> DispatchResultWithPostInfo {
            let caller = ensure_signed(origin)?;
            let mut record =
                Sponsorships::<T>::get(&beneficiary).ok_or(Error::<T>::NotSponsored)?;
            ensure!(caller == record.sponsor, Error::<T>::NotSponsor);

            let now = frame_system::Pallet::<T>::block_number();
            Self::validate_lease(&lease, now)?;

            if record.lease != lease {
                Self::insert_expiration(&beneficiary, &lease)?;
                Self::remove_expiration(&beneficiary, &record.lease);
            }

            if matches!(auto_unlock, AutoUnlockMode::Enabled(_)) {
                Self::enqueue_auto_unlock(&beneficiary)?;
            }

            record.auto_unlock = auto_unlock.clone();
            record.lease = lease.clone();
            Sponsorships::<T>::insert(&beneficiary, record);

            Self::deposit_event(Event::PolicyUpdated {
                sponsor: caller,
                beneficiary,
                auto_unlock,
                lease,
            });

            Ok(().into())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(T::WeightInfo::force_unlock())]
        pub fn force_unlock(
            origin: OriginFor<T>,
            beneficiary: T::AccountId,
        ) -> DispatchResultWithPostInfo {
            T::ForceOrigin::ensure_origin(origin)?;

            let record = Self::revoke_sponsorship(&beneficiary)?;
            Self::deposit_event(Event::Unlocked {
                sponsor: record.sponsor,
                beneficiary,
                amount: record.amount,
                reason: UnlockReason::Force,
            });

            Ok(().into())
        }

        #[pallet::call_index(5)]
        #[pallet::weight(T::WeightInfo::refresh())]
        pub fn refresh(
            origin: OriginFor<T>,
            beneficiary: T::AccountId,
        ) -> DispatchResultWithPostInfo {
            let _caller = ensure_signed(origin)?;
            let _ = Self::try_auto_unlock_internal(&beneficiary, true)?;
            Ok(().into())
        }
    }

    impl<T: Config> Pallet<T> {
        pub(crate) fn minimum_sponsorship() -> BalanceOf<T> {
            T::MinSponsorship::get().max(T::Currency::minimum_balance())
        }

        pub(crate) fn sponsorship_hold_reason() -> T::RuntimeHoldReason {
            HoldReason::Sponsorship.into()
        }

        pub(crate) fn storage_deposit_hold_reason() -> T::RuntimeHoldReason {
            HoldReason::StorageDeposit.into()
        }

        pub(crate) fn validate_lease(
            lease: &Lease<BlockNumberFor<T>>,
            now: BlockNumberFor<T>,
        ) -> DispatchResult {
            match lease {
                Lease::None => Ok(()),
                Lease::Until(end) => {
                    ensure!(*end >= now, Error::<T>::LeaseInPast);
                    ensure!(
                        end.saturating_sub(now) <= T::MaxLeaseDuration::get(),
                        Error::<T>::LeaseTooLong
                    );
                    Ok(())
                }
            }
        }

        fn refresh_per_block_ledger(
            ledger: &mut SponsorState<BalanceOf<T>, BlockNumberFor<T>>,
            now: BlockNumberFor<T>,
        ) {
            if ledger.last_block != now {
                ledger.last_block = now;
                ledger.sponsored_amount_in_block = Zero::zero();
                ledger.new_sponsorships_in_block = 0;
            }
        }

        pub(crate) fn insert_expiration(
            beneficiary: &T::AccountId,
            lease: &Lease<BlockNumberFor<T>>,
        ) -> DispatchResult {
            let Lease::Until(expiry) = lease else {
                return Ok(());
            };

            let mut bucket = Expirations::<T>::get(expiry);
            if !bucket.contains(beneficiary) {
                bucket
                    .try_push(beneficiary.clone())
                    .map_err(|_| Error::<T>::QueueFull)?;
                Expirations::<T>::insert(expiry, bucket);
            }

            Ok(())
        }

        pub(crate) fn remove_expiration(
            beneficiary: &T::AccountId,
            lease: &Lease<BlockNumberFor<T>>,
        ) {
            let Lease::Until(expiry) = lease else {
                return;
            };

            Expirations::<T>::mutate(expiry, |bucket| {
                bucket.retain(|account| account != beneficiary);
            });
        }

        pub(crate) fn enqueue_auto_unlock(beneficiary: &T::AccountId) -> DispatchResult {
            let mut queue = AutoUnlockQueue::<T>::get();
            if !queue.contains(beneficiary) {
                queue
                    .try_push(beneficiary.clone())
                    .map_err(|_| Error::<T>::QueueFull)?;
                AutoUnlockQueue::<T>::put(queue);
            }

            Ok(())
        }

        pub(crate) fn dequeue_auto_unlock(beneficiary: &T::AccountId) {
            AutoUnlockQueue::<T>::mutate(|queue| {
                queue.retain(|account| account != beneficiary);
            });
        }

        fn auto_unlock_condition_met(
            beneficiary: &T::AccountId,
            amount: BalanceOf<T>,
            condition: &AutoUnlockCondition,
        ) -> bool {
            match condition {
                AutoUnlockCondition::SpendableGeSponsored => {
                    T::Currency::reducible_balance(
                        beneficiary,
                        Preservation::Preserve,
                        Fortitude::Polite,
                    ) >= amount
                }
                AutoUnlockCondition::FreeGeSponsored => T::Currency::balance(beneficiary) >= amount,
            }
        }

        #[transactional]
        pub(crate) fn do_sponsor(
            sponsor: &T::AccountId,
            beneficiary: &T::AccountId,
            amount: BalanceOf<T>,
            auto_unlock: AutoUnlockMode,
            lease: Lease<BlockNumberFor<T>>,
            emit_event: bool,
        ) -> DispatchResult {
            ensure!(
                !Sponsorships::<T>::contains_key(beneficiary),
                Error::<T>::AlreadySponsored
            );

            ensure!(
                amount >= Self::minimum_sponsorship(),
                Error::<T>::AmountTooLow
            );

            let now = frame_system::Pallet::<T>::block_number();
            Self::validate_lease(&lease, now)?;

            let mut ledger = SponsorLedger::<T>::get(sponsor);
            Self::refresh_per_block_ledger(&mut ledger, now);

            ensure!(
                ledger.active_amount.saturating_add(amount)
                    <= T::MaxTotalSponsoredPerSponsor::get(),
                Error::<T>::SponsorCapExceeded
            );
            ensure!(
                ledger.active_count.saturating_add(1) <= T::MaxSponsoredAccountsPerSponsor::get(),
                Error::<T>::SponsorCountCapExceeded
            );
            ensure!(
                ledger.sponsored_amount_in_block.saturating_add(amount)
                    <= T::MaxSponsoredAmountPerBlock::get(),
                Error::<T>::SponsorPerBlockCapExceeded
            );
            ensure!(
                ledger.new_sponsorships_in_block.saturating_add(1)
                    <= T::MaxNewSponsorshipsPerBlock::get(),
                Error::<T>::SponsorPerBlockCountExceeded
            );

            let storage_deposit = T::SponsorshipStorageDeposit::get();
            let total_deduction = amount.saturating_add(storage_deposit);

            ensure!(
                T::Currency::reducible_balance(sponsor, Preservation::Preserve, Fortitude::Polite,)
                    >= total_deduction,
                Error::<T>::SponsorMinRemainingViolation
            );
            ensure!(
                T::Currency::balance(sponsor).saturating_sub(total_deduction)
                    >= T::SponsorMinRemaining::get(),
                Error::<T>::SponsorMinRemainingViolation
            );

            let sponsorship_reason = Self::sponsorship_hold_reason();
            frame_system::Pallet::<T>::inc_providers(beneficiary);
            ensure!(
                T::Currency::hold_available(&sponsorship_reason, beneficiary),
                Error::<T>::HoldUnavailable
            );

            if !storage_deposit.is_zero() {
                let storage_reason = Self::storage_deposit_hold_reason();
                ensure!(
                    T::Currency::hold_available(&storage_reason, sponsor),
                    Error::<T>::HoldUnavailable
                );
            }

            T::Currency::transfer_and_hold(
                &sponsorship_reason,
                sponsor,
                beneficiary,
                amount,
                Precision::Exact,
                Preservation::Preserve,
                Fortitude::Polite,
            )
            .map_err(|_| Error::<T>::CannotTransferAndHold)?;

            if !storage_deposit.is_zero() {
                T::Currency::hold(
                    &Self::storage_deposit_hold_reason(),
                    sponsor,
                    storage_deposit,
                )
                .map_err(|_| Error::<T>::StorageDepositFailed)?;
                StorageDepositHeld::<T>::insert(beneficiary, storage_deposit);
            }

            let record = SponsorshipRecord::<T> {
                sponsor: sponsor.clone(),
                amount,
                auto_unlock: auto_unlock.clone(),
                lease: lease.clone(),
                created_at: now,
            };

            Sponsorships::<T>::insert(beneficiary, record);

            ledger.active_amount = ledger.active_amount.saturating_add(amount);
            ledger.active_count = ledger.active_count.saturating_add(1);
            ledger.sponsored_amount_in_block =
                ledger.sponsored_amount_in_block.saturating_add(amount);
            ledger.new_sponsorships_in_block = ledger.new_sponsorships_in_block.saturating_add(1);
            SponsorLedger::<T>::insert(sponsor, ledger);

            Self::insert_expiration(beneficiary, &lease)?;

            if matches!(auto_unlock, AutoUnlockMode::Enabled(_)) {
                Self::enqueue_auto_unlock(beneficiary)?;
            }

            if emit_event {
                Self::deposit_event(Event::Sponsored {
                    sponsor: sponsor.clone(),
                    beneficiary: beneficiary.clone(),
                    amount,
                    auto_unlock,
                    lease,
                });
            }

            Ok(())
        }

        #[transactional]
        pub(crate) fn revoke_sponsorship(
            beneficiary: &T::AccountId,
        ) -> Result<SponsorshipRecord<T>, DispatchError> {
            let record = Sponsorships::<T>::get(beneficiary).ok_or(Error::<T>::NotSponsored)?;

            T::Currency::transfer_on_hold(
                &Self::sponsorship_hold_reason(),
                beneficiary,
                &record.sponsor,
                record.amount,
                Precision::Exact,
                Restriction::Free,
                Fortitude::Polite,
            )
            .map_err(|_| Error::<T>::CannotTransferOnHold)?;

            let storage_deposit = StorageDepositHeld::<T>::take(beneficiary);
            if !storage_deposit.is_zero() {
                T::Currency::release(
                    &Self::storage_deposit_hold_reason(),
                    &record.sponsor,
                    storage_deposit,
                    Precision::Exact,
                )
                .map_err(|_| Error::<T>::StorageDepositFailed)?;
            }

            Sponsorships::<T>::remove(beneficiary);
            Self::remove_expiration(beneficiary, &record.lease);
            Self::dequeue_auto_unlock(beneficiary);
            let _ = frame_system::Pallet::<T>::dec_providers(beneficiary);

            SponsorLedger::<T>::mutate(&record.sponsor, |ledger| {
                ledger.active_amount = ledger.active_amount.saturating_sub(record.amount);
                ledger.active_count = ledger.active_count.saturating_sub(1);
            });

            Ok(record)
        }

        pub(crate) fn try_auto_unlock_internal(
            beneficiary: &T::AccountId,
            emit_event: bool,
        ) -> Result<bool, DispatchError> {
            let record = Sponsorships::<T>::get(beneficiary).ok_or(Error::<T>::NotSponsored)?;
            let AutoUnlockMode::Enabled(condition) = record.auto_unlock.clone() else {
                return Ok(false);
            };

            if !Self::auto_unlock_condition_met(beneficiary, record.amount, &condition) {
                return Ok(false);
            }

            let record = Self::revoke_sponsorship(beneficiary)?;
            if emit_event {
                Self::deposit_event(Event::AutoUnlocked {
                    sponsor: record.sponsor,
                    beneficiary: beneficiary.clone(),
                    amount: record.amount,
                    condition,
                });
            }

            Ok(true)
        }

        fn process_auto_unlock_queue() -> u32 {
            let limit = T::MaxQueueProcessingPerBlock::get() as usize;
            let queue = AutoUnlockQueue::<T>::take();

            let mut unprocessed = Vec::new();
            let mut requeue = Vec::new();
            let mut processed = 0u32;

            for (index, beneficiary) in queue.into_inner().into_iter().enumerate() {
                if index >= limit {
                    unprocessed.push(beneficiary);
                    continue;
                }

                processed = processed.saturating_add(1);

                let Some(record) = Sponsorships::<T>::get(&beneficiary) else {
                    continue;
                };

                let AutoUnlockMode::Enabled(condition) = record.auto_unlock.clone() else {
                    continue;
                };

                if Self::auto_unlock_condition_met(&beneficiary, record.amount, &condition) {
                    match Self::revoke_sponsorship(&beneficiary) {
                        Ok(record) => Self::deposit_event(Event::AutoUnlocked {
                            sponsor: record.sponsor,
                            beneficiary,
                            amount: record.amount,
                            condition,
                        }),
                        Err(_) => requeue.push(beneficiary),
                    }
                } else {
                    requeue.push(beneficiary);
                }
            }

            unprocessed.extend(requeue);
            let next_queue =
                BoundedVec::<T::AccountId, T::MaxQueueProcessingPerBlock>::try_from(unprocessed)
                    .expect("processing never increases queued items; qed");
            AutoUnlockQueue::<T>::put(next_queue);

            processed
        }

        fn process_expirations(now: BlockNumberFor<T>) -> u32 {
            let expiring = Expirations::<T>::take(now);
            let processed = expiring.len() as u32;

            for beneficiary in expiring.into_inner() {
                let Some(record) = Sponsorships::<T>::get(&beneficiary) else {
                    continue;
                };

                if record.lease != Lease::Until(now) {
                    continue;
                }

                if let Ok(record) = Self::revoke_sponsorship(&beneficiary) {
                    Self::deposit_event(Event::Expired {
                        sponsor: record.sponsor,
                        beneficiary,
                        amount: record.amount,
                    });
                }
            }

            processed
        }
    }
}

impl<T: pallet::Config>
    SponsorshipProvider<
        <T as frame_system::Config>::AccountId,
        BalanceOf<T>,
        frame_system::pallet_prelude::BlockNumberFor<T>,
    > for pallet::Pallet<T>
{
    fn sponsorship_of(
        beneficiary: &<T as frame_system::Config>::AccountId,
    ) -> Option<
        SponsorshipView<
            <T as frame_system::Config>::AccountId,
            BalanceOf<T>,
            frame_system::pallet_prelude::BlockNumberFor<T>,
        >,
    > {
        pallet::Sponsorships::<T>::get(beneficiary).map(|record| SponsorshipView {
            sponsor: record.sponsor,
            beneficiary: beneficiary.clone(),
            amount: record.amount,
            auto_unlock: record.auto_unlock,
            lease: record.lease,
            created_at: record.created_at,
        })
    }

    fn is_sponsored(beneficiary: &<T as frame_system::Config>::AccountId) -> bool {
        pallet::Sponsorships::<T>::contains_key(beneficiary)
    }

    fn sponsored_amount(beneficiary: &<T as frame_system::Config>::AccountId) -> BalanceOf<T> {
        pallet::Sponsorships::<T>::get(beneficiary)
            .map(|record| record.amount)
            .unwrap_or_else(Zero::zero)
    }

    fn sponsor_of(
        beneficiary: &<T as frame_system::Config>::AccountId,
    ) -> Option<<T as frame_system::Config>::AccountId> {
        pallet::Sponsorships::<T>::get(beneficiary).map(|record| record.sponsor)
    }

    fn create_sponsorship(
        sponsor: &<T as frame_system::Config>::AccountId,
        beneficiary: &<T as frame_system::Config>::AccountId,
        amount: BalanceOf<T>,
        auto_unlock: AutoUnlockMode,
        lease: Lease<frame_system::pallet_prelude::BlockNumberFor<T>>,
    ) -> frame::deps::sp_runtime::DispatchResult {
        pallet::Pallet::<T>::do_sponsor(sponsor, beneficiary, amount, auto_unlock, lease, true)
    }

    fn unlock_sponsorship(
        sponsor: &<T as frame_system::Config>::AccountId,
        beneficiary: &<T as frame_system::Config>::AccountId,
    ) -> frame::deps::sp_runtime::DispatchResult {
        let record =
            pallet::Sponsorships::<T>::get(beneficiary).ok_or(pallet::Error::<T>::NotSponsored)?;
        if record.sponsor != *sponsor {
            return Err(pallet::Error::<T>::NotSponsor.into());
        }

        let record = pallet::Pallet::<T>::revoke_sponsorship(beneficiary)?;
        pallet::Pallet::<T>::deposit_event(pallet::Event::Unlocked {
            sponsor: record.sponsor,
            beneficiary: beneficiary.clone(),
            amount: record.amount,
            reason: UnlockReason::Manual,
        });

        Ok(())
    }

    fn try_auto_unlock(
        beneficiary: &<T as frame_system::Config>::AccountId,
    ) -> frame::deps::sp_runtime::DispatchResult {
        let _ = pallet::Pallet::<T>::try_auto_unlock_internal(beneficiary, true)?;
        Ok(())
    }
}
