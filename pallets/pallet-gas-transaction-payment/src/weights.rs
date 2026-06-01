#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use core::marker::PhantomData;
use frame::{deps::frame_support::weights::constants::RocksDbWeight, prelude::*};

pub trait WeightInfo {
	fn charge_transaction_payment() -> Weight;
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn charge_transaction_payment() -> Weight {
		Weight::from_parts(5_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(1_u64))
	}
}

impl WeightInfo for () {
	fn charge_transaction_payment() -> Weight {
		Weight::from_parts(5_000_000, 0)
			.saturating_add(RocksDbWeight::get().reads(1_u64))
	}
}
