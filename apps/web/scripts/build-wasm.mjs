// Сборка ядра для браузера: cargo (wasm32, release) → wasm-bindgen → src/wasm.
// Каталог сборки — CARGO_TARGET_DIR, если задан (у каждой копии репозитория свой).
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const WASM_BINDGEN = '0.2.129';
const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../../..');
const target = process.env.CARGO_TARGET_DIR ? path.resolve(process.env.CARGO_TARGET_DIR) : path.join(root, 'target');
const out = path.resolve(here, '../src/wasm');

const version = execFileSync('wasm-bindgen', ['--version'], { encoding: 'utf8' }).trim();
if (version !== `wasm-bindgen ${WASM_BINDGEN}`) {
  console.error(`Нужен wasm-bindgen ${WASM_BINDGEN}, найден «${version}».`);
  console.error(`Установите: cargo install wasm-bindgen-cli --version ${WASM_BINDGEN} --locked`);
  process.exit(1);
}
execFileSync(
  'cargo',
  ['build', '--release', '--target', 'wasm32-unknown-unknown', '-p', 'ferrum-wasm'],
  { cwd: root, stdio: 'inherit' },
);
execFileSync(
  'wasm-bindgen',
  ['--target', 'web', '--out-dir', out, path.join(target, 'wasm32-unknown-unknown/release/ferrum_wasm.wasm')],
  { stdio: 'inherit' },
);
console.log(`ядро собрано: ${path.relative(process.cwd(), out)}`);
