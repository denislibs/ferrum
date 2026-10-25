// Редактор (index.html): запуск ядра под интерфейсом на SolidJS, события ядра в
// состоянии интерфейса, шрифты и токены дизайн-системы.

import { expect, test } from '@playwright/test';
import { liveWhen, openApp } from './helpers';

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
      errors: f.events.filter((e) => e.type === 'error'),
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
  await liveWhen(page, 'l.frames > 0');
});

test('сбой ядра после запуска виден пользователю один раз', async ({ page }) => {
  await openApp(page);
  // Сбой worker'а после `ready` — тот же путь, что `fatal` от worker'а.
  await page.evaluate(() => (window.__ferrum!.core as unknown as { fail(m: string): void }).fail('проверка'));
  await expect(page.getByRole('alert')).toHaveText('Ядро остановлено: проверка. Правки больше не принимаются — перезагрузите страницу');
  // Команды после остановки — без новых окон и без вечной очереди.
  await page.evaluate(() => window.__ferrum!.core.send({ type: 'execute', command: 'edit.undo' }));
  await expect(page.getByRole('alert')).toHaveCount(1);
});
