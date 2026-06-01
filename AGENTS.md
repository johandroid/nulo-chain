# Repository Guidelines

## Project Structure & Module Organization

This repository is a Rust workspace with `runtime/` for parachain logic, custom pallets under `pallets/`, and `node/` for the optional `nulo-node` binary. Runtime code lives in `runtime/src/`, with configuration grouped under `runtime/src/configs/` and generated weights under `runtime/src/weights/`. Chain specs sit at the repo root (`chain_spec.json`, `dev_chain_spec.json`, `raw_chain_spec.json`). Local runtime testing uses `polkadot-omni-node`.

## Build, Test, and Development Commands

Use the same commands the CI pipeline uses:

- `cargo build --profile production` builds the runtime for release-style local work.
- `cargo build --workspace --all-features --locked --profile production` builds every workspace member, including `node/`.
- `SKIP_WASM_BUILD=1 cargo test` runs the fast local test suite.
- `SKIP_WASM_BUILD=1 cargo clippy --all-targets --all-features --locked --workspace --quiet` runs lint checks.
- `cargo install --path node --locked` installs the local node binary.
- `chain-spec-builder -c chain_spec.json create -t development --relay-chain paseo --para-id 5153 --runtime ./target/release/wbuild/nulo-chain/nulo_chain.compact.compressed.wasm named-preset development` generates the local Omni Node chain spec.
- `polkadot-omni-node --chain ./chain_spec.json --dev --dev-block-time 1000` starts the local runtime test node.

## Coding Style & Naming Conventions

The workspace uses Rust 2024 edition and workspace-level lints defined in [`Cargo.toml`](/home/clara/Documents/nulo/sw/nulo-chain/Cargo.toml). Format with `cargo fmt` before opening a PR. Follow standard Rust naming: modules and files in `snake_case`, types and traits in `UpperCamelCase`, constants in `ALL_CAPS`, and crates in `kebab-case` such as `nulo-chain` and `pallet-prepaid-gas`. Keep runtime wiring in `runtime/src/lib.rs`, and place pallet test scaffolding next to pallet code in `mock.rs` and `tests.rs`.

## Testing Guidelines

Unit tests use Rust `#[test]` plus FRAME test helpers from `frame::testing_prelude`. Prefer behavior-focused test names. Run focused pallet tests with commands such as `cargo test -p pallet-prepaid-gas`, `cargo test -p pallet-gas-transaction-payment`, `cargo test -p pallet-existential-sponsorship`, or `cargo test -p pallet-hyper-fungible-token`. There is no explicit coverage gate in CI, but runtime and chain-spec changes should include an Omni Node dev run.

## Commit & Pull Request Guidelines

Recent history uses short, imperative commit subjects (`Fixing typos and id`, `First approach to include hyperbridge`). Keep subjects brief, specific, and action-oriented. PRs should summarize runtime or node impact, link the relevant issue when one exists, and list the commands you ran locally. If a change affects chain specs, parachain ID assumptions, or local network behavior, call that out explicitly in the PR description.
