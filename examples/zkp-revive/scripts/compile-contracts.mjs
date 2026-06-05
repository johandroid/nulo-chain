import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import {
  asHex,
  ensureDir,
  findAbiFile,
  findBinFile,
  listFiles,
  readJson,
  requireCommand,
  run,
  solcDir,
  targetDir,
  writeJson
} from './common.mjs';

try {
  requireCommand('solc', 'Install the Solidity compiler, then rerun `npm run build:contracts`.');

  const verifierSol = join(targetDir, 'Verifier.sol');
  const wrapperSol = join('contracts', 'BalanceProofGate.sol');

  if (!existsSync(verifierSol)) {
    throw new Error('Missing target/Verifier.sol. Run npm run build:proof first.');
  }

  ensureDir(solcDir);
  run('solc', [
    '--optimize',
    '--overwrite',
    '--abi',
    '--bin',
    '-o',
    solcDir,
    verifierSol,
    wrapperSol
  ]);

  function readContractBin(contractName) {
    return readFileSync(findBinFile(contractName), 'utf8').split(/\r?\n/)[0].trim();
  }

  const verifierCandidates = listFiles(solcDir, '.abi')
    .map((file) => file.replace(/\.abi$/, ''))
    .filter((name) => name !== 'BalanceProofGate' && name !== 'INoirVerifier')
    .filter((name) => {
      const abi = readJson(findAbiFile(name));
      const bytecode = readContractBin(name);
      return bytecode.length > 0 && abi.some((item) => item.type === 'function' && item.name === 'verify');
    });

  if (verifierCandidates.length !== 1) {
    throw new Error(
      `Expected exactly one generated verifier ABI with a verify function, found: ${verifierCandidates.join(', ')}`
    );
  }

  const verifierName = verifierCandidates[0];
  const wrapperName = 'BalanceProofGate';
  const verifierBytecode = readContractBin(verifierName);
  const verifierBinFile = readFileSync(findBinFile(verifierName), 'utf8');
  const libraries = [];

  for (const match of verifierBinFile.matchAll(/\/\/ \$([0-9a-f]+)\$ -> .*:([A-Za-z0-9_]+)/g)) {
    const [, placeholderId, contractName] = match;
    const placeholder = `__$${placeholderId}$__`;
    if (!verifierBytecode.includes(placeholder)) {
      continue;
    }

    libraries.push({
      contractName,
      placeholder,
      bytecode: asHex(readContractBin(contractName))
    });
  }

  const build = {
    libraries,
    verifier: {
      contractName: verifierName,
      abi: readJson(findAbiFile(verifierName)),
      bytecode: asHex(verifierBytecode)
    },
    wrapper: {
      contractName: wrapperName,
      abi: readJson(findAbiFile(wrapperName)),
      bytecode: asHex(readContractBin(wrapperName))
    }
  };

  writeJson(join(targetDir, 'build.json'), build);

  console.log(`Compiled ${verifierName} and ${wrapperName}`);
  if (libraries.length > 0) {
    console.log(`Verifier libraries: ${libraries.map((library) => library.contractName).join(', ')}`);
  }
  console.log('Wrote target/build.json');
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
