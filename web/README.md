# ZKP Revive Web Tester

This is the browser tester for `examples/zkp-revive`. It connects to a local Nulo node, uses a Polkadot extension account for signing, and submits prebuilt proof calls with `api.tx.revive.call`.

The web app does not build Noir witnesses, proofs, or Solidity contracts. Build those first in the example project, then sync the generated browser artifacts:

```sh
cd examples/zkp-revive
npm install
npm run build

cd ../../web
npm install
npm run sync:artifacts
npm run dev
```

`public/artifacts/`, `dist/`, and `node_modules/` are generated locally and ignored.
