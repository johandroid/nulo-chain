// Copyright (C) Polytope Labs Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Weight information for the hyper-fungible-token pallet.

use polkadot_sdk::sp_runtime::Weight;

pub trait WeightInfo {
    fn send() -> Weight;
    fn register_token(c: u32) -> Weight;
    fn update_token(c: u32) -> Weight;
}

impl WeightInfo for () {
    fn send() -> Weight {
        Weight::from_parts(100_000_000, 0)
    }

    fn register_token(_c: u32) -> Weight {
        Weight::from_parts(100_000_000, 0)
    }

    fn update_token(_c: u32) -> Weight {
        Weight::from_parts(100_000_000, 0)
    }
}
