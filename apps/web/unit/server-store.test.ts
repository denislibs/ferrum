// Основной поток и worker файлов: код отказа доходит до сохранителя, он решает, повторять ли запись.

import { expect, test } from '@playwright/test';
import { ServerFileStore } from '../src/ui/server/server-store';

const g = globalThis as Record<string, unknown>;
const names = ['Worker', 'document', 'window', 'BroadcastChannel'];
const saved = names.map((k) => [k, Object.getOwnPropertyDescriptor(globalThis, k)] as const);
test.afterEach(() => {
  for (const [k, d] of saved) {
    if (d) Object.defineProperty(globalThis, k, d);
    else delete g[k];
  }
});

test("отказ worker'а несёт код: сохранитель отличает `CONFLICT` от сбоя сети (T6-I4)", async () => {
  const workers: { onmessage: (e: { data: unknown }) => void; sent: { t: string; id?: number }[] }[] = [];
  g.Worker = class {
    onmessage: (e: { data: unknown }) => void = () => {};
    onerror = null;
    sent: { t: string; id?: number }[] = [];
    constructor() {
      workers.push(this);
    }
    postMessage(m: { t: string; id?: number }) {
      this.sent.push(m);
    }
    terminate() {}
  };
  g.window = { addEventListener() {}, removeEventListener() {} };
  g.document = { visibilityState: 'visible', addEventListener() {}, removeEventListener() {} };
  g.BroadcastChannel = class {
    postMessage() {}
    close() {}
  };
  const store = new ServerFileStore('user');
  // Первый — worker настроек браузера, последний — worker сервера.
  const worker = workers.at(-1)!;
  const reading = store.read('f').catch((e: unknown) => e);
  const request = worker.sent.find((m) => m.t === 'req')!;
  worker.onmessage({ data: { t: 'res', id: request.id, ok: false, message: 'Файл изменён', code: 'CONFLICT' } });
  const e = (await reading) as Error & { code?: string };
  expect(e.message).toBe('Файл изменён');
  expect(e.code).toBe('CONFLICT');
});

test("T9-M5: запрос комментариев не зависает — сбой worker'а и вызов после остановки отвечают `NETWORK`", async () => {
  const workers: { onerror: ((e: { preventDefault(): void }) => void) | null; sent: { t: string }[] }[] = [];
  g.Worker = class {
    onmessage = null;
    onerror: ((e: { preventDefault(): void }) => void) | null = null;
    sent: { t: string }[] = [];
    constructor() {
      workers.push(this);
    }
    postMessage(m: { t: string }) {
      this.sent.push(m);
    }
    terminate() {}
  };
  g.window = { addEventListener() {}, removeEventListener() {} };
  g.document = { visibilityState: 'visible', addEventListener() {}, removeEventListener() {} };
  g.BroadcastChannel = class {
    postMessage() {}
    close() {}
  };
  const store = new ServerFileStore('user');
  const core = { connectSync() {}, disconnectSync() {} };
  const events = { status() {}, reload() {}, participants() {} };
  const hang = Symbol('нет ответа');
  // Ответ уже должен быть: обещание, решённое раньше, выигрывает гонку у решённого следом.
  const now = <T>(p: Promise<T>) => Promise.race([p, Promise.resolve(hang)]);

  const a = store.sync('f', core, events);
  const asked = a.comments!({ k: 'mentions', q: '' });
  workers.at(-1)!.onerror!({ preventDefault() {} });
  expect(await now(asked)).toEqual({ ok: false, failure: 'NETWORK' });

  const b = store.sync('f', core, events);
  b.stop();
  const sent = workers.at(-1)!.sent.length;
  expect(await now(b.comments!({ k: 'mentions', q: '' }))).toEqual({ ok: false, failure: 'NETWORK' });
  expect(workers.at(-1)!.sent).toHaveLength(sent);
});
