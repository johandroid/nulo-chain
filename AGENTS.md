# Repository Guidelines

## Workspace Structure

This is a Rust workspace with:
- `runtime/`: parachain runtime (`nulo-chain`)
- `node/`: native collator binary (`nulo-node`)
- `pallets/`: custom FRAME pallets
- Chain specs at repo root (`chain_spec.json`, `dev_chain_spec.json`, `raw_chain_spec.json`)

Use `SKIP_WASM_BUILD=1` to skip WASM builds for local iteration.

## Commands

**Build:**
```sh
cargo build --profile production                      # runtime only
cargo build --workspace --all-features --locked --profile production  # full workspace including node
cargo install --path node --locked                     # install nulo-node
```

**Test:**
```sh
SKIP_WASM_BUILD=1 cargo test                          # full test suite
cargo test -p <pallet>                                # pallet-specific tests
SKIP_PALLET_REVIVE_FIXTURES=1 SKIP_WASM_BUILD=1 cargo clippy --all-targets --all-features --locked --workspace --quiet
```

**Chain specs:** Build runtime first, then regenerate specs after pallet changes.

## CI Commands

Use the exact CI commands for local verification:
- Clippy: `SKIP_PALLET_REVIVE_FIXTURES=1 SKIP_WASM_BUILD=1 cargo clippy --all-targets --all-features --locked --workspace --quiet`
- Tests: `SKIP_WASM_BUILD=1 cargo test`
- Docs: `SKIP_WASM_BUILD=1 cargo doc --workspace --no-deps`

## Lint Configuration

Workspace lints are defined in `Cargo.toml`. Notable allowances:
- `complexity=warn`, `correctness=warn`
- Disabled: `all`, `suspicious_double_ref_op`, `bind_instead_of_map`, `borrowed-box`
- Allow `eq_op` in tests, `default_constructed_unit_structs`, `needless-lifetimes` (generated code)

## Pallet Tests

Run pallet tests with `cargo test -p <pallet>`:
- `pallet-prepaid-gas`
- `pallet-gas-transaction-payment`
- `pallet-existential-sponsorship`
- `pallet-hyper-fungible-token`
- `pallet-ismp-rpc`

## Commit & PR Guidelines

- Use short, imperative subjects: `Fix typo in pallet-name`
- Link issues when creating PRs
- PR descriptions should note runtime/node impact or chain spec changes
- Regenerate chain specs after pallet removals or index changes

## Local Development

```sh
polkadot-omni-node --chain ./chain_spec.json --dev --dev-block-time 1000 --rpc-port 9944
```

Default Omni dev endpoint: `ws://127.0.0.1:9944`

## Workspace Members

```toml
runtime
node
pallets/pallet-prepaid-gas
pallets/pallet-gas-transaction-payment
pallets/pallet-existential-sponsorship
pallets/pallet-hyper-fungible-token
```

## Runtime Wiring

Keep runtime wiring in `runtime/src/lib.rs`. Pallet test scaffolding goes in `mock.rs` or `tests.rs` next to pallets.

## ZKP Revive/Web Example

- `examples/zkp-revive/` is the Noir/Barretenberg Solidity verifier example for `pallet-revive`.
- `web/` is the browser tester. It should submit calls with Polkadot extension accounts through `api.tx.revive.call`.
- Frontend/example work must not modify `runtime/`, `node/`, or pallets unless the user explicitly asks for runtime behavior changes.
- Build proofs and contracts with `cd examples/zkp-revive && source /home/clara/.hyde.zshrc && npm run build`.
- Deploy to the current local node with `REVIVE_REF_TIME=500000000000 WS=ws://127.0.0.1:39944 npm run deploy`.
- Sync browser artifacts with `cd web && npm run sync:artifacts` after rebuilding proofs or contracts.

## Notes

- `pallet-template` removed from runtime and workspace
- `data/` is local node state, not source code
- Para ID: `5153`, Relay chain: `paseo`
