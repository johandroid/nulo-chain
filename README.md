# Nulo Chain

Nulo Chain is a Polkadot SDK parachain runtime. The default local development path uses `polkadot-omni-node` with parachain id `5153`.

## What It Includes

- A Cumulus parachain runtime in `runtime/`.
- A native collator node in `node/`, built as `nulo-node`.
- XCM support for relay-chain and sibling parachain messaging.
- ISMP/Hyperbridge support through `pallet-ismp`, `ismp-parachain`, `pallet-hyperbridge`, `pallet-token-gateway`, and `pallet-hyper-fungible-token`.
- Prepaid gas support through `pallet-prepaid-gas` and `pallet-gas-transaction-payment`.
- Hold-based existential sponsorship through `pallet-existential-sponsorship`.

## Project Structure

- `runtime/`: parachain runtime wiring, genesis presets, runtime APIs and weights.
- `node/`: optional native collator binary, RPC wiring and chain-spec helpers.
- `pallets/pallet-prepaid-gas/`: buys and tracks prepaid weight credit.
- `pallets/pallet-gas-transaction-payment/`: transaction extension that consumes prepaid gas before falling back to normal transaction fees.
- `pallets/pallet-existential-sponsorship/`: sponsors beneficiary accounts by holding funds with expiration and auto-unlock policies.
- `pallets/pallet-hyper-fungible-token/`: ISMP module for cross-chain fungible token flows.

## Constants

- Parachain id: `5153`.
- Relay chain for local development: `paseo`.
- Native node binary: `nulo-node`.
- Runtime crate: `nulo-chain`.

## Build

Build the runtime:

```sh
cargo build --profile production
```

Build the full workspace, including `nulo-node`:

```sh
cargo build --workspace --all-features --locked --profile production
```

Install the native node locally:

```sh
cargo install --path node --locked
```

## Test

Run the fast local test suite:

```sh
SKIP_WASM_BUILD=1 cargo test
```

Run clippy with the same shape used by CI:

```sh
SKIP_WASM_BUILD=1 cargo clippy --all-targets --all-features --locked --workspace --quiet
```

Run focused pallet tests:

```sh
cargo test -p pallet-prepaid-gas
cargo test -p pallet-gas-transaction-payment
cargo test -p pallet-existential-sponsorship
cargo test -p pallet-hyper-fungible-token
```

## Chain Specs

Build the runtime before generating specs:

```sh
cargo build --profile production
```

Generate a development chain spec for Omni Node:

```sh
chain-spec-builder -c chain_spec.json create -t development \
  --relay-chain paseo \
  --para-id 5153 \
  --runtime ./target/release/wbuild/nulo-chain/nulo_chain.compact.compressed.wasm \
  named-preset development
```

Generate a raw chain spec:

```sh
chain-spec-builder -c raw_chain_spec.json create --raw-storage -t development \
  --relay-chain paseo \
  --para-id 5153 \
  --runtime ./target/release/wbuild/nulo-chain/nulo_chain.compact.compressed.wasm \
  named-preset development
```

## Local Network

Start Omni Node dev mode:

```sh
polkadot-omni-node --chain ./chain_spec.json --dev
```

For faster block production and explicit local ports:

```sh
polkadot-omni-node --chain ./chain_spec.json --dev --dev-block-time 1000 --rpc-port 9944
```

Omni `--dev` runs the parachain runtime directly. It is enough for runtime, pallet, extrinsic, and RPC testing. It does not test relay-chain inclusion, XCM routing through a relay, collator networking, or Agile Coretime.

## RPC

The node exposes standard system and transaction payment RPCs, plus ISMP RPC methods via `pallet-ismp-rpc`.

Default Omni dev endpoint:

```text
ws://127.0.0.1:9944
```

## Docker

Build the image:

```sh
docker build . -t nulo-chain
```

The container entrypoint is `nulo-node`.

## Notes

- `pallet-template` has been removed from the runtime and workspace.
- Existing generated chain specs should be regenerated after runtime changes, especially after pallet removals or pallet index changes.
- `data/` is local node state and should not be treated as source code.

## ZKP Revive Example

The ZKP example lives outside the runtime and node code. It uses Noir and `bb` to prove two private balance comparisons, deploys a Solidity verifier plus `BalanceProofGate` wrapper through `pallet-revive`, and provides a browser tester that signs `revive.call` extrinsics with the Polkadot extension.

Build and deploy the contracts:

```sh
cd examples/zkp-revive
source /home/clara/.hyde.zshrc
npm install
npm run build
REVIVE_REF_TIME=500000000000 WS=ws://127.0.0.1:39944 npm run deploy
```

The deploy script prints the deployed `BalanceProofGate` address. Use that address in the browser tester. On a restarted dev node, redeploy before testing because the old contract addresses disappear with node state.

Run the browser tester:

```sh
cd web
npm install
npm run sync:artifacts
npm run dev
```

Open the Vite URL, connect the local RPC endpoint, connect a Polkadot extension account, paste the `BalanceProofGate` address, then submit either prebuilt proof:

- dominance proof: one hidden token balance is greater than another hidden token balance
- threshold proof: one hidden token balance is greater than a public threshold

The frontend loads ABI and proof artifacts from `web/public/artifacts/`, generated by `npm run sync:artifacts`. It does not generate Noir witnesses or proofs in the browser.
