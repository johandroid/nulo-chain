import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const exampleDir = dirname(dirname(fileURLToPath(import.meta.url)));
export const targetDir = join(exampleDir, 'target');
export const solcDir = join(targetDir, 'solc');

export function ensureDir(path) {
  mkdirSync(path, { recursive: true });
}

export function requireCommand(command, installHint) {
  const result = spawnSync('sh', ['-lc', `command -v ${command}`], {
    stdio: 'ignore'
  });

  if (result.status !== 0) {
    throw new Error(`Missing required command: ${command}\n${installHint}`);
  }
}

export function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? exampleDir,
    stdio: options.stdio ?? 'inherit',
    encoding: options.encoding ?? 'utf8'
  });

  if (result.status !== 0) {
    throw new Error(`Command failed: ${command} ${args.join(' ')}`);
  }

  return result;
}

export function readJson(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

export function writeJson(path, value) {
  ensureDir(dirname(path));
  writeFileSync(path, `${JSON.stringify(value, null, 2)}\n`);
}

export function asHex(bytecode) {
  if (!bytecode || bytecode === '0x') {
    throw new Error('Compiled contract bytecode is empty');
  }
  return bytecode.startsWith('0x') ? bytecode : `0x${bytecode}`;
}

export function readHexFile(path) {
  const bytes = readFileSync(path);
  return `0x${bytes.toString('hex')}`;
}

export function mustExist(path, message) {
  if (!existsSync(path)) {
    throw new Error(message);
  }
}

export function findAbiFile(contractName) {
  const filename = `${contractName}.abi`;
  const path = join(solcDir, filename);
  mustExist(path, `Missing ${filename}. Run npm run build:contracts first.`);
  return path;
}

export function findBinFile(contractName) {
  const filename = `${contractName}.bin`;
  const path = join(solcDir, filename);
  mustExist(path, `Missing ${filename}. Run npm run build:contracts first.`);
  return path;
}

export function listFiles(path, suffix) {
  if (!existsSync(path)) {
    return [];
  }

  return readdirSync(path).filter((file) => file.endsWith(suffix));
}
