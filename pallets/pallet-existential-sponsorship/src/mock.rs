use frame::{
    deps::{
        frame_support::{derive_impl, parameter_types, weights::constants::RocksDbWeight},
        sp_runtime::{BuildStorage, traits::IdentityLookup},
    },
    prelude::*,
    runtime::prelude::*,
    testing_prelude::*,
};
use polkadot_sdk::*;

pub type Balance = u128;

#[frame_construct_runtime]
mod test_runtime {
    #[runtime::runtime]
    #[runtime::derive(
        RuntimeCall,
        RuntimeEvent,
        RuntimeError,
        RuntimeOrigin,
        RuntimeFreezeReason,
        RuntimeHoldReason,
        RuntimeSlashReason,
        RuntimeLockId,
        RuntimeTask,
        RuntimeViewFunction
    )]
    pub struct Test;

    #[runtime::pallet_index(0)]
    pub type System = frame_system;
    #[runtime::pallet_index(1)]
    pub type Balances = pallet_balances;
    #[runtime::pallet_index(2)]
    pub type ExistentialSponsorship = crate;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Block = MockBlock<Test>;
    type AccountData = pallet_balances::AccountData<Balance>;
    type BlockHashCount = ConstU64<250>;
    type DbWeight = RocksDbWeight;
}

parameter_types! {
    pub const ExistentialDeposit: Balance = 10;
}

impl pallet_balances::Config for Test {
    type MaxLocks = ConstU32<50>;
    type Balance = Balance;
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = ();
    type MaxReserves = ConstU32<50>;
    type ReserveIdentifier = [u8; 8];
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = ConstU32<50>;
    type DoneSlashHandler = ();
}

parameter_types! {
    pub const MinSponsorship: Balance = 10;
    pub const SponsorMinRemaining: Balance = 25;
    pub const MaxTotalSponsoredPerSponsor: Balance = 1_000;
    pub const MaxSponsoredAccountsPerSponsor: u32 = 8;
    pub const MaxSponsoredAmountPerBlock: Balance = 500;
    pub const MaxNewSponsorshipsPerBlock: u32 = 2;
    pub const MaxLeaseDuration: u64 = 100;
    pub const MaxQueueProcessingPerBlock: u32 = 8;
    pub const SponsorshipStorageDeposit: Balance = 5;
}

impl crate::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type RuntimeHoldReason = RuntimeHoldReason;
    type WeightInfo = ();
    type MinSponsorship = MinSponsorship;
    type SponsorMinRemaining = SponsorMinRemaining;
    type MaxTotalSponsoredPerSponsor = MaxTotalSponsoredPerSponsor;
    type MaxSponsoredAccountsPerSponsor = MaxSponsoredAccountsPerSponsor;
    type MaxSponsoredAmountPerBlock = MaxSponsoredAmountPerBlock;
    type MaxNewSponsorshipsPerBlock = MaxNewSponsorshipsPerBlock;
    type MaxLeaseDuration = MaxLeaseDuration;
    type MaxQueueProcessingPerBlock = MaxQueueProcessingPerBlock;
    type SponsorshipStorageDeposit = SponsorshipStorageDeposit;
    type ForceOrigin = frame_system::EnsureRoot<u64>;
}

pub const SPONSOR: u64 = 1;
pub const BENEFICIARY: u64 = 2;
pub const CALLER: u64 = 3;
pub const ALT_SPONSOR: u64 = 4;
pub const LOW_BALANCE_SPONSOR: u64 = 5;

pub fn sponsorship_hold_reason() -> RuntimeHoldReason {
    RuntimeHoldReason::ExistentialSponsorship(crate::HoldReason::Sponsorship)
}

pub fn storage_deposit_hold_reason() -> RuntimeHoldReason {
    RuntimeHoldReason::ExistentialSponsorship(crate::HoldReason::StorageDeposit)
}

pub fn new_test_ext() -> TestState {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![
            (SPONSOR, 500),
            (CALLER, 100),
            (ALT_SPONSOR, 500),
            (LOW_BALANCE_SPONSOR, 140),
        ],
        ..Default::default()
    }
    .assimilate_storage(&mut storage)
    .unwrap();

    let mut ext: TestState = storage.into();
    ext.execute_with(|| {
        System::set_block_number(1);
    });
    ext
}
