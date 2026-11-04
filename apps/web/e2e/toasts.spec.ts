// Уведомление с «Отменить» (спека подсистемы 2, раздел 10): кнопка отменяет шаг, пока он
// последний; у каждого уведомления — кнопка закрытия; 4 с; пока над ним курсор — живёт.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, mainMenu, openApp } from './helpers';

const notice = (page: Page) => page.locator('.fe-toast', { hasText: 'Удалено' });

async function selectLayer(page: Page) {
  const l = await live(page);
  const at = await appScreen(page, l, 200, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, 'l.selectionCount === 1');
  return l;
}

test('@firefox «Отменить» возвращает удалённое, фокус остаётся на холсте', async ({ page }) => {
  await openApp(page);
  const l = await selectLayer(page);
  await page.keyboard.press('Delete');
  await liveWhen(page, `l.nodes === ${l.nodes - 1}`);
  await expect(notice(page)).toBeVisible();
  await notice(page).getByRole('button', { name: 'Отменить' }).click();
  await liveWhen(page, `l.nodes === ${l.nodes} && l.selectionCount === 1`);
  await expect(notice(page)).toHaveCount(0);
  // Клавиши по-прежнему доходят до холста: Delete снова удаляет.
  await expect(page.locator('canvas')).toBeFocused();
  await page.keyboard.press('Delete');
  await liveWhen(page, `l.nodes === ${l.nodes - 1}`);
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

test('@firefox шаг уже не последний: кнопка ничего не отменяет', async ({ page }) => {
  await openApp(page);
  const l = await selectLayer(page);
  await page.keyboard.press('Delete');
  await liveWhen(page, `l.nodes === ${l.nodes - 1}`);
  // Новая правка поверх: новый шаг.
  await page.keyboard.press('ControlOrMeta+KeyA');
  await page.keyboard.press('ArrowRight');
  const after = await liveWhen(page, `l.steps === ${l.steps + 2}`);
  await notice(page).getByRole('button', { name: 'Отменить' }).click();
  await expect(notice(page)).toHaveCount(0);
  // Команда дошла (следующая в очереди отвечает), шаги и узлы те же.
  await page.keyboard.press('ArrowRight');
  await liveWhen(page, `l.steps === ${after.steps + 1}`);
  expect((await live(page)).nodes).toBe(l.nodes - 1);
});

test('@firefox кнопка закрытия у любого уведомления; 4 с; пока над ним курсор — не исчезает', async ({ page }) => {
  await openApp(page);
  // Обычное уведомление без действия тоже закрывается.
  await mainMenu(page, 'Справка', 'О программе');
  const about = page.locator('.fe-toast', { hasText: 'Ferrum' });
  await expect(about.getByRole('button', { name: 'Отменить' })).toHaveCount(0);
  await about.getByRole('button', { name: 'Закрыть уведомление' }).click();
  await expect(about).toHaveCount(0);

  const l = await selectLayer(page);
  await page.keyboard.press('Delete');
  await liveWhen(page, `l.nodes === ${l.nodes - 1}`);
  await expect(notice(page)).toBeVisible();
  // Без курсора уходит само: и не раньше, и не сильно позже.
  const started = Date.now();
  await expect(notice(page)).toHaveCount(0, { timeout: 6000 });
  expect(Date.now() - started).toBeGreaterThan(2500);

  await selectAgain(page, l.nodes - 1);
  await page.keyboard.press('Delete');
  await expect(notice(page)).toBeVisible();
  await notice(page).hover();
  await page.waitForTimeout(4500);
  await expect(notice(page)).toBeVisible();
  await page.mouse.move(5, 5);
  await expect(notice(page)).toHaveCount(0, { timeout: 6000 });
});

async function selectAgain(page: Page, nodes: number) {
  const l = await live(page);
  const at = await appScreen(page, l, 340, 40);
  await page.mouse.click(at.x, at.y);
  await liveWhen(page, `l.selectionCount === 1 && l.nodes === ${nodes}`);
}

test('разгруппировка и «Сплющить» из меню дают уведомление с отменой', async ({ page }) => {
  await openApp(page);
  const l = await selectLayer(page);
  await page.keyboard.press('ControlOrMeta+KeyG');
  await liveWhen(page, `l.nodes === ${l.nodes + 1}`);
  await expect(page.locator('.fe-toast')).toHaveCount(0);
  await page.keyboard.press('ControlOrMeta+Shift+KeyG');
  await liveWhen(page, `l.nodes === ${l.nodes}`);
  const t = page.locator('.fe-toast', { hasText: 'Разгруппировано' });
  await t.getByRole('button', { name: 'Отменить' }).click();
  await liveWhen(page, `l.nodes === ${l.nodes + 1}`);
  await mainMenu(page, 'Объект', 'Сплющить');
  const f = page.locator('.fe-toast', { hasText: 'Сплющено' });
  await expect(f.getByRole('button', { name: 'Отменить' })).toBeVisible();
  const steps = (await live(page)).steps;
  await f.getByRole('button', { name: 'Отменить' }).click();
  await expect(f).toHaveCount(0);
  await liveWhen(page, `l.canRedo === 1 && l.steps === ${steps}`);
});
