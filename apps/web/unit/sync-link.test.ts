// Порт синхронизации в worker'е ядра: у него есть конец, а ядро будится только тогда, когда ему есть что собрать.

import { expect, test } from '@playwright/test';
import { SYNC_FLUSH_MS, SyncLink, type PortLike } from '../src/core/sync-link';
import type { CoreToSync, SyncToCore } from '../src/core/sync-messages';

class FakePort implements PortLike {
  onmessage: ((e: MessageEvent<SyncToCore>) => void) | null = null;
  posted: CoreToSync[] = [];
  closed = false;
  postMessage(m: CoreToSync): void {
    this.posted.push(m);
  }
  close(): void {
    this.closed = true;
  }
  /** Сообщение от `sync.worker`; закрытый порт сообщения не доставляет. */
  deliver(m: SyncToCore): void {
    if (!this.closed) this.onmessage?.({ data: m } as MessageEvent<SyncToCore>);
  }
}

function stand() {
  const runs: string[] = [];
  const intervals = new Map<number, () => void>();
  let seq = 0;
  const link = new SyncLink({
    run: (cmd) => runs.push(cmd.type),
    setInterval: (fn) => {
      intervals.set(++seq, fn);
      return seq;
    },
    clearInterval: (t) => void intervals.delete(t as number),
  });
  const tick = () => {
    for (const fn of Array.from(intervals.values())) fn();
  };
  const flushes = () => runs.filter((r) => r === 'syncFlush').length;
  return { link, runs, intervals, tick, flushes };
}

test('пачки — пока разрешено и ядру есть что собрать: в простое ядро не будится 30 раз в секунду (T8-M13)', () => {
  const s = stand();
  const port = new FakePort();
  s.link.connect(port);
  port.deliver({ t: 'flush', on: true });
  expect(s.flushes()).toBe(1);
  expect(s.intervals.size).toBe(1);
  expect(SYNC_FLUSH_MS).toBeLessThanOrEqual(34);
  // Простой: ядро ничего не делало.
  for (let i = 0; i < 30; i++) s.tick();
  expect(s.flushes()).toBe(1);
  // Правка: следующий шаг собирает её, дальше снова тихо.
  s.link.touch();
  s.tick();
  s.tick();
  expect(s.flushes()).toBe(2);
  // Команда `sync.worker` (ответ на подписку) — тоже повод собрать.
  port.deliver({ t: 'cmd', cmd: { type: 'syncAcked', doc: 'root', clientSeq: 1 } });
  s.tick();
  expect(s.flushes()).toBe(3);
  port.deliver({ t: 'flush', on: false });
  expect(s.intervals.size).toBe(0);
});

test('остановка: таймер снят, порт закрыт, поздний кадр не заводит таймер снова (T8-I5)', () => {
  const s = stand();
  const port = new FakePort();
  s.link.connect(port);
  port.deliver({ t: 'flush', on: true });
  s.link.stop();
  expect(s.intervals.size).toBe(0);
  expect(port.closed).toBe(true);
  expect(port.onmessage).toBeNull();
  port.closed = false;
  port.deliver({ t: 'flush', on: true });
  s.link.touch();
  s.tick();
  expect(s.intervals.size).toBe(0);
  expect(s.flushes()).toBe(1);
});

test('новый порт закрывает прежний и его таймер: запоздалый кадр старого порта ничего не включает (T8-I5)', () => {
  const s = stand();
  const old = new FakePort();
  s.link.connect(old);
  old.deliver({ t: 'flush', on: true });
  const next = new FakePort();
  s.link.connect(next);
  expect(old.closed).toBe(true);
  expect(s.intervals.size).toBe(0);
  const onmessage = old.onmessage;
  expect(onmessage).toBeNull();
  // События ядра идут только новому порту.
  s.link.post({ t: 'applied' });
  expect(old.posted).toEqual([]);
  expect(next.posted).toEqual([{ t: 'applied' }]);
});

test('ядро выполнило сброс и пачку чужих правок — `sync.worker` узнаёт это по порядку порта (T8-I1)', () => {
  const s = stand();
  const port = new FakePort();
  s.link.connect(port);
  port.deliver({ t: 'cmd', cmd: { type: 'syncReset' } });
  port.deliver({ t: 'cmd', cmd: { type: 'applyRemote', doc: 'root', batches: [] } });
  expect(s.runs).toEqual(['syncReset', 'applyRemote']);
  expect(port.posted).toEqual([{ t: 'reset' }, { t: 'applied' }]);
});
