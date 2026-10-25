// Сборка ядра для браузера: cargo (wasm32, release) → wasm-bindgen → src/wasm.
// Каталог сборки — CARGO_TARGET_DIR, если задан (у каждой копии репозитория свой).
// `--locked`: версии зависимостей — ровно из Cargo.lock, сборка их не обновляет.
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
// SIMD128 включает `.cargo/config.toml` (`target.wasm32-unknown-unknown.rustflags`),
// но заданный RUSTFLAGS (или CARGO_ENCODED_RUSTFLAGS) cargo берёт вместо него — даже
// пустой, — и ядро молча собралось бы без SIMD. Флаг дописывается к заданным.
const SIMD = ['-C', 'target-feature=+simd128'];
// Каталог сборки — уже абсолютный: cargo считает относительный CARGO_TARGET_DIR от
// корня репозитория (cwd ниже), а этот скрипт — от текущего каталога.
const env = { ...process.env, CARGO_TARGET_DIR: target };
for (const [name, sep] of [['CARGO_ENCODED_RUSTFLAGS', '\x1f'], ['RUSTFLAGS', ' ']]) {
  const value = env[name];
  if (value === undefined || value.includes('+simd128')) continue;
  console.warn(`${name} задан («${value}»): cargo не возьмёт флаги из .cargo/config.toml, SIMD128 дописан.`);
  env[name] = value.trim() === '' ? SIMD.join(sep) : [value, ...SIMD].join(sep);
}
execFileSync(
  'cargo',
  ['build', '--release', '--locked', '--target', 'wasm32-unknown-unknown', '-p', 'ferrum-wasm'],
  { cwd: root, stdio: 'inherit', env },
);
execFileSync(
  'wasm-bindgen',
  ['--target', 'web', '--out-dir', out, path.join(target, 'wasm32-unknown-unknown/release/ferrum_wasm.wasm')],
  { stdio: 'inherit' },
);
console.log(`ядро собрано: ${path.relative(process.cwd(), out)}`);
