// Worker проверки с подстроенными сбоями для `validator.test.ts`. Метка — файл: он один на все worker'ы процесса, поэтому
// сбой случается ровно один раз, а новый worker уже работает как настоящий.

import { existsSync, unlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import '../src/sync/validator.worker';

/** Путь метки: тест и worker считают его одинаково. */
export const faultMark = (): string => join(tmpdir(), `ferrum-validator-fault-${process.pid}`);

const scope = globalThis as unknown as Worker;
const real = scope.onmessage!;
const marker = faultMark();

scope.onmessage = (e: MessageEvent<{ id: number; op: string }>) => {
  const m = e.data;
  if (marker && existsSync(`${marker}.validate`) && m.op === 'validate') {
    unlinkSync(`${marker}.validate`);
    // Как нехватка памяти wasm на невиновной пачке.
    scope.postMessage({ id: m.id, ok: false, reason: 'bad', fatal: true });
    return;
  }
  if (marker && existsSync(`${marker}.load`) && m.op === 'load') {
    unlinkSync(`${marker}.load`);
    // Как нехватка памяти wasm при загрузке невиновного документа: память заняли чужие документы.
    scope.postMessage({ id: m.id, ok: false, reason: 'fatal', message: 'unreachable' });
    return;
  }
  if (marker && existsSync(`${marker}.base`) && m.op === 'base') {
    unlinkSync(`${marker}.base`);
    throw new Error('сбой worker’а');
  }
  real.call(scope, e);
};
