import { existsSync } from 'node:fs';
import { join } from 'node:path';
import { exampleDir, requireCommand, run, targetDir } from './common.mjs';

try {
  requireCommand('nargo', 'Install Noir/Nargo, then rerun `npm run build:proof`.');
  requireCommand('bb', 'Install Barretenberg `bb`, then rerun `npm run build:proof`.');

  const noirArtifact = join(targetDir, 'balance_proof.json');
  const proofs = [
    { name: 'dominance', proverName: 'Prover.dominance.toml', witnessName: 'dominance_witness' },
    { name: 'threshold', proverName: 'Prover.threshold.toml', witnessName: 'threshold_witness' }
  ];

  run('nargo', ['compile']);
  run('bb', ['write_vk', '-b', noirArtifact, '-o', targetDir, '--verifier_target', 'evm']);
  run('bb', [
    'write_solidity_verifier',
    '-k',
    join(targetDir, 'vk'),
    '-o',
    join(targetDir, 'Verifier.sol'),
    '--verifier_target',
    'evm'
  ]);
  for (const proof of proofs) {
    run('nargo', ['execute', '--prover-name', proof.proverName, proof.witnessName]);

    const witnessCandidates = [join(targetDir, proof.witnessName), join(targetDir, `${proof.witnessName}.gz`)];
    const witnessPath = witnessCandidates.find((path) => existsSync(path));

    if (!witnessPath) {
      throw new Error(
        `Expected witness at ${witnessCandidates.join(' or ')}. If your Nargo version writes a different witness filename, update scripts/build-proof.mjs.`
      );
    }

    run('bb', [
      'prove',
      '-b',
      noirArtifact,
      '-w',
      witnessPath,
      '-o',
      join(targetDir, 'proofs', proof.name),
      '--verifier_target',
      'evm',
      '--output_format',
      'binary'
    ]);
  }

  console.log(`Generated Noir proof artifacts in ${targetDir}`);
  console.log(`Run npm run build:contracts next from ${exampleDir}`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
