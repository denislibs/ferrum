// Планировщик: события применяются раз в кадр одной batch(), measure идёт до
// mutate, чтения layout вне measure дают предупреждение.

import { expect, test } from '@playwright/test';
import { createScheduler, guardLayout } from '../src/ui/scheduler';

function setup() {
  const log: string[] = [];
  let batches = 0;
  const s = createScheduler({
    batch: (fn) => {
      batches++;
      log.push('batch{');
      fn();
      log.push('}');
    },
  });
  return { s, log, batches: () => batches };
}

test('события между кадрами применяются в одной batch() следующего кадра', () => {
  const { s, log, batches } = setup();
  for (let i = 0; i < 3; i++) s.enqueue(() => log.push(`e${i}`));
  expect(log).toEqual([]);
  s.frame();
  expect(log).toEqual(['batch{', 'e0', 'e1', 'e2', '}']);
  expect(batches()).toBe(1);
  s.frame();
  expect(log.filter((l) => l.startsWith('e'))).toHaveLength(3);
});

test('measure раньше mutate; запись из чтения — в этом кадре, чтение из записи — в следующем', () => {
  const { s, log } = setup();
  s.everyFrame(() => {
    log.push('read');
    return () => log.push('write');
  });
  s.mutate(() => {
    log.push('m1');
    s.measure(() => log.push('next-measure'));
  });
  s.measure(() => {
    log.push('r1');
    s.mutate(() => log.push('m2'));
  });
  s.enqueue(() => log.push('event'));
  s.frame();
  expect(log).toEqual(['read', 'r1', 'batch{', 'event', 'write', 'm1', 'm2', '}']);
  log.length = 0;
  s.frame();
  expect(log).toEqual(['read', 'next-measure', 'batch{', 'write', '}']);
});

test('фаза видна подписчикам; отписка работает', () => {
  const { s } = setup();
  const phases: string[] = [];
  const off = s.everyFrame(() => {
    phases.push(s.phase());
    return () => phases.push(s.phase());
  });
  s.frame();
  expect(phases).toEqual(['measure', 'mutate']);
  expect(s.phase()).toBe('idle');
  off();
  s.frame();
  expect(phases).toHaveLength(2);
});

test('ошибка одного подписчика не срывает кадр', () => {
  const errors: unknown[] = [];
  const s = createScheduler({ batch: (fn) => fn(), onError: (e) => errors.push(e) });
  const done: string[] = [];
  s.enqueue(() => {
    throw new Error('сбой');
  });
  s.enqueue(() => done.push('после'));
  s.frame();
  expect(done).toEqual(['после']);
  expect(errors).toHaveLength(1);
});

test('чтение layout вне measure предупреждает один раз, внутри — нет', () => {
  const { s } = setup();
  const proto = {
    getBoundingClientRect: () => ({ width: 10 }),
    get offsetWidth() {
      return 10;
    },
  };
  const el = Object.create(proto) as typeof proto;
  const warnings: string[] = [];
  const restore = guardLayout([proto], s, (m) => warnings.push(m));
  s.measure(() => {
    el.getBoundingClientRect();
    void el.offsetWidth;
  });
  s.frame();
  expect(warnings).toEqual([]);
  el.getBoundingClientRect();
  el.getBoundingClientRect();
  void el.offsetWidth;
  expect(warnings).toHaveLength(2);
  expect(warnings[0]).toContain('getBoundingClientRect');
  restore();
  el.getBoundingClientRect();
  expect(warnings).toHaveLength(2);
  expect(el.getBoundingClientRect()).toEqual({ width: 10 });
});

test('ошибка в batch (эффект Solid) не срывает следующий кадр и доходит до onError', () => {
  const errors: unknown[] = [];
  let fail = true;
  const s = createScheduler({
    batch: (fn) => {
      fn();
      if (fail) throw new Error('эффект');
    },
    onError: (e) => errors.push(e),
  });
  const done: string[] = [];
  s.enqueue(() => done.push('1'));
  expect(() => s.frame()).not.toThrow();
  expect(errors).toHaveLength(1);
  expect(s.phase()).toBe('idle');
  fail = false;
  s.enqueue(() => done.push('2'));
  s.frame();
  expect(done).toEqual(['1', '2']);
  expect(errors).toHaveLength(1);
});

test('запись из записи откладывается на следующий кадр', () => {
  const { s, log } = setup();
  s.mutate(() => {
    log.push('a');
    s.mutate(() => log.push('b'));
  });
  s.frame();
  expect(log.filter((l) => l === 'a' || l === 'b')).toEqual(['a']);
  s.frame();
  expect(log.filter((l) => l === 'a' || l === 'b')).toEqual(['a', 'b']);
});

test('ошибки в подписчиках, чтениях и записях не мешают остальным', () => {
  const errors: unknown[] = [];
  const s = createScheduler({ batch: (fn) => fn(), onError: (e) => errors.push(e) });
  const done: string[] = [];
  s.everyFrame(() => {
    throw new Error('читатель');
  });
  s.everyFrame(() => () => {
    throw new Error('запись читателя');
  });
  s.everyFrame(() => () => done.push('запись'));
  s.measure(() => {
    throw new Error('чтение');
  });
  s.measure(() => done.push('чтение'));
  s.mutate(() => {
    throw new Error('запись');
  });
  s.mutate(() => done.push('мутация'));
  s.frame();
  expect(done).toEqual(['чтение', 'запись', 'мутация']);
  expect(errors).toHaveLength(4);
  expect(s.phase()).toBe('idle');
});

test('чтение, снова просящее чтение, ограничено: остаток — в следующий кадр', () => {
  const errors: unknown[] = [];
  const s = createScheduler({ batch: (fn) => fn(), onError: (e) => errors.push(e), dev: true });
  let n = 0;
  const again = (): void => {
    n++;
    s.measure(again);
  };
  s.measure(again);
  s.frame();
  expect(n).toBe(8);
  expect(errors).toHaveLength(1);
  expect(s.phase()).toBe('idle');
  // Остаток продолжился в следующем кадре (и снова упёрся в предел).
  s.frame();
  expect(n).toBe(16);
});

test('guardLayout повторно (HMR) не оборачивает дважды, снятие возвращает оригинал', () => {
  const { s } = setup();
  const getter = { getBoundingClientRect: () => 1 };
  const original = getter.getBoundingClientRect;
  const warnings: string[] = [];
  guardLayout([getter], s, (m) => warnings.push(m));
  const restore = guardLayout([getter], s, (m) => warnings.push(m));
  getter.getBoundingClientRect();
  expect(warnings).toHaveLength(1);
  restore();
  expect(getter.getBoundingClientRect).toBe(original);
});
