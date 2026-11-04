// Линейки и направляющие протяжкой (спека подсистемы 2, раздел 9): рисует их ядро в
// кадре, поэтому проверка — по пикселям снимка и по живому состоянию; настройка
// переживает перезагрузку. Линии в страницах примера: фреймы до (1380, 1020), справа
// и снизу от них пусто.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, expectPixel, live, liveWhen, mainMenu, openApp, pixel } from './helpers';

/** Цвет линейки и направляющей в тёмной теме (`OverlayTheme::dark`). */
const RULER = [0x23, 0x21, 0x20];
const GUIDE = [0xff, 0x5a, 0x5f];

/** Есть ли цвет направляющей в строке снимка рядом с `y`: положение линии — в целых
 * единицах мира, на экране при зуме 0,5 строка может быть соседней. */
async function guideRow(page: Page, x: number, y: number): Promise<boolean> {
  for (const row of [y - 1, y, y + 1]) {
    const p = await pixel(page, x, row);
    if (Math.abs(p[0] - GUIDE[0]) <= 40 && Math.abs(p[1] - GUIDE[1]) <= 50 && Math.abs(p[2] - GUIDE[2]) <= 50) return true;
  }
  return false;
}

async function expectGuide(page: Page, x: number, y: number, on = true): Promise<void> {
  await expect.poll(() => guideRow(page, x, y), { timeout: 10_000 }).toBe(on);
}

async function canvasBox(page: Page) {
  return (await page.locator('canvas').boundingBox())!;
}

async function rulersOn(page: Page) {
  const box = await canvasBox(page);
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+KeyR');
  await expectPixel(page, box.x + 60, box.y + 2, RULER);
  return box;
}

test('@firefox Shift+R и пункт «Вид → Линейки» включают линейки, настройка переживает перезагрузку', async ({ page }) => {
  await openApp(page);
  const box = await canvasBox(page);
  const canvas = await pixel(page, box.x + 60, box.y + 2);
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+KeyR');
  await expectPixel(page, box.x + 60, box.y + 2, RULER);
  await expectPixel(page, box.x + 2, box.y + 60, RULER);
  expect(await live(page).then((l) => l.steps)).toBe(0);
  // Пункт меню отмечен и снимает отметку.
  await mainMenu(page, 'Вид', 'Линейки');
  await expectPixel(page, box.x + 60, box.y + 2, canvas.slice(0, 3));
  // Настройка сохраняется: снова включаем — после перезагрузки включена.
  await mainMenu(page, 'Вид', 'Линейки');
  await expectPixel(page, box.x + 60, box.y + 2, RULER);
  await page.waitForTimeout(400); // запись настроек — раз в четверть секунды
  await page.reload();
  await page.waitForFunction(() => window.__ferrum?.ready === true && window.__ferrum.ui?.ready === true);
  await expectPixel(page, box.x + 60, box.y + 2, RULER);
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

test('@firefox «Линейки» в меню пустого места холста', async ({ page }) => {
  await openApp(page);
  const box = await canvasBox(page);
  const at = await appScreen(page, await live(page), 100, 1080);
  await page.mouse.click(at.x, at.y, { button: 'right' });
  const item = page.getByRole('menuitemcheckbox', { name: /Линейки/ });
  await expect(item).toBeVisible();
  await expect(item).toHaveAttribute('aria-checked', 'false');
  await item.click();
  await expectPixel(page, box.x + 60, box.y + 2, RULER);
});

test('@firefox направляющая из линейки: один шаг, тянется, на линейке удаляется, отмена', async ({ page }) => {
  await openApp(page);
  const box = await rulersOn(page);
  const l = await live(page);
  // Ниже страницы примера: пусто, линия принадлежит странице.
  const low = await appScreen(page, l, 100, 1080);
  const y = Math.round(low.y);
  const x = box.x + 120; // левее тулбара
  expect(y).toBeLessThan(box.y + box.height - 40);
  await drag(page, { x, y: box.y + 10 }, 0, y - box.y - 10);
  await liveWhen(page, `l.steps === ${l.steps + 1}`);
  await expectGuide(page, box.x + 60, y);
  // Подвинуть за саму линию.
  await drag(page, { x, y }, 0, 30);
  await liveWhen(page, `l.steps === ${l.steps + 2}`);
  await expectGuide(page, box.x + 60, y + 30);
  // Обратно на линейку — удаление, тоже один шаг.
  await drag(page, { x, y: y + 30 }, 0, box.y + 10 - (y + 30));
  await liveWhen(page, `l.steps === ${l.steps + 3}`);
  await expectGuide(page, box.x + 60, y + 30, false);
  // Отмена возвращает линию, ещё отмена — на прежнее место.
  await page.keyboard.press('ControlOrMeta+KeyZ');
  await expectGuide(page, box.x + 60, y + 30);
  await page.keyboard.press('ControlOrMeta+KeyZ');
  await expectGuide(page, box.x + 60, y);
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

test('@firefox Esc во время протяжки — без следов', async ({ page }) => {
  await openApp(page);
  const box = await rulersOn(page);
  const l = await live(page);
  await page.mouse.move(box.x + 400, box.y + 10);
  await page.mouse.down();
  // Ниже страницы примера: рядом нет краёв узлов, привязка не сдвинет линию.
  await page.mouse.move(box.x + 400, box.y + 660, { steps: 5 });
  await expectGuide(page, box.x + 60, box.y + 660);
  await page.keyboard.press('Escape');
  await page.mouse.up();
  await expectGuide(page, box.x + 60, box.y + 660, false);
  expect((await live(page)).steps).toBe(l.steps);
});

test('@firefox к направляющей привязываются узлы, щелчок по линии выделяет узел под ней', async ({ page }) => {
  await openApp(page);
  const box = await rulersOn(page);
  const l = await live(page);
  // Вертикальная линия на мировой x = 400: «Слой 1» в «Экран 0» — (140, 0)–(260, 80).
  const line = await appScreen(page, l, 400, 40);
  const lineX = Math.round(line.x);
  await drag(page, { x: box.x + 10, y: box.y + 300 }, lineX - box.x - 10, 0);
  const placed = await liveWhen(page, `l.steps === ${l.steps + 1}`);
  // Положение линии — от координат экрана; читаем его сдвигом узла ниже.
  const node = await appScreen(page, placed, 200, 40);
  await page.mouse.click(node.x, node.y);
  await liveWhen(page, 'l.selectionCount === 1 && l.selectionX === 140');
  // Левый край дойдёт почти до линии: остаётся 2 экранных пикселя — привязка.
  const lineWorld = Math.round((lineX - box.x - box.width / 2) / placed.zoom + placed.cameraX);
  const dx = (lineWorld - 140) * placed.zoom - 2;
  // Нажатие не в той же точке: иначе это был бы двойной щелчок, вход в правку.
  await drag(page, { x: node.x + 10, y: node.y + 5 }, dx, 0, 8);
  const snapped = await liveWhen(page, `l.steps === ${l.steps + 2}`);
  expect(snapped.selectionX).toBe(lineWorld);
  // Щелчок по линии над другим фреймом выделяет то, что под ней.
  await page.keyboard.press('Escape');
  await liveWhen(page, 'l.selectionCount === 0');
  await page.mouse.click(lineX, node.y + 40);
  await liveWhen(page, 'l.selectionCount === 1');
  expect((await live(page)).steps).toBe(snapped.steps);
});

test('@firefox Shift+G прячет направляющие: линий и привязки нет; без линеек пункт серый', async ({ page }) => {
  await openApp(page);
  const box = await rulersOn(page);
  const l = await live(page);
  const y = Math.round(box.y + box.height - 60);
  await drag(page, { x: box.x + 400, y: box.y + 10 }, 0, y - box.y - 10);
  await liveWhen(page, `l.steps === ${l.steps + 1}`);
  await expectGuide(page, box.x + 60, y);
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+KeyG');
  await expectGuide(page, box.x + 60, y, false);
  await expectPixel(page, box.x + 60, box.y + 2, RULER);
  // Отметка в меню снята.
  await mainMenu(page, 'Вид', 'Направляющие');
  await expectGuide(page, box.x + 60, y);
  // Линейки выключены — и линии не видно (как в Figma), пункт серый.
  await page.locator('canvas').focus();
  await page.keyboard.press('Shift+KeyR');
  await expectGuide(page, box.x + 60, y, false);
  await page.getByRole('button', { name: 'Главное меню' }).click();
  await page.getByRole('menuitem', { name: 'Вид', exact: true }).hover();
  await expect(page.getByRole('menuitemcheckbox', { name: /Направляющие/ })).toHaveAttribute('aria-disabled', 'true');
});

test('удержание Shift+R не мигает линейками', async ({ page }) => {
  await openApp(page);
  const box = await rulersOn(page);
  // Повтор клавиши: то же сочетание без отпускания.
  await page.keyboard.down('Shift');
  await page.keyboard.down('KeyR');
  await page.keyboard.down('KeyR');
  await page.keyboard.down('KeyR');
  await page.keyboard.up('KeyR');
  await page.keyboard.up('Shift');
  const before = await live(page);
  await page.locator('canvas').hover({ position: { x: 300, y: 300 } });
  await page.waitForFunction((f) => window.__ferrum!.core!.live!.frames >= f, before.frames);
  // Первое нажатие выключило линейки, повторы ничего не поменяли.
  const px = await pixel(page, box.x + 60, box.y + 2);
  expect(px.slice(0, 3)).not.toEqual(RULER);
});
