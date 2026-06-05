import { join } from 'node:path';
import { mustExist, readJson, targetDir } from './common.mjs';

try {
  const buildPath = join(targetDir, 'build.json');
  const proofNames = ['dominance', 'threshold'];

  mustExist(buildPath, 'Missing target/build.json. Run npm run build.');
  for (const proofName of proofNames) {
    mustExist(join(targetDir, 'proofs', proofName, 'proof'), `Missing target/proofs/${proofName}/proof. Run npm run build:proof.`);
    mustExist(join(targetDir, 'proofs', proofName, 'public_inputs'), `Missing target/proofs/${proofName}/public_inputs. Run npm run build:proof.`);
  }

  const build = readJson(buildPath);

  for (const [name, artifact] of Object.entries({ verifier: build.verifier, wrapper: build.wrapper })) {
    if (!artifact.contractName || !Array.isArray(artifact.abi) || !artifact.bytecode?.startsWith('0x')) {
      throw new Error(`Invalid ${name} artifact in target/build.json`);
    }
  }

  if (!Array.isArray(build.libraries)) {
    throw new Error('Invalid libraries artifact in target/build.json');
  }

  for (const library of build.libraries) {
    if (!library.contractName || !library.placeholder || !library.bytecode?.startsWith('0x')) {
      throw new Error(`Invalid library artifact in target/build.json: ${library.contractName ?? '<unknown>'}`);
    }
  }

  console.log('Artifacts look complete.');
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
