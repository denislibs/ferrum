// Редактор (index.html): запуск ядра под интерфейсом на SolidJS, события ядра в
// состоянии интерфейса, шрифты и токены дизайн-системы.

import { expect, test } from '@playwright/test';
import { live, liveWhen, openApp } from './helpers';

test('редактор запускает ядро и получает раскладку, пресеты и документ', async ({ page }) => {
  await openApp(page);
  const info = await page.evaluate(() => {
    const f = window.__ferrum!;
    return {
      mode: f.core.mode,
      commands: f.ui!.keymap.commands.length,
      presets: f.ui!.presets.length,
      pages: f.ui!.pages.list.length,
      total: f.ui!.layers.total,
      name: f.ui!.documentName,
      errors: f.uiEvents!.filter((e) => e.type === 'error'),
    };
  });
  expect(info).toEqual({ mode: 'sab', commands: 43, presets: 11, pages: 1, total: 10, name: 'Без названия', errors: [] });
});

test('шрифты и цвета — из дизайн-системы', async ({ page }) => {
  await openApp(page);
  const style = await page.evaluate(async () => {
    // Шрифты вшиты в бандл (fontsource): грузятся с того же сервера, без сети.
    const onest = await document.fonts.load('12px "Onest Variable"', 'Слой');
    const mono = await document.fonts.load('11px "JetBrains Mono"', '1440');
    const body = getComputedStyle(document.body);
    return {
      onest: onest.length > 0,
      mono: mono.length > 0,
      bg: body.backgroundColor,
      font: body.fontFamily,
    };
  });
  expect(style.onest && style.mono).toBe(true);
  expect(style.bg).toBe('rgb(26, 25, 24)');
  expect(style.font).toContain('Onest Variable');
});

test('клавиши с холста доходят до редактора', async ({ page }) => {
  await openApp(page);
  await page.locator('canvas').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('KeyR');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('rect');
  await page.keyboard.press('Escape');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('move');
  // Кадры идут и после ввода: счётчик кадров worker'а растёт.
  const before = (await live(page)).frames;
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'execute', command: 'view.zoom_in' }));
  await liveWhen(page, `l.frames > ${before}`);
});

test('поля ввода и виджеты оставляют клавиши себе', async ({ page }) => {
  await openApp(page);
  await page.evaluate(() => {
    document.body.insertAdjacentHTML(
      'beforeend',
      '<div id="ce" contenteditable="" tabindex="0">x</div><div id="plain" contenteditable="plaintext-only" tabindex="0">x</div><div id="tab" role="tab" tabindex="0">t</div>',
    );
  });
  for (const id of ['ce', 'plain']) {
    await page.locator(`#${id}`).focus();
    await page.keyboard.press('KeyR');
    expect(await page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('move');
  }
  // Стрелка на вкладке не уходит в редактор (двигала бы выделение).
  await page.locator('#tab').focus();
  const sent = await page.evaluate(() => {
    const e = new KeyboardEvent('keydown', { code: 'ArrowRight', bubbles: true, cancelable: true });
    document.getElementById('tab')!.dispatchEvent(e);
    return e.defaultPrevented;
  });
  expect(sent).toBe(false);
  await page.locator('canvas').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('KeyR');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('rect');
});

test('сбой обработчика кадра не останавливает кадры и показан один раз', async ({ page }) => {
  await openApp(page);
  await page.evaluate(() => {
    const w = window as unknown as { __ticks: number };
    w.__ticks = 0;
    window.__ferrum!.core.onFrame(() => {
      throw new Error('проверочный сбой');
    });
    window.__ferrum!.core.onFrame(() => w.__ticks++);
  });
  await page.waitForFunction(() => (window as unknown as { __ticks: number }).__ticks > 10);
  await expect(page.locator('.fe-toast', { hasText: 'Ошибка интерфейса: проверочный сбой' })).toHaveCount(1);
  expect(await page.evaluate(() => window.__ferrum!.ui!.stopped)).toBeNull();
});

test('ошибка внутри batch планировщика не замораживает редактор', async ({ page }) => {
  await openApp(page);
  // Эффект Solid, бросающий в общей batch(): событие `tool` вызывает исключение.
  await page.evaluate(() => {
    const sched = (window.__ferrum as unknown as { scheduler: { enqueue(f: () => void): void } }).scheduler;
    sched.enqueue(() => {
      throw new Error('эффект');
    });
  });
  await page.locator('canvas').click({ position: { x: 5, y: 5 } });
  await page.keyboard.press('KeyR');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.tool)).toBe('rect');
  await expect(page.locator('.fe-toast', { hasText: 'Ошибка интерфейса: эффект' })).toHaveCount(1);
});

test('защита layout: чтения самого Core не предупреждают, чтение интерфейса вне measure — да', async ({ page }) => {
  await openApp(page);
  await page.setViewportSize({ width: 900, height: 600 });
  await liveWhen(page, 'l.viewportW === 900 || l.viewportW > 0');
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  const warnings = () => page.evaluate(() => (window as unknown as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? []);
  expect(await warnings()).toEqual([]);
  await page.evaluate(() => document.querySelector('canvas')!.getBoundingClientRect());
  expect(await warnings()).toHaveLength(1);
});

test('в обычном режиме журнал событий не ведётся и данные не копятся', async ({ page }) => {
  await page.goto('/?nodes=10');
  await page.waitForFunction(() => window.__ferrum?.ui?.ready === true);
  expect(await page.evaluate(() => [window.__ferrum!.events.length, window.__ferrum!.uiEvents])).toEqual([0, undefined]);
});

test('сбой ядра после запуска виден пользователю один раз', async ({ page }) => {
  await openApp(page);
  // Сбой worker'а после `ready` — тот же путь, что `fatal` от worker'а.
  await page.evaluate(() => (window.__ferrum!.core as unknown as { fail(m: string): void }).fail('проверка'));
  await expect(page.getByRole('alert')).toHaveText('Ядро остановлено: проверка. Правки больше не принимаются — перезагрузите страницу');
  // Команды после остановки — без новых окон и без вечной очереди.
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'execute', command: 'edit.undo' }));
  await expect(page.getByRole('alert')).toHaveCount(1);
  // Ни уведомлений, ни нового окна на каждую команду после остановки.
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'execute', command: 'edit.redo' }));
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(r)));
  await expect(page.locator('.fe-toast')).toHaveCount(0);
  expect(await page.evaluate(() => window.__ferrum!.ui!.toasts.length)).toBe(0);
});
