// Copyright (C) Polytope Labs Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Type definitions for the hyper-fungible-token pallet.

use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, Encode};
use frame_support::pallet_prelude::*;
use ismp::host::StateMachine;
use polkadot_sdk::*;
use sp_core::{ConstU32, H160};

use crate::Config;

/// Local asset ID type alias.
pub type AssetId<T> =
    <<T as Config>::Assets as fungibles::Inspect<<T as frame_system::Config>::AccountId>>::AssetId;

use frame_support::traits::fungibles;

// ABI-compatible Message matching the Solidity HyperFungibleToken.Message struct:
// struct Message { bytes from; bytes to; uint256 amount; bytes data; }
alloy_sol_macro::sol! {
    #![sol(all_derives)]
    struct Message {
        bytes from;
        bytes to;
        uint256 amount;
        bytes data;
    }
}

#[derive(
    Debug, Clone, Encode, Decode, DecodeWithMemTracking, scale_info::TypeInfo, PartialEq, Eq,
)]
pub struct SendParams<AssetId, Balance> {
    /// Local asset ID.
    pub asset_id: AssetId,
    /// Destination state machine.
    pub destination: StateMachine,
    /// Recipient account on the destination chain, up to 32 bytes.
    pub recipient: BoundedVec<u8, ConstU32<32>>,
    /// Amount to send in local denomination.
    pub amount: Balance,
    /// Request timeout in seconds.
    pub timeout: u64,
    /// Relayer fee.
    pub relayer_fee: Balance,
    /// Optional calldata to execute on the destination chain.
    pub call_data: Option<Vec<u8>>,
}

#[derive(
    Debug, Clone, Encode, Decode, DecodeWithMemTracking, scale_info::TypeInfo, PartialEq, Eq,
)]
pub struct ChainConfig {
    /// The HyperFungibleToken/WrappedHyperFungibleToken module ID on this chain.
    pub token_contract: Vec<u8>,
    /// ERC20 decimals on this chain.
    pub decimals: u8,
}

#[derive(
    Debug, Clone, Encode, Decode, DecodeWithMemTracking, scale_info::TypeInfo, PartialEq, Eq,
)]
pub struct TokenRegistration<AssetId> {
    /// Local asset ID, which must already exist in the runtime's asset registry.
    pub local_id: AssetId,
    /// Whether this asset is native to this chain (custody) or non-native (mint/burn).
    pub native: bool,
    /// Per-chain configuration.
    pub chains: BTreeMap<StateMachine, ChainConfig>,
}

#[derive(
    Debug, Clone, Encode, Decode, DecodeWithMemTracking, scale_info::TypeInfo, PartialEq, Eq,
)]
pub struct TokenUpdate<AssetId> {
    /// Local asset ID.
    pub asset_id: AssetId,
    /// Chains to add or update.
    pub add_chains: BTreeMap<StateMachine, ChainConfig>,
    /// Chains to remove.
    pub remove_chains: Vec<StateMachine>,
}

#[derive(Debug, Clone, Encode, Decode, scale_info::TypeInfo, PartialEq, Eq)]
pub struct SubstrateCalldata {
    /// Optional SCALE-encoded MultiSignature over the account nonce and runtime call.
    pub signature: Option<Vec<u8>>,
    /// SCALE-encoded runtime call to execute.
    pub runtime_call: Vec<u8>,
}

/// Converts an EVM address to a Substrate AccountId.
pub trait EvmToSubstrate<T: frame_system::Config> {
    fn convert(addr: H160) -> T::AccountId;
}

impl<T: frame_system::Config> EvmToSubstrate<T> for ()
where
    <T as frame_system::Config>::AccountId: From<[u8; 32]>,
{
    fn convert(addr: H160) -> <T as frame_system::Config>::AccountId {
        let mut account = [0u8; 32];
        account[12..].copy_from_slice(&addr.0);
        account.into()
    }
}
