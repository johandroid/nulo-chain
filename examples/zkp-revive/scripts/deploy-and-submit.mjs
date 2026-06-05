#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { ApiPromise, WsProvider } from '@polkadot/api';
import { Keyring } from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { AbiCoder, Interface } from 'ethers';
import { readHexFile, readJson, targetDir } from './common.mjs';

const WS = process.env.WS ?? 'ws://127.0.0.1:9944';
const SURI = process.env.SURI ?? '//Alice';
const REVIVE_REF_TIME = process.env.REVIVE_REF_TIME ?? '1900000000000';
const REVIVE_PROOF_SIZE = process.env.REVIVE_PROOF_SIZE ?? '3000000';
const REVIVE_STORAGE_DEPOSIT = process.env.REVIVE_STORAGE_DEPOSIT ?? '100000000000000000';
const shouldSubmitInvalid = process.argv.includes('--invalid');

const weightLimit = {
  refTime: REVIVE_REF_TIME,
  proofSize: REVIVE_PROOF_SIZE
};

function dispatchErrorToString(api, dispatchError) {
  if (dispatchError.isModule) {
    const decoded = api.registry.findMetaError(dispatchError.asModule);
    return `${decoded.section}.${decoded.name}: ${decoded.docs.join(' ')}`;
  }

  return dispatchError.toString();
}

function submitTx(api, tx, signer, label, options = {}) {
  return new Promise((resolve, reject) => {
    let unsub;

    tx.signAndSend(signer, (result) => {
      if (result.dispatchError) {
        const message = dispatchErrorToString(api, result.dispatchError);
        if (options.acceptDispatchError?.(message)) {
          console.log(`${label}: accepted dispatch error ${message}`);
          unsub?.();
          resolve(result);
          return;
        }

        unsub?.();
        reject(new Error(`${label} failed: ${message}`));
        return;
      }

      if (result.status.isInBlock || result.status.isFinalized) {
        const blockHash = result.status.isInBlock
          ? result.status.asInBlock.toHex()
          : result.status.asFinalized.toHex();
        console.log(`${label}: ${result.status.type} ${blockHash}`);
        unsub?.();
        resolve(result);
      }
    }).then((unsubscribe) => {
      unsub = unsubscribe;
    }).catch(reject);
  });
}

function collectH160(value, addresses = []) {
  if (!value) {
    return addresses;
  }

  if (typeof value === 'string' && /^0x[0-9a-fA-F]{40}$/.test(value)) {
    addresses.push(value);
    return addresses;
  }

  if (Array.isArray(value)) {
    for (const item of value) {
      collectH160(item, addresses);
    }
  }

  if (typeof value === 'object') {
    for (const item of Object.values(value)) {
      collectH160(item, addresses);
    }
  }

  return addresses;
}

function extractContractAddress(result) {
  for (const { event } of result.events) {
    if (event.section !== 'revive') {
      continue;
    }

    const addresses = collectH160(event.data.toJSON());
    if (addresses.length > 0) {
      return addresses[addresses.length - 1];
    }
  }

  throw new Error('Could not find deployed contract address in revive events');
}

function linkBytecode(bytecode, libraries) {
  let linked = bytecode.startsWith('0x') ? bytecode.slice(2) : bytecode;

  for (const library of libraries) {
    if (!library.address) {
      throw new Error(`Missing deployed address for library ${library.contractName}`);
    }

    const placeholder = library.placeholder.replace(/^0x/, '');
    const replacement = library.address.replace(/^0x/, '');
    linked = linked.split(placeholder).join(replacement);
  }

  if (linked.includes('__$')) {
    throw new Error('Verifier bytecode still contains unresolved Solidity library placeholders');
  }

  return `0x${linked}`;
}

function readPublicInputs(name) {
  const bytes = readFileSync(join(targetDir, 'proofs', name, 'public_inputs'));
  if (bytes.length % 32 !== 0) {
    throw new Error(`${name} public inputs are not 32-byte aligned`);
  }

  const inputs = [];
  for (let offset = 0; offset < bytes.length; offset += 32) {
    inputs.push(`0x${bytes.subarray(offset, offset + 32).toString('hex')}`);
  }

  return inputs;
}

function readProof(name) {
  return readHexFile(join(targetDir, 'proofs', name, 'proof'));
}

async function deployCreationCode(api, signer, label, creationCode) {
  const salt = `0x${Buffer.from(`${label}-${Date.now()}`).toString('hex').padEnd(64, '0').slice(0, 64)}`;
  const tx = api.tx.revive.instantiateWithCode(0, weightLimit, REVIVE_STORAGE_DEPOSIT, creationCode, '0x', salt);
  const result = await submitTx(api, tx, signer, `deploy ${label}`);
  const address = extractContractAddress(result);
  console.log(`${label}: ${address}`);
  return address;
}

async function callWrapper(api, signer, wrapperAddress, iface, method, proof, publicInputs) {
  const data = iface.encodeFunctionData(method, [proof, publicInputs]);
  const tx = api.tx.revive.call(wrapperAddress, 0, weightLimit, REVIVE_STORAGE_DEPOSIT, data);
  return submitTx(api, tx, signer, method);
}

try {
  await cryptoWaitReady();

  const build = readJson(join(targetDir, 'build.json'));
  const provider = new WsProvider(WS);
  const api = await ApiPromise.create({ provider });
  const keyring = new Keyring({ type: 'sr25519' });
  const signer = keyring.addFromUri(SURI);

  console.log(`Connected: ${WS}`);
  console.log(`Signer: ${signer.address}`);

  if (api.tx.revive.mapAccount) {
    await submitTx(api, api.tx.revive.mapAccount(), signer, 'map account', {
      acceptDispatchError: (message) => message.includes('AccountAlreadyMapped')
    });
  }

  for (const library of build.libraries ?? []) {
    library.address = await deployCreationCode(api, signer, library.contractName, library.bytecode);
  }

  const verifierBytecode = linkBytecode(build.verifier.bytecode, build.libraries ?? []);
  const verifierAddress = await deployCreationCode(api, signer, build.verifier.contractName, verifierBytecode);

  const constructorArgs = AbiCoder.defaultAbiCoder().encode(['address'], [verifierAddress]).slice(2);
  const wrapperAddress = await deployCreationCode(
    api,
    signer,
    build.wrapper.contractName,
    `${build.wrapper.bytecode}${constructorArgs}`
  );

  const iface = new Interface(build.wrapper.abi);
  const dominanceInputs = readPublicInputs('dominance');
  const thresholdInputs = readPublicInputs('threshold');

  await callWrapper(api, signer, wrapperAddress, iface, 'submitDominanceProof', readProof('dominance'), dominanceInputs);

  if (shouldSubmitInvalid) {
    const tampered = [...thresholdInputs];
    tampered[1] = `0x${(BigInt(tampered[1]) + 1n).toString(16).padStart(64, '0')}`;

    try {
      await callWrapper(api, signer, wrapperAddress, iface, 'submitThresholdProof', readProof('threshold'), tampered);
      throw new Error('Invalid threshold proof was accepted');
    } catch (error) {
      if (!String(error.message).includes('ContractReverted')) {
        throw error;
      }

      console.log('Invalid threshold proof rejected with revive.ContractReverted');
    }
  } else {
    await callWrapper(api, signer, wrapperAddress, iface, 'submitThresholdProof', readProof('threshold'), thresholdInputs);
  }

  console.log('\nDeployment summary');
  for (const library of build.libraries ?? []) {
    console.log(`${library.contractName}: ${library.address}`);
  }
  console.log(`${build.verifier.contractName}: ${verifierAddress}`);
  console.log(`${build.wrapper.contractName}: ${wrapperAddress}`);
  console.log('\nUse the BalanceProofGate address in ../../web.');

  await api.disconnect();
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
