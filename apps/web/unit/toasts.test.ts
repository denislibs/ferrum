// Часы уведомлений (спека подсистемы 2, раздел 10): 4 с, пока над уведомлением курсор,
// не исчезает, после ухода курсора — снова 4 с.

import { expect, test } from '@playwright/test';
import { createRoot } from 'solid-js';
import { createUiStore, MAX_TOASTS, TOAST_MS } from '../src/ui/store';

/** Ручные часы вместо `setTimeout`: тест не ждёт и не зависит от машины. */
function fakeTimers() {
  const realSet = globalThis.setTimeout;
  const realClear = globalThis.clearTimeout;
  let now = 0;
  let next = 1;
  const queue = new Map<number, { at: number; f: () => void }>();
  globalThis.setTimeout = ((f: () => void, ms: number) => {
    const id = next++;
    queue.set(id, { at: now + ms, f });
    return id;
  }) as unknown as typeof setTimeout;
  globalThis.clearTimeout = ((id: number) => void queue.delete(id)) as unknown as typeof clearTimeout;
  return {
    advance(ms: number) {
      now += ms;
      for (const [id, t] of [...queue]) {
        if (t.at <= now) {
          queue.delete(id);
          t.f();
        }
      }
    },
    restore() {
      globalThis.setTimeout = realSet;
      globalThis.clearTimeout = realClear;
    },
  };
}

function withStore(run: (s: ReturnType<typeof createUiStore>, clock: ReturnType<typeof fakeTimers>) => void) {
  const clock = fakeTimers();
  try {
    createRoot((dispose) => {
      run(createUiStore(), clock);
      dispose();
    });
  } finally {
    clock.restore();
  }
}

test('уведомление живёт 4 с', () =>
  withStore((s, clock) => {
    expect(TOAST_MS).toBe(4000);
    s.toast('Привет');
    clock.advance(3999);
    expect(s.state.toasts).toHaveLength(1);
    clock.advance(1);
    expect(s.state.toasts).toHaveLength(0);
  }));

test('пока курсор над уведомлением, оно живёт; уход курсора — снова 4 с', () =>
  withStore((s, clock) => {
    s.toast('Привет');
    const id = s.state.toasts[0].id;
    clock.advance(3000);
    s.pauseToast(id);
    clock.advance(60_000);
    expect(s.state.toasts).toHaveLength(1);
    s.startToast(id);
    clock.advance(3999);
    expect(s.state.toasts).toHaveLength(1);
    clock.advance(1);
    expect(s.state.toasts).toHaveLength(0);
  }));

test('закрытие кнопкой снимает часы; соседнее не трогает', () =>
  withStore((s, clock) => {
    s.toast('Первое');
    clock.advance(1000);
    s.toast('Второе');
    const [a, b] = s.state.toasts;
    s.dismissToast(a.id);
    expect(s.state.toasts.map((t) => t.id)).toEqual([b.id]);
    clock.advance(3999);
    expect(s.state.toasts).toHaveLength(1);
    clock.advance(1);
    expect(s.state.toasts).toHaveLength(0);
  }));

test('«Отменить»: тот же текст про новый шаг заменяет прежнее, простое не дублируется', () =>
  withStore((s, clock) => {
    s.apply({ type: 'undoNotice', text: 'Удалено', step: 1 });
    clock.advance(2000);
    s.apply({ type: 'undoNotice', text: 'Удалено', step: 2 });
    expect(s.state.toasts.map((t) => t.undoStep)).toEqual([2]);
    clock.advance(2500);
    expect(s.state.toasts).toHaveLength(1);
    s.toast('Текст');
    s.toast('Текст');
    expect(s.state.toasts.filter((t) => t.text === 'Текст')).toHaveLength(1);
  }));

test('больше предела: старые уходят вместе с часами', () =>
  withStore((s, clock) => {
    for (let i = 0; i < MAX_TOASTS + 2; i++) s.toast(`т${i}`);
    expect(s.state.toasts).toHaveLength(MAX_TOASTS);
    clock.advance(TOAST_MS);
    expect(s.state.toasts).toHaveLength(0);
  }));
