use crate::{
    impls::{convert_to_balance, convert_to_erc20},
    types::{ChainConfig, SendParams, TokenRegistration},
    Config, Message, Pallet, TokenContracts,
};
use alloy_primitives::U256 as AlloyU256;
use alloy_sol_types::SolValue;
use codec::Encode;
use frame::testing_prelude::*;
use frame_support::pallet_prelude::DispatchResultWithPostInfo;
use frame_support::{derive_impl, parameter_types};
use frame_system::EnsureRootWithSuccess;
use ismp::{
    consensus::{ConsensusClient, ConsensusClientId, ConsensusStateId, VerifiedCommitments},
    dispatcher::{DispatchRequest, FeeMetadata, IsmpDispatcher},
    error::Error as IsmpError,
    host::{IsmpHost, StateMachine},
    module::IsmpModule,
    router::{PostRequest, PostResponse, Request, Timeout},
};
use pallet_ismp::fee_handler::FeeHandler;
use polkadot_sdk::*;
use sp_core::H256;
use sp_runtime::{traits::IdentityLookup, AccountId32};

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
    #[runtime::pallet_index(1)]
    pub type Timestamp = pallet_timestamp;
    #[runtime::pallet_index(10)]
    pub type Balances = pallet_balances;
    #[runtime::pallet_index(11)]
    pub type Assets = pallet_assets;
    #[runtime::pallet_index(12)]
    pub type Ismp = pallet_ismp;
    #[runtime::pallet_index(13)]
    pub type HyperFungibleToken = crate;
}

const ALICE: AccountId32 = AccountId32::new([1; 32]);
const BENEFICIARY: AccountId32 = AccountId32::new([2; 32]);
const RELAYER_FEE: u128 = 7;
const LOCAL_ASSET: u32 = 0;
const REMOTE_CONTRACT: &[u8] = b"remote_hft";

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type AccountId = AccountId32;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Block = MockBlock<Test>;
    type AccountData = pallet_balances::AccountData<u128>;
}

#[derive_impl(pallet_timestamp::config_preludes::TestDefaultConfig)]
impl pallet_timestamp::Config for Test {}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
    type Balance = u128;
    type AccountStore = System;
}

parameter_types! {
    pub const AssetDeposit: u128 = 0;
    pub const AssetAccountDeposit: u128 = 0;
    pub const MetadataDepositBase: u128 = 0;
    pub const MetadataDepositPerByte: u128 = 0;
    pub const ApprovalDeposit: u128 = 0;
    pub const AssetsStringLimit: u32 = 50;
    pub const RemoveItemsLimit: u32 = 1_000;
}

impl pallet_assets::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type Balance = u128;
    type RemoveItemsLimit = RemoveItemsLimit;
    type AssetId = u32;
    type AssetIdParameter = u32;
    type ReserveData = ();
    type Currency = Balances;
    type CreateOrigin = EnsureRootWithSuccess<AccountId32, AssetAdmin>;
    type ForceOrigin = frame_system::EnsureRoot<AccountId32>;
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
    type WeightInfo = ();
}

pub struct AssetAdmin;

impl Get<AccountId32> for AssetAdmin {
    fn get() -> AccountId32 {
        AccountId32::new([9; 32])
    }
}

impl TypedGet for AssetAdmin {
    type Type = AccountId32;

    fn get() -> Self::Type {
        <Self as Get<AccountId32>>::get()
    }
}

parameter_types! {
    pub const NativeAssetId: u32 = LOCAL_ASSET;
    pub const Decimals: u8 = 12;
    pub const HostStateMachine: StateMachine = StateMachine::Kusama(5153);
    pub const Coprocessor: Option<StateMachine> = None;
}

#[derive(Default)]
pub struct TestDispatcher;

impl IsmpDispatcher for TestDispatcher {
    type Account = AccountId32;
    type Balance = u128;

    fn dispatch_request(
        &self,
        request: DispatchRequest,
        fee: FeeMetadata<Self::Account, Self::Balance>,
    ) -> Result<H256, anyhow::Error> {
        match request {
            DispatchRequest::Post(post) => {
                assert_eq!(post.from, crate::PALLET_ID.to_bytes());
                assert_eq!(post.to, REMOTE_CONTRACT);
            }
            DispatchRequest::Get(_) => panic!("HFT should dispatch POST requests"),
        }
        assert_eq!(fee.payer, ALICE);
        assert_eq!(fee.fee, RELAYER_FEE);
        Ok(H256::repeat_byte(42))
    }

    fn dispatch_response(
        &self,
        _response: PostResponse,
        _fee: FeeMetadata<Self::Account, Self::Balance>,
    ) -> Result<H256, anyhow::Error> {
        panic!("HFT should not dispatch responses")
    }
}

#[derive(Default)]
pub struct DummyConsensusClient;

impl ConsensusClient for DummyConsensusClient {
    fn verify_consensus(
        &self,
        _host: &dyn IsmpHost,
        _consensus_state_id: ConsensusStateId,
        _trusted_consensus_state: Vec<u8>,
        _proof: Vec<u8>,
    ) -> Result<(Vec<u8>, VerifiedCommitments), IsmpError> {
        Err(IsmpError::Custom("unused in HFT tests".into()))
    }

    fn verify_fraud_proof(
        &self,
        _host: &dyn IsmpHost,
        _trusted_consensus_state: Vec<u8>,
        _proof_1: Vec<u8>,
        _proof_2: Vec<u8>,
    ) -> Result<(), IsmpError> {
        Err(IsmpError::Custom("unused in HFT tests".into()))
    }

    fn consensus_client_id(&self) -> ConsensusClientId {
        *b"DUMY"
    }

    fn state_machine(
        &self,
        _id: StateMachine,
    ) -> Result<Box<dyn ismp::consensus::StateMachineClient>, IsmpError> {
        Err(IsmpError::Custom("unused in HFT tests".into()))
    }
}

pub struct TestFeeHandler;

impl FeeHandler for TestFeeHandler {
    fn on_executed(
        _messages: Vec<ismp::messaging::MessageWithWeight>,
        _events: Vec<ismp::events::Event>,
    ) -> DispatchResultWithPostInfo {
        Ok(().into())
    }
}

#[derive(Default)]
pub struct TestRouter;

impl ismp::router::IsmpRouter for TestRouter {
    fn module_for_id(&self, _bytes: Vec<u8>) -> Result<Box<dyn IsmpModule>, anyhow::Error> {
        Ok(Box::new(HyperFungibleToken::default()))
    }
}

impl pallet_ismp::Config for Test {
    type AdminOrigin = frame_system::EnsureRoot<AccountId32>;
    type TimestampProvider = Timestamp;
    type Balance = u128;
    type Currency = Balances;
    type HostStateMachine = HostStateMachine;
    type Coprocessor = Coprocessor;
    type Router = TestRouter;
    type ConsensusClients = (DummyConsensusClient,);
    type FeeHandler = TestFeeHandler;
    type OffchainDB = ();
    type MigrationWeightInfo = ();
}

impl Config for Test {
    type Dispatcher = TestDispatcher;
    type NativeCurrency = Balances;
    type CreateOrigin = frame_system::EnsureRoot<AccountId32>;
    type Assets = Assets;
    type NativeAssetId = NativeAssetId;
    type Decimals = Decimals;
    type EvmToSubstrate = ();
    type WeightInfo = ();
}

fn new_test_ext() -> TestExternalities {
    let mut storage = frame_system::GenesisConfig::<Test>::default()
        .build_storage()
        .unwrap();

    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(ALICE, 1_000), (Pallet::<Test>::pallet_account(), 500)],
        ..Default::default()
    }
    .assimilate_storage(&mut storage)
    .unwrap();

    let mut ext: TestExternalities = storage.into();
    ext.execute_with(|| System::set_block_number(1));
    ext
}

fn remote_chain() -> StateMachine {
    StateMachine::Evm(1)
}

fn register_native_token() {
    let registration = TokenRegistration {
        local_id: LOCAL_ASSET,
        native: true,
        chains: [(
            remote_chain(),
            ChainConfig {
                token_contract: REMOTE_CONTRACT.to_vec(),
                decimals: 18,
            },
        )]
        .into_iter()
        .collect(),
    };
    assert_ok!(HyperFungibleToken::register_token(
        RuntimeOrigin::root(),
        registration
    ));
}

#[test]
fn conversion_helpers_scale_between_substrate_and_erc20_decimals() {
    let erc20_amount = convert_to_erc20(1_500_000_000_000, 18, 12);
    assert_eq!(
        erc20_amount,
        sp_core::U256::from(1_500_000_000_000_000_000u128)
    );

    let local_amount = convert_to_balance::<u128>(erc20_amount, 18, 12).unwrap();
    assert_eq!(local_amount, 1_500_000_000_000);
}

#[test]
fn register_token_configures_ismp_contract_routes() {
    new_test_ext().execute_with(|| {
        register_native_token();

        assert_eq!(
            TokenContracts::<Test>::get(remote_chain(), LOCAL_ASSET),
            Some(REMOTE_CONTRACT.to_vec())
        );
        assert_eq!(
            crate::ContractToAsset::<Test>::get(remote_chain(), REMOTE_CONTRACT),
            Some(LOCAL_ASSET)
        );
        assert_eq!(
            crate::Precisions::<Test>::get(LOCAL_ASSET, remote_chain()),
            Some(18)
        );
    });
}

#[test]
fn send_native_token_locks_funds_and_dispatches_ismp_post() {
    new_test_ext().execute_with(|| {
        register_native_token();

        let params = SendParams {
            asset_id: LOCAL_ASSET,
            destination: remote_chain(),
            recipient: BENEFICIARY.encode().try_into().unwrap(),
            amount: 100,
            timeout: 1_000,
            relayer_fee: RELAYER_FEE,
            call_data: None,
        };

        assert_ok!(HyperFungibleToken::send(
            RuntimeOrigin::signed(ALICE),
            params
        ));
        assert_eq!(Balances::free_balance(ALICE), 900);
        assert_eq!(
            Balances::free_balance(Pallet::<Test>::pallet_account()),
            600
        );
    });
}

#[test]
fn ismp_accept_releases_native_tokens_to_beneficiary() {
    new_test_ext().execute_with(|| {
        register_native_token();
        let message = Message {
            from: ALICE.encode().into(),
            to: BENEFICIARY.encode().into(),
            amount: AlloyU256::from(25_000_000u128),
            data: Default::default(),
        };
        let request = PostRequest {
            source: remote_chain(),
            dest: HostStateMachine::get(),
            nonce: 1,
            from: REMOTE_CONTRACT.to_vec(),
            to: crate::PALLET_ID.to_bytes(),
            timeout_timestamp: 1_000,
            body: Message::abi_encode(&message),
        };

        assert_ok!(HyperFungibleToken::default().on_accept(request));
        assert_eq!(Balances::free_balance(BENEFICIARY), 25);
        assert_eq!(
            Balances::free_balance(Pallet::<Test>::pallet_account()),
            475
        );
    });
}

#[test]
fn ismp_timeout_refunds_native_tokens_to_original_sender() {
    new_test_ext().execute_with(|| {
        register_native_token();
        let message = Message {
            from: ALICE.encode().into(),
            to: BENEFICIARY.encode().into(),
            amount: AlloyU256::from(10_000_000u128),
            data: Default::default(),
        };
        let request = PostRequest {
            source: HostStateMachine::get(),
            dest: remote_chain(),
            nonce: 1,
            from: crate::PALLET_ID.to_bytes(),
            to: REMOTE_CONTRACT.to_vec(),
            timeout_timestamp: 1_000,
            body: Message::abi_encode(&message),
        };

        assert_ok!(
            HyperFungibleToken::default().on_timeout(Timeout::Request(Request::Post(request)))
        );
        assert_eq!(Balances::free_balance(ALICE), 1_010);
        assert_eq!(
            Balances::free_balance(Pallet::<Test>::pallet_account()),
            490
        );
    });
}
