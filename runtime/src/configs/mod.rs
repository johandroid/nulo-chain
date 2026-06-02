// This is free and unencumbered software released into the public domain.
//
// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.
//
// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.
//
// For more information, please refer to <http://unlicense.org>

mod xcm_config;

use alloc::{boxed::Box, vec::Vec};

use anyhow::anyhow;
use ismp::{host::StateMachine, module::IsmpModule, router::IsmpRouter};
use pallet_ismp::fee_handler::WeightFeeHandler;
use polkadot_sdk::{staging_parachain_info as parachain_info, staging_xcm as xcm, *};
#[cfg(not(feature = "runtime-benchmarks"))]
use polkadot_sdk::{staging_xcm_builder as xcm_builder, staging_xcm_executor as xcm_executor};

// Substrate and Polkadot dependencies
use cumulus_pallet_parachain_system::RelayNumberMonotonicallyIncreases;
use cumulus_primitives_core::{AggregateMessageOrigin, ParaId};
use frame_support::{
    PalletId, derive_impl,
    dispatch::DispatchClass,
    parameter_types,
    traits::{
        ConstBool, ConstU8, ConstU32, ConstU64, EitherOfDiverse, Get, TransformOrigin, TypedGet,
        VariantCountOf,
    },
    weights::{ConstantMultiplier, Weight},
};
use frame_system::{
    EnsureRoot, EnsureRootWithSuccess, EnsureSigned,
    limits::{BlockLength, BlockWeights},
};
use pallet_xcm::{EnsureXcm, IsVoiceOfBody};
use parachains_common::message_queue::{NarrowOriginToSibling, ParaIdToSibling};
use polkadot_runtime_common::{
    BlockHashCount, SlowAdjustingFeeUpdate, xcm_sender::ExponentialPrice,
};
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_runtime::{FixedU128, Perbill, traits::AccountIdConversion};
use sp_version::RuntimeVersion;
use xcm::latest::prelude::{AssetId, BodyId};

// Local module imports
use super::{
    AVERAGE_ON_INITIALIZE_RATIO, AccountId, Assets, Aura, Balance, Balances, Block, BlockNumber,
    CENTS, CollatorSelection, ConsensusHook, DAYS, EXISTENTIAL_DEPOSIT, HOURS, Hash,
    HyperFungibleToken, Hyperbridge, Ismp, IsmpParachain, MAXIMUM_BLOCK_WEIGHT, MICRO_UNIT,
    MILLI_UNIT, MessageQueue, NORMAL_DISPATCH_RATIO, Nonce, PARACHAIN_ID, PalletInfo,
    ParachainSystem, PrepaidGas, Runtime, RuntimeCall, RuntimeEvent, RuntimeFreezeReason,
    RuntimeHoldReason, RuntimeOrigin, RuntimeTask, SLOT_DURATION, Session, SessionKeys, System,
    Timestamp, TokenGateway, VERSION, WeightToFee, XcmpQueue,
    weights::{BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight},
};
use xcm_config::{RelayLocation, XcmOriginToTransactDispatchOrigin};

parameter_types! {
    pub const Version: RuntimeVersion = VERSION;

    // This part is copied from Substrate's `bin/node/runtime/src/lib.rs`.
    //  The `RuntimeBlockLength` and `RuntimeBlockWeights` exist here because the
    // `DeletionWeightLimit` and `DeletionQueueDepth` depend on those to parameterize
    // the lazy contract deletion.
    pub RuntimeBlockLength: BlockLength =
        BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
    pub RuntimeBlockWeights: BlockWeights = BlockWeights::builder()
        .base_block(BlockExecutionWeight::get())
        .for_class(DispatchClass::all(), |weights| {
            weights.base_extrinsic = ExtrinsicBaseWeight::get();
        })
        .for_class(DispatchClass::Normal, |weights| {
            weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
        })
        .for_class(DispatchClass::Operational, |weights| {
            weights.max_total = Some(MAXIMUM_BLOCK_WEIGHT);
            // Operational transactions have some extra reserved space, so that they
            // are included even if block reached `MAXIMUM_BLOCK_WEIGHT`.
            weights.reserved = Some(
                MAXIMUM_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT
            );
        })
        .avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
        .build_or_panic();
    pub const SS58Prefix: u16 = 42;
}

/// All migrations of the runtime, aside from the ones declared in the pallets.
///
/// This can be a tuple of types, each implementing `OnRuntimeUpgrade`.
#[allow(unused_parens)]
type SingleBlockMigrations = ();

/// The default types are being injected by [`derive_impl`](`frame_support::derive_impl`) from
/// [`ParaChainDefaultConfig`](`struct@frame_system::config_preludes::ParaChainDefaultConfig`),
/// but overridden as needed.
#[derive_impl(frame_system::config_preludes::ParaChainDefaultConfig)]
impl frame_system::Config for Runtime {
    /// The identifier used to distinguish between accounts.
    type AccountId = AccountId;
    /// The index type for storing how many extrinsics an account has signed.
    type Nonce = Nonce;
    /// The type for hashing blocks and tries.
    type Hash = Hash;
    /// The block type.
    type Block = Block;
    /// Maximum number of block number to block hash mappings to keep (oldest pruned first).
    type BlockHashCount = BlockHashCount;
    /// Runtime version.
    type Version = Version;
    /// The data to be stored in an account.
    type AccountData = pallet_balances::AccountData<Balance>;
    /// The weight of database operations that the runtime can invoke.
    type DbWeight = RocksDbWeight;
    /// Block & extrinsics weights: base values and limits.
    type BlockWeights = RuntimeBlockWeights;
    /// The maximum length of a block (in bytes).
    type BlockLength = RuntimeBlockLength;
    /// This is used as an identifier of the chain. 42 is the generic substrate prefix.
    type SS58Prefix = SS58Prefix;
    /// The action to take on a Runtime Upgrade
    type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Self>;
    type MaxConsumers = frame_support::traits::ConstU32<16>;
    type SingleBlockMigrations = SingleBlockMigrations;
}

/// Configure the palelt weight reclaim tx.
impl cumulus_pallet_weight_reclaim::Config for Runtime {
    type WeightInfo = ();
}

impl pallet_timestamp::Config for Runtime {
    /// A timestamp: milliseconds since the unix epoch.
    type Moment = u64;
    type OnTimestampSet = Aura;
    type MinimumPeriod = ConstU64<0>;
    type WeightInfo = ();
}

impl pallet_authorship::Config for Runtime {
    type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
    type EventHandler = (CollatorSelection,);
}

parameter_types! {
    pub const ExistentialDeposit: Balance = EXISTENTIAL_DEPOSIT;
}

impl pallet_balances::Config for Runtime {
    type MaxLocks = ConstU32<50>;
    /// The type for recording an account's balance.
    type Balance = Balance;
    /// The ubiquitous event type.
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
    type MaxReserves = ConstU32<50>;
    type ReserveIdentifier = [u8; 8];
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type FreezeIdentifier = RuntimeFreezeReason;
    type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
    type DoneSlashHandler = ();
}

parameter_types! {
    pub const AssetDeposit: Balance = 0;
    pub const AssetAccountDeposit: Balance = 0;
    pub const MetadataDepositBase: Balance = 0;
    pub const MetadataDepositPerByte: Balance = 0;
    pub const ApprovalDeposit: Balance = 0;
    pub const AssetsStringLimit: u32 = 50;
    pub const RemoveItemsLimit: u32 = 1_000;
    pub const NativeAssetId: u32 = 0;
    pub const TokenGatewayDecimals: u8 = 12;
    pub const AssetAdminPalletId: PalletId = PalletId(*b"tgadmin!");
    pub const IsmpFeesPalletId: PalletId = PalletId(*b"ismpfees");
}

pub struct AssetAdmin;

impl Get<AccountId> for AssetAdmin {
    fn get() -> AccountId {
        AssetAdminPalletId::get().into_account_truncating()
    }
}

impl TypedGet for AssetAdmin {
    type Type = AccountId;

    fn get() -> Self::Type {
        <Self as Get<AccountId>>::get()
    }
}

impl pallet_assets::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Balance = Balance;
    type RemoveItemsLimit = RemoveItemsLimit;
    type AssetId = u32;
    type AssetIdParameter = u32;
    type ReserveData = ();
    type Currency = Balances;
    type CreateOrigin = EnsureRootWithSuccess<AccountId, AssetAdmin>;
    type ForceOrigin = EnsureRoot<AccountId>;
    type AssetDeposit = AssetDeposit;
    type AssetAccountDeposit = AssetAccountDeposit;
    type MetadataDepositBase = MetadataDepositBase;
    type MetadataDepositPerByte = MetadataDepositPerByte;
    type ApprovalDeposit = ApprovalDeposit;
    type StringLimit = AssetsStringLimit;
    type Freezer = ();
    type Holder = ();
    type Extra = ();
    type CallbackHandle = ();
    type WeightInfo = pallet_assets::weights::SubstrateWeight<Runtime>;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
}

parameter_types! {
    /// Relay Chain `TransactionByteFee` / 10
    pub const TransactionByteFee: Balance = 10 * MICRO_UNIT;
}

impl pallet_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction =
        pallet_gas_transaction_payment::PrepaidFeeAdapter<Balances, PrepaidGas, ()>;
    type WeightToFee = WeightToFee;
    type LengthToFee = ConstantMultiplier<Balance, TransactionByteFee>;
    type FeeMultiplierUpdate = SlowAdjustingFeeUpdate<Self>;
    type OperationalFeeMultiplier = ConstU8<5>;
    type WeightInfo = ();
}

parameter_types! {
    pub const PrepaidGasPalletId: PalletId = PalletId(*b"prpgas!!");
    pub const MinPrepaidGasPurchase: Balance = 1;
}

impl pallet_prepaid_gas::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type PalletId = PrepaidGasPalletId;
    type MinPurchase = MinPrepaidGasPurchase;
    type WeightInfo = pallet_prepaid_gas::weights::SubstrateWeight<Runtime>;
}

impl pallet_gas_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = pallet_gas_transaction_payment::weights::SubstrateWeight<Runtime>;
}

impl pallet_sudo::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = ();
}

parameter_types! {
    pub const ReviveDepositPerItem: Balance = 20 * CENTS;
    pub const ReviveDepositPerByte: Balance = MILLI_UNIT;
    pub const ReviveDepositPerChildTrieItem: Balance = ReviveDepositPerItem::get() / 100;
    pub const ReviveCodeHashLockupDepositPercent: Perbill = Perbill::from_percent(0);
    pub const ReviveRuntimeMemory: u32 = 128 * 1024 * 1024;
    pub const RevivePvfMemory: u32 = 512 * 1024 * 1024;
    pub const ReviveChainId: u64 = PARACHAIN_ID as u64;
    pub const ReviveNativeToEthRatio: u32 = 1_000_000;
    pub const ReviveMaxEthExtrinsicWeight: FixedU128 = FixedU128::from_rational(9, 10);
    pub const ReviveDebugEnabled: bool = false;
    pub const ReviveGasScale: u32 = 10;
}

impl pallet_revive::Config for Runtime {
    type Time = Timestamp;
    type Balance = Balance;
    type Currency = Balances;
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeHoldReason = RuntimeHoldReason;
    type WeightInfo = pallet_revive::weights::SubstrateWeight<Runtime>;
    type Precompiles = ();
    type FindAuthor = ();
    type DepositPerByte = ReviveDepositPerByte;
    type DepositPerItem = ReviveDepositPerItem;
    type DepositPerChildTrieItem = ReviveDepositPerChildTrieItem;
    type CodeHashLockupDepositPercent = ReviveCodeHashLockupDepositPercent;
    type AddressMapper = pallet_revive::AccountId32Mapper<Self>;
    type UnsafeUnstableInterface = ConstBool<false>;
    type AllowEVMBytecode = ConstBool<true>;
    type UploadOrigin = EnsureSigned<AccountId>;
    type InstantiateOrigin = EnsureSigned<AccountId>;
    type RuntimeMemory = ReviveRuntimeMemory;
    type PVFMemory = RevivePvfMemory;
    type ChainId = ReviveChainId;
    type NativeToEthRatio = ReviveNativeToEthRatio;
    type FeeInfo = ();
    type MaxEthExtrinsicWeight = ReviveMaxEthExtrinsicWeight;
    type DebugEnabled = ReviveDebugEnabled;
    type GasScale = ReviveGasScale;
}

parameter_types! {
    pub const ReservedXcmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
    pub const ReservedDmpWeight: Weight = MAXIMUM_BLOCK_WEIGHT.saturating_div(4);
    pub const RelayOrigin: AggregateMessageOrigin = AggregateMessageOrigin::Parent;
}

impl cumulus_pallet_parachain_system::Config for Runtime {
    type WeightInfo = ();
    type RuntimeEvent = RuntimeEvent;
    type OnSystemEvent = ();
    type SelfParaId = parachain_info::Pallet<Runtime>;
    type OutboundXcmpMessageSource = XcmpQueue;
    type DmpQueue = frame_support::traits::EnqueueWithOrigin<MessageQueue, RelayOrigin>;
    type ReservedDmpWeight = ReservedDmpWeight;
    type XcmpMessageHandler = XcmpQueue;
    type ReservedXcmpWeight = ReservedXcmpWeight;
    type CheckAssociatedRelayNumber = RelayNumberMonotonicallyIncreases;
    type ConsensusHook = ConsensusHook;
    type RelayParentOffset = ConstU32<0>;
}

impl parachain_info::Config for Runtime {}

parameter_types! {
    pub MessageQueueServiceWeight: Weight = Perbill::from_percent(35) * RuntimeBlockWeights::get().max_block;
}

impl pallet_message_queue::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    #[cfg(feature = "runtime-benchmarks")]
    type MessageProcessor = pallet_message_queue::mock_helpers::NoopMessageProcessor<
        cumulus_primitives_core::AggregateMessageOrigin,
    >;
    #[cfg(not(feature = "runtime-benchmarks"))]
    type MessageProcessor = xcm_builder::ProcessXcmMessage<
        AggregateMessageOrigin,
        xcm_executor::XcmExecutor<xcm_config::XcmConfig>,
        RuntimeCall,
    >;
    type Size = u32;
    // The XCMP queue pallet is only ever able to handle the `Sibling(ParaId)` origin:
    type QueueChangeHandler = NarrowOriginToSibling<XcmpQueue>;
    type QueuePausedQuery = NarrowOriginToSibling<XcmpQueue>;
    type HeapSize = sp_core::ConstU32<{ 103 * 1024 }>;
    type MaxStale = sp_core::ConstU32<8>;
    type ServiceWeight = MessageQueueServiceWeight;
    type IdleMaxServiceWeight = ();
}

impl cumulus_pallet_aura_ext::Config for Runtime {}

parameter_types! {
    /// The asset ID for the asset that we use to pay for message delivery fees.
    pub FeeAssetId: AssetId = AssetId(xcm_config::RelayLocation::get());
    /// The base fee for the message delivery fees.
    pub const ToSiblingBaseDeliveryFee: u128 = CENTS.saturating_mul(3);
    pub const ToParentBaseDeliveryFee: u128 = CENTS.saturating_mul(3);
}

/// The price for delivering XCM messages to sibling parachains.
pub type PriceForSiblingParachainDelivery =
    ExponentialPrice<FeeAssetId, ToSiblingBaseDeliveryFee, TransactionByteFee, XcmpQueue>;

/// The price for delivering XCM messages to relay chain.
pub type PriceForParentDelivery =
    ExponentialPrice<FeeAssetId, ToParentBaseDeliveryFee, TransactionByteFee, ParachainSystem>;

impl cumulus_pallet_xcmp_queue::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ChannelInfo = ParachainSystem;
    type VersionWrapper = ();
    // Enqueue XCMP messages from siblings for later processing.
    type XcmpQueue = TransformOrigin<MessageQueue, AggregateMessageOrigin, ParaId, ParaIdToSibling>;
    type MaxInboundSuspended = sp_core::ConstU32<1_000>;
    type MaxActiveOutboundChannels = ConstU32<128>;
    type MaxPageSize = ConstU32<{ 1 << 16 }>;
    type ControllerOrigin = EnsureRoot<AccountId>;
    type ControllerOriginConverter = XcmOriginToTransactDispatchOrigin;
    type WeightInfo = ();
    type PriceForSiblingDelivery = PriceForSiblingParachainDelivery;
}

parameter_types! {
    pub const Period: u32 = 6 * HOURS;
    pub const Offset: u32 = 0;
}

impl pallet_session::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ValidatorId = <Self as frame_system::Config>::AccountId;
    // we don't have stash and controller, thus we don't need the convert as well.
    type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
    type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
    type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
    type SessionManager = CollatorSelection;
    // Essentially just Aura, but let's be pedantic.
    type SessionHandler = <SessionKeys as sp_runtime::traits::OpaqueKeys>::KeyTypeIdProviders;
    type Keys = SessionKeys;
    type DisablingStrategy = ();
    type WeightInfo = ();
    type Currency = Balances;
    type KeyDeposit = ();
}

#[docify::export(aura_config)]
impl pallet_aura::Config for Runtime {
    type AuthorityId = AuraId;
    type DisabledValidators = ();
    type MaxAuthorities = ConstU32<100_000>;
    type AllowMultipleBlocksPerSlot = ConstBool<true>;
    type SlotDuration = ConstU64<{ SLOT_DURATION }>;
}

parameter_types! {
    pub const PotId: PalletId = PalletId(*b"PotStake");
    pub const SessionLength: BlockNumber = 6 * HOURS;
    // StakingAdmin pluralistic body.
    pub const StakingAdminBodyId: BodyId = BodyId::Defense;
}

/// We allow root and the StakingAdmin to execute privileged collator selection operations.
pub type CollatorSelectionUpdateOrigin = EitherOfDiverse<
    EnsureRoot<AccountId>,
    EnsureXcm<IsVoiceOfBody<RelayLocation, StakingAdminBodyId>>,
>;

impl pallet_collator_selection::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type UpdateOrigin = CollatorSelectionUpdateOrigin;
    type PotId = PotId;
    type MaxCandidates = ConstU32<100>;
    type MinEligibleCollators = ConstU32<4>;
    type MaxInvulnerables = ConstU32<20>;
    // should be a multiple of session or things will get inconsistent
    type KickThreshold = Period;
    type ValidatorId = <Self as frame_system::Config>::AccountId;
    type ValidatorIdOf = pallet_collator_selection::IdentityCollator;
    type ValidatorRegistration = Session;
    type WeightInfo = ();
}

parameter_types! {
    pub const SponsorshipMinAmount: Balance = EXISTENTIAL_DEPOSIT;
    pub const SponsorshipMinRemaining: Balance = 5 * EXISTENTIAL_DEPOSIT;
    pub const MaxTotalSponsoredBySponsor: Balance = 1_000 * EXISTENTIAL_DEPOSIT;
    pub const MaxSponsoredAccountsBySponsor: u32 = 64;
    pub const MaxSponsoredAmountCreatedPerBlock: Balance = 100 * EXISTENTIAL_DEPOSIT;
    pub const MaxNewSponsorshipsCreatedPerBlock: u32 = 16;
    pub const MaxSponsorshipLeaseDuration: BlockNumber = 7 * DAYS;
    pub const MaxSponsorshipQueueProcessingPerBlock: u32 = 64;
    pub const SponsorshipStorageDeposit: Balance = EXISTENTIAL_DEPOSIT;
}

impl pallet_existential_sponsorship::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = Balances;
    type RuntimeHoldReason = RuntimeHoldReason;
    type WeightInfo = pallet_existential_sponsorship::weights::SubstrateWeight<Runtime>;
    type MinSponsorship = SponsorshipMinAmount;
    type SponsorMinRemaining = SponsorshipMinRemaining;
    type MaxTotalSponsoredPerSponsor = MaxTotalSponsoredBySponsor;
    type MaxSponsoredAccountsPerSponsor = MaxSponsoredAccountsBySponsor;
    type MaxSponsoredAmountPerBlock = MaxSponsoredAmountCreatedPerBlock;
    type MaxNewSponsorshipsPerBlock = MaxNewSponsorshipsCreatedPerBlock;
    type MaxLeaseDuration = MaxSponsorshipLeaseDuration;
    type MaxQueueProcessingPerBlock = MaxSponsorshipQueueProcessingPerBlock;
    type SponsorshipStorageDeposit = SponsorshipStorageDeposit;
    type ForceOrigin = EnsureRoot<AccountId>;
}

const HYPERBRIDGE_PASEO_PARA_ID: u32 = 4_009;

parameter_types! {
    pub const Coprocessor: Option<StateMachine> =
        Some(StateMachine::Kusama(HYPERBRIDGE_PASEO_PARA_ID));
    pub const HostStateMachine: StateMachine = StateMachine::Kusama(PARACHAIN_ID);
}

#[derive(Default)]
pub struct Router;

impl IsmpRouter for Router {
    fn module_for_id(&self, bytes: Vec<u8>) -> Result<Box<dyn IsmpModule>, anyhow::Error> {
        match bytes.as_slice() {
            id if id == pallet_hyperbridge::PALLET_HYPERBRIDGE_ID => {
                Ok(Box::new(Hyperbridge::default()))
            }
            id if HyperFungibleToken::is_module(id) => Ok(Box::new(HyperFungibleToken::default())),
            id if TokenGateway::is_token_gateway(id) => Ok(Box::new(TokenGateway::default())),
            _ => Err(anyhow!(ismp::Error::ModuleNotFound(bytes))),
        }
    }
}

pub struct IsmpParachainWeightInfo;

impl ismp_parachain::weights::WeightInfo for IsmpParachainWeightInfo {
    fn add_parachain(n: u32) -> Weight {
        Weight::from_parts(25_000_000, 0)
            .saturating_add(Weight::from_parts(15_000_000, 0).saturating_mul(u64::from(n)))
            .saturating_add(
                RocksDbWeight::get().reads_writes(2 + (2 * u64::from(n)), 2 + (2 * u64::from(n))),
            )
    }

    fn remove_parachain(n: u32) -> Weight {
        Weight::from_parts(15_000_000, 0)
            .saturating_add(Weight::from_parts(5_000_000, 0).saturating_mul(u64::from(n)))
            .saturating_add(RocksDbWeight::get().reads_writes(1, u64::from(n)))
    }

    fn update_parachain_consensus() -> Weight {
        Weight::from_parts(25_000_000, 0).saturating_add(RocksDbWeight::get().reads_writes(2, 2))
    }
}

impl pallet_ismp::Config for Runtime {
    type AdminOrigin = EnsureRoot<AccountId>;
    type HostStateMachine = HostStateMachine;
    type TimestampProvider = Timestamp;
    type Currency = Balances;
    type Balance = Balance;
    type Router = Router;
    type Coprocessor = Coprocessor;
    type ConsensusClients = (ismp_parachain::ParachainConsensusClient<Runtime, IsmpParachain>,);
    type OffchainDB = ();
    type FeeHandler = WeightFeeHandler<AccountId, Balances, WeightToFee, IsmpFeesPalletId, true>;
    type MigrationWeightInfo = ();
}

impl ismp_parachain::Config for Runtime {
    type IsmpHost = Ismp;
    type WeightInfo = IsmpParachainWeightInfo;
    type RootOrigin = EnsureRoot<AccountId>;
}

impl pallet_hyperbridge::Config for Runtime {
    type IsmpHost = Ismp;
}

impl pallet_hyper_fungible_token::Config for Runtime {
    type Dispatcher = Hyperbridge;
    type NativeCurrency = Balances;
    type CreateOrigin = EnsureRoot<AccountId>;
    type Assets = Assets;
    type NativeAssetId = NativeAssetId;
    type Decimals = TokenGatewayDecimals;
    type EvmToSubstrate = ();
    type WeightInfo = ();
}

impl pallet_token_gateway::Config for Runtime {
    type Dispatcher = Hyperbridge;
    type NativeCurrency = Balances;
    type AssetAdmin = AssetAdmin;
    type CreateOrigin = EnsureRoot<AccountId>;
    type Assets = Assets;
    type NativeAssetId = NativeAssetId;
    type Decimals = TokenGatewayDecimals;
    type EvmToSubstrate = ();
    type WeightInfo = ();
}
