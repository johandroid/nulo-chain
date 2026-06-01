#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]

use core::marker::PhantomData;
use frame::{deps::frame_support::weights::constants::RocksDbWeight, prelude::*};

pub trait WeightInfo {
	fn sponsor() -> Weight;
	fn sponsor_minimum() -> Weight;
	fn unlock() -> Weight;
	fn set_policy() -> Weight;
	fn force_unlock() -> Weight;
	fn refresh() -> Weight;
	fn on_initialize_process_auto_unlock(items: u32) -> Weight;
	fn on_initialize_process_expiries(items: u32) -> Weight;
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn sponsor() -> Weight {
		Weight::from_parts(45_000_000, 4_000)
			.saturating_add(T::DbWeight::get().reads(5_u64))
			.saturating_add(T::DbWeight::get().writes(6_u64))
	}

	fn sponsor_minimum() -> Weight {
		Self::sponsor()
	}

	fn unlock() -> Weight {
		Weight::from_parts(35_000_000, 3_500)
			.saturating_add(T::DbWeight::get().reads(4_u64))
			.saturating_add(T::DbWeight::get().writes(6_u64))
	}

	fn set_policy() -> Weight {
		Weight::from_parts(20_000_000, 2_000)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(4_u64))
	}

	fn force_unlock() -> Weight {
		Self::unlock()
	}

	fn refresh() -> Weight {
		Weight::from_parts(18_000_000, 2_000)
			.saturating_add(T::DbWeight::get().reads(2_u64))
			.saturating_add(T::DbWeight::get().writes(3_u64))
	}

	fn on_initialize_process_auto_unlock(items: u32) -> Weight {
		Weight::from_parts(7_000_000, 1_000)
			.saturating_mul(items.into())
			.saturating_add(T::DbWeight::get().reads_writes(items.into(), items.into()))
	}

	fn on_initialize_process_expiries(items: u32) -> Weight {
		Weight::from_parts(6_000_000, 1_000)
			.saturating_mul(items.into())
			.saturating_add(T::DbWeight::get().reads_writes(items.into(), items.into()))
	}
}

impl WeightInfo for () {
	fn sponsor() -> Weight {
		Weight::from_parts(45_000_000, 4_000)
			.saturating_add(RocksDbWeight::get().reads(5_u64))
			.saturating_add(RocksDbWeight::get().writes(6_u64))
	}

	fn sponsor_minimum() -> Weight {
		Self::sponsor()
	}

	fn unlock() -> Weight {
		Weight::from_parts(35_000_000, 3_500)
			.saturating_add(RocksDbWeight::get().reads(4_u64))
			.saturating_add(RocksDbWeight::get().writes(6_u64))
	}

	fn set_policy() -> Weight {
		Weight::from_parts(20_000_000, 2_000)
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(RocksDbWeight::get().writes(4_u64))
	}

	fn force_unlock() -> Weight {
		Self::unlock()
	}

	fn refresh() -> Weight {
		Weight::from_parts(18_000_000, 2_000)
			.saturating_add(RocksDbWeight::get().reads(2_u64))
			.saturating_add(RocksDbWeight::get().writes(3_u64))
	}

	fn on_initialize_process_auto_unlock(items: u32) -> Weight {
		Weight::from_parts(7_000_000, 1_000)
			.saturating_mul(items.into())
			.saturating_add(RocksDbWeight::get().reads_writes(items.into(), items.into()))
	}

	fn on_initialize_process_expiries(items: u32) -> Weight {
		Weight::from_parts(6_000_000, 1_000)
			.saturating_mul(items.into())
			.saturating_add(RocksDbWeight::get().reads_writes(items.into(), items.into()))
	}
}
