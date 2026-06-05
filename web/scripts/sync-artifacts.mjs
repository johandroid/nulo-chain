#!/usr/bin/env node
import { copyFileSync, existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const webDir = dirname(dirname(fileURLToPath(import.meta.url)));
const repoDir = dirname(webDir);
const sourceDir = join(repoDir, 'examples', 'zkp-revive', 'target');
const outputDir = join(webDir, 'public', 'artifacts');

const requiredFiles = [
  'build.json',
  'proofs/dominance/proof',
  'proofs/dominance/public_inputs',
  'proofs/threshold/proof',
  'proofs/threshold/public_inputs'
];

for (const file of requiredFiles) {
  const source = join(sourceDir, file);
  if (!existsSync(source)) {
    throw new Error(`Missing ${source}. Run \`cd ../examples/zkp-revive && npm run build\` first.`);
  }
}

for (const file of requiredFiles) {
  const source = join(sourceDir, file);
  const destination = join(outputDir, file);
  mkdirSync(dirname(destination), { recursive: true });
  copyFileSync(source, destination);
}

writeFileSync(
  join(outputDir, 'manifest.json'),
  `${JSON.stringify(
    {
      source: 'examples/zkp-revive/target',
      generatedAt: new Date().toISOString(),
      files: requiredFiles
    },
    null,
    2
  )}\n`
);

console.log(`Synced ${requiredFiles.length} ZKP artifact files to ${outputDir}`);
