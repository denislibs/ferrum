// Умный дубликат и «Выделить подобные» (спека подсистемы 2, раздел 6): после
// Ctrl/Cmd+D и сдвига копии следующий Ctrl/Cmd+D повторяет сдвиг; «Выделить
// подобные» — узлы страницы того же типа с теми же заливками и обводками.

import { expect, test } from '@playwright/test';
import { appScreen, drag, live, liveWhen, mainMenu, openApp } from './helpers';

test('Ctrl/Cmd+D после сдвига копии повторяет тот же сдвиг', async ({ page }) => {
  await openApp(page);
  // «Слой 1» в «Экран 0»: мир (140, 0) — (260, 80).
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1 && l.selectionX === 140');
  await page.keyboard.press('ControlOrMeta+KeyD');
  await liveWhen(page, `l.nodes === ${l.nodes + 1} && l.selectionX === 140`);
  // Копию — стрелками: Shift+→ дважды и Shift+↓ (20, 10).
  for (const key of ['Shift+ArrowRight', 'Shift+ArrowRight', 'Shift+ArrowDown']) await page.keyboard.press(key);
  await liveWhen(page, 'l.selectionX === 160 && l.selectionY === 10');
  await page.keyboard.press('ControlOrMeta+KeyD');
  await liveWhen(page, `l.nodes === ${l.nodes + 2} && l.selectionX === 180 && l.selectionY === 20`);
  await page.keyboard.press('ControlOrMeta+KeyD');
  await liveWhen(page, `l.nodes === ${l.nodes + 3} && l.selectionX === 200 && l.selectionY === 30`);
  // Протяжкой — тоже сдвиг: следующий дубликат повторяет шаг от прошлой копии
  // (180, 20) до этой, где бы её ни оставила привязка.
  const copy = await appScreen(page, await live(page), 300, 90);
  await drag(page, copy, 0, 40);
  const moved = await liveWhen(page, `l.steps === ${l.steps + 7}`);
  expect(moved.selectionY).toBeGreaterThan(30);
  await page.keyboard.press('ControlOrMeta+KeyD');
  await liveWhen(
    page,
    `l.nodes === ${l.nodes + 4} && l.selectionX === ${2 * moved.selectionX - 180} && l.selectionY === ${2 * moved.selectionY - 20}`,
  );
});

test('«Выделить подобные» из меню и Ctrl/Cmd+Alt+A', async ({ page }) => {
  await openApp(page);
  const at = await appScreen(page, await live(page), 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  // Двойник «Слоя 1» — в каждом из 10 фреймов.
  await mainMenu(page, 'Правка', 'Выделить подобные');
  await liveWhen(page, 'l.selectionCount === 10');
  // Снова один «Слой 1»: щелчок мимо фреймов снимает выделение.
  const empty = await appScreen(page, await live(page), 100, 1080);
  await page.mouse.click(empty.x, empty.y);
  await liveWhen(page, 'l.selectionCount === 0');
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  await page.keyboard.press('ControlOrMeta+Alt+KeyA');
  await liveWhen(page, 'l.selectionCount === 10');
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});
