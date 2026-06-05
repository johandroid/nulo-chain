# Examples

This directory contains source-only examples that exercise runtime features from outside the Rust workspace.

## `zkp-revive`

`zkp-revive` is a Noir and Solidity example for `pallet-revive`. It builds private balance-comparison proofs with Noir/Barretenberg, compiles a Solidity verifier plus `BalanceProofGate` wrapper, and deploys them through `pallet-revive`.

Generated files stay inside `examples/zkp-revive/target/` and are intentionally ignored. Install dependencies and rebuild artifacts locally when needed:

```sh
cd examples/zkp-revive
npm install
npm run build
```

The browser tester for the generated proofs is the root-level `web/` project. It copies only the required files from `examples/zkp-revive/target/` with `npm run sync:artifacts`.
