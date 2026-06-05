import { ApiPromise, WsProvider } from '@polkadot/api';
import { web3Accounts, web3Enable, web3FromAddress } from '@polkadot/extension-dapp';
import { Interface } from 'ethers';
import './styles.css';

const APP_NAME = 'Nulo ZKP Revive Tester';
const DEFAULT_ENDPOINT = 'ws://127.0.0.1:39944';
const DEFAULT_CONTRACT = '0x14fe614f497b1cd2c13721fe671015895dfd385a';
const DEFAULT_REF_TIME = '500000000000';
const DEFAULT_PROOF_SIZE = '3000000';
const DEFAULT_STORAGE_DEPOSIT = '100000000000000000';

const state = {
  api: undefined,
  account: undefined,
  accounts: [],
  artifacts: undefined,
  iface: undefined,
  pending: false
};

const app = document.querySelector('#app');

app.innerHTML = `
  <main class="shell">
    <header class="header">
      <div>
        <p class="eyebrow">pallet-revive</p>
        <h1>Nulo ZKP Tester</h1>
      </div>
      <span id="chainStatus" class="status">offline</span>
    </header>

    <section class="layout">
      <form id="settingsForm" class="panel">
        <h2>Connection</h2>
        <label>
          RPC endpoint
          <input id="endpoint" name="endpoint" value="${localStorage.getItem('zkp.endpoint') ?? DEFAULT_ENDPOINT}" autocomplete="off" />
        </label>
        <label>
          BalanceProofGate address
          <input id="contract" name="contract" value="${localStorage.getItem('zkp.contract') ?? DEFAULT_CONTRACT}" autocomplete="off" />
        </label>
        <div class="weights">
          <label>
            refTime
            <input id="refTime" name="refTime" value="${localStorage.getItem('zkp.refTime') ?? DEFAULT_REF_TIME}" inputmode="numeric" />
          </label>
          <label>
            proofSize
            <input id="proofSize" name="proofSize" value="${localStorage.getItem('zkp.proofSize') ?? DEFAULT_PROOF_SIZE}" inputmode="numeric" />
          </label>
        </div>
        <label>
          storage deposit limit
          <input id="storageDeposit" name="storageDeposit" value="${localStorage.getItem('zkp.storageDeposit') ?? DEFAULT_STORAGE_DEPOSIT}" inputmode="numeric" />
        </label>
        <div class="actions">
          <button id="connectChain" type="button">Connect chain</button>
          <button id="connectWallet" type="button" class="secondary">Connect extension</button>
        </div>
      </form>

      <section class="panel">
        <h2>Signer</h2>
        <label>
          Extension account
          <select id="accountSelect" disabled>
            <option>No account connected</option>
          </select>
        </label>
        <dl class="facts">
          <div>
            <dt>Artifact status</dt>
            <dd id="artifactStatus">loading</dd>
          </div>
          <div>
            <dt>Selected account</dt>
            <dd id="selectedAccount">none</dd>
          </div>
        </dl>
      </section>
    </section>

    <section class="proofGrid">
      <article class="proofPanel">
        <div>
          <p class="eyebrow">hidden balance vs hidden balance</p>
          <h2>Dominance Proof</h2>
        </div>
        <dl id="dominanceSummary" class="proofFacts"></dl>
        <button id="submitDominance" type="button">Submit dominance proof</button>
      </article>

      <article class="proofPanel">
        <div>
          <p class="eyebrow">hidden balance vs public threshold</p>
          <h2>Threshold Proof</h2>
        </div>
        <dl id="thresholdSummary" class="proofFacts"></dl>
        <button id="submitThreshold" type="button">Submit threshold proof</button>
      </article>
    </section>

    <section class="panel logPanel">
      <div class="logHeader">
        <h2>Activity</h2>
        <button id="clearLog" type="button" class="ghost">Clear</button>
      </div>
      <pre id="log"></pre>
    </section>
  </main>
`;

const elements = {
  chainStatus: document.querySelector('#chainStatus'),
  endpoint: document.querySelector('#endpoint'),
  contract: document.querySelector('#contract'),
  refTime: document.querySelector('#refTime'),
  proofSize: document.querySelector('#proofSize'),
  storageDeposit: document.querySelector('#storageDeposit'),
  connectChain: document.querySelector('#connectChain'),
  connectWallet: document.querySelector('#connectWallet'),
  accountSelect: document.querySelector('#accountSelect'),
  artifactStatus: document.querySelector('#artifactStatus'),
  selectedAccount: document.querySelector('#selectedAccount'),
  dominanceSummary: document.querySelector('#dominanceSummary'),
  thresholdSummary: document.querySelector('#thresholdSummary'),
  submitDominance: document.querySelector('#submitDominance'),
  submitThreshold: document.querySelector('#submitThreshold'),
  clearLog: document.querySelector('#clearLog'),
  log: document.querySelector('#log')
};

function log(message, detail) {
  const suffix = detail === undefined ? '' : `\n${typeof detail === 'string' ? detail : JSON.stringify(detail, null, 2)}`;
  elements.log.textContent = `[${new Date().toLocaleTimeString()}] ${message}${suffix}\n${elements.log.textContent}`;
}

function setPending(pending) {
  state.pending = pending;
  for (const button of [elements.connectChain, elements.connectWallet, elements.submitDominance, elements.submitThreshold]) {
    button.disabled = pending;
  }
}

function weightLimit() {
  return {
    refTime: elements.refTime.value.trim(),
    proofSize: elements.proofSize.value.trim()
  };
}

function dispatchErrorToString(dispatchError) {
  if (dispatchError.isModule) {
    const decoded = state.api.registry.findMetaError(dispatchError.asModule);
    return `${decoded.section}.${decoded.name}: ${decoded.docs.join(' ')}`;
  }

  return dispatchError.toString();
}

function submitTx(tx, address, label, options = {}) {
  return new Promise(async (resolve, reject) => {
    const injector = await web3FromAddress(address);
    let unsubscribe;

    tx.signAndSend(address, { signer: injector.signer }, (result) => {
      if (result.dispatchError) {
        const message = dispatchErrorToString(result.dispatchError);
        if (options.acceptDispatchError?.(message)) {
          unsubscribe?.();
          resolve(result);
          return;
        }

        unsubscribe?.();
        reject(new Error(message));
        return;
      }

      if (result.status.isInBlock || result.status.isFinalized) {
        unsubscribe?.();
        resolve(result);
      }
    }).then((unsub) => {
      unsubscribe = unsub;
    }).catch((error) => {
      reject(new Error(`${label}: ${error.message}`));
    });
  });
}

function bytesToHex(bytes) {
  return `0x${[...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('')}`;
}

async function fetchBytes(path) {
  const response = await fetch(path);
  if (!response.ok) {
    throw new Error(`Unable to load ${path}`);
  }

  return new Uint8Array(await response.arrayBuffer());
}

function splitPublicInputs(bytes) {
  if (bytes.length % 32 !== 0) {
    throw new Error('public_inputs length is not 32-byte aligned');
  }

  const inputs = [];
  for (let offset = 0; offset < bytes.length; offset += 32) {
    inputs.push(bytesToHex(bytes.subarray(offset, offset + 32)));
  }

  return inputs;
}

function shortHex(value) {
  return `${value.slice(0, 10)}...${value.slice(-8)}`;
}

function renderSummary(target, inputs) {
  const rows = [
    ['proof kind', BigInt(inputs[0]).toString()],
    ['threshold', BigInt(inputs[1]).toString()],
    ['account commitment', shortHex(inputs[2])],
    ['other commitment', shortHex(inputs[3])]
  ];

  target.innerHTML = rows.map(([label, value]) => `<div><dt>${label}</dt><dd>${value}</dd></div>`).join('');
}

async function loadArtifacts() {
  const [buildResponse, dominanceProof, dominanceInputsBytes, thresholdProof, thresholdInputsBytes] = await Promise.all([
    fetch('/artifacts/build.json'),
    fetchBytes('/artifacts/proofs/dominance/proof'),
    fetchBytes('/artifacts/proofs/dominance/public_inputs'),
    fetchBytes('/artifacts/proofs/threshold/proof'),
    fetchBytes('/artifacts/proofs/threshold/public_inputs')
  ]);

  if (!buildResponse.ok) {
    throw new Error('Missing /artifacts/build.json. Run npm run sync:artifacts.');
  }

  const build = await buildResponse.json();
  state.iface = new Interface(build.wrapper.abi);
  state.artifacts = {
    dominance: {
      proof: bytesToHex(dominanceProof),
      publicInputs: splitPublicInputs(dominanceInputsBytes)
    },
    threshold: {
      proof: bytesToHex(thresholdProof),
      publicInputs: splitPublicInputs(thresholdInputsBytes)
    }
  };

  renderSummary(elements.dominanceSummary, state.artifacts.dominance.publicInputs);
  renderSummary(elements.thresholdSummary, state.artifacts.threshold.publicInputs);
  elements.artifactStatus.textContent = 'ready';
  log('Artifacts loaded');
}

async function connectChain() {
  setPending(true);
  try {
    await state.api?.disconnect();

    const endpoint = elements.endpoint.value.trim();
    localStorage.setItem('zkp.endpoint', endpoint);
    localStorage.setItem('zkp.contract', elements.contract.value.trim());
    localStorage.setItem('zkp.refTime', elements.refTime.value.trim());
    localStorage.setItem('zkp.proofSize', elements.proofSize.value.trim());
    localStorage.setItem('zkp.storageDeposit', elements.storageDeposit.value.trim());

    state.api = await ApiPromise.create({ provider: new WsProvider(endpoint) });
    elements.chainStatus.textContent = 'online';
    elements.chainStatus.classList.add('online');
    log(`Connected to ${endpoint}`);
  } catch (error) {
    elements.chainStatus.textContent = 'offline';
    elements.chainStatus.classList.remove('online');
    log(`Chain connection failed: ${error.message}`);
  } finally {
    setPending(false);
  }
}

function renderAccounts() {
  elements.accountSelect.disabled = state.accounts.length === 0;
  elements.accountSelect.innerHTML = state.accounts
    .map((account, index) => `<option value="${index}">${account.meta.name ?? 'Account'} - ${account.address}</option>`)
    .join('');

  state.account = state.accounts[0];
  elements.selectedAccount.textContent = state.account?.address ?? 'none';
}

async function connectWallet() {
  setPending(true);
  try {
    const extensions = await web3Enable(APP_NAME);
    if (extensions.length === 0) {
      throw new Error('No extension approved the connection');
    }

    state.accounts = await web3Accounts();
    if (state.accounts.length === 0) {
      throw new Error('No extension accounts found');
    }

    renderAccounts();
    log(`Loaded ${state.accounts.length} extension account(s)`);
  } catch (error) {
    log(`Extension connection failed: ${error.message}`);
  } finally {
    setPending(false);
  }
}

function decodeReviveEvents(result) {
  const decoded = [];

  for (const { event } of result.events) {
    if (event.section !== 'revive') {
      continue;
    }

    decoded.push({
      method: `${event.section}.${event.method}`,
      data: event.data.toHuman()
    });
  }

  return decoded;
}

async function ensureMappedAccount() {
  if (!state.api.tx.revive.mapAccount) {
    return;
  }

  await submitTx(state.api.tx.revive.mapAccount(), state.account.address, 'map account', {
    acceptDispatchError: (message) => message.includes('AccountAlreadyMapped')
  });
}

async function submitProof(kind) {
  setPending(true);
  try {
    if (!state.api) {
      throw new Error('Connect the chain first');
    }
    if (!state.account) {
      throw new Error('Connect the Polkadot extension first');
    }
    if (!state.artifacts || !state.iface) {
      throw new Error('Artifacts are not loaded');
    }

    const contract = elements.contract.value.trim();
    const artifact = state.artifacts[kind];
    const method = kind === 'dominance' ? 'submitDominanceProof' : 'submitThresholdProof';
    const data = state.iface.encodeFunctionData(method, [artifact.proof, artifact.publicInputs]);

    await ensureMappedAccount();

    const tx = state.api.tx.revive.call(
      contract,
      0,
      weightLimit(),
      elements.storageDeposit.value.trim(),
      data
    );
    const result = await submitTx(tx, state.account.address, method);
    log(`${method} included`, decodeReviveEvents(result));
  } catch (error) {
    log(`${kind} submission failed: ${error.message}`);
  } finally {
    setPending(false);
  }
}

elements.connectChain.addEventListener('click', connectChain);
elements.connectWallet.addEventListener('click', connectWallet);
elements.submitDominance.addEventListener('click', () => submitProof('dominance'));
elements.submitThreshold.addEventListener('click', () => submitProof('threshold'));
elements.clearLog.addEventListener('click', () => {
  elements.log.textContent = '';
});
elements.accountSelect.addEventListener('change', () => {
  state.account = state.accounts[Number(elements.accountSelect.value)];
  elements.selectedAccount.textContent = state.account?.address ?? 'none';
});

loadArtifacts().catch((error) => {
  elements.artifactStatus.textContent = 'missing';
  log(error.message);
});
