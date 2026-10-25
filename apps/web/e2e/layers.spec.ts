// Панель слоёв и страниц: выделение в обе стороны, переименование, порядок
// перетаскиванием, скрыть и заблокировать, страницы, виртуальный список
// (в DOM — только окно строк).

import { expect, test, type Page } from '@playwright/test';
import { appScreen, live, liveWhen, openApp } from './helpers';

const tree = (page: Page) => page.getByRole('tree', { name: 'Слои' });
const row = (page: Page, name: string) => tree(page).getByRole('treeitem', { name, exact: true });
const names = (page: Page) => tree(page).locator('.fe-layer__name').allTextContents();

test('выделение из панели слоёв и с холста', async ({ page }) => {
  await openApp(page);
  await row(page, 'Экран 8').click();
  await expect(row(page, 'Экран 8')).toHaveAttribute('aria-selected', 'true');
  const l = await liveWhen(page, 'l.selectionCount === 1');
  expect([l.selectionW, l.selectionH]).toEqual([1440, 1024]);
  // Щелчок по узлу на холсте: фрейм раскрывается, строка выделена.
  const node = await appScreen(page, l, 200, 40);
  await page.mouse.click(node.x, node.y);
  await expect(row(page, 'Экран 0')).toHaveAttribute('aria-expanded', 'true');
  await expect(row(page, 'Слой 1')).toHaveAttribute('aria-selected', 'true');
  await expect(row(page, 'Слой 1')).toBeInViewport();
  // Shift — диапазон, Ctrl/Cmd — добавить. Строки фреймов наверху списка: список
  // прокручен к «Слой 1», их нет в DOM, пока не прокрутить обратно.
  await tree(page).evaluate((el) => (el.scrollTop = 0));
  await row(page, 'Экран 9').click();
  await row(page, 'Экран 7').click({ modifiers: ['Shift'] });
  await liveWhen(page, 'l.selectionCount === 3');
  await row(page, 'Экран 5').click({ modifiers: ['ControlOrMeta'] });
  await liveWhen(page, 'l.selectionCount === 4');
});

test('переименование на месте — один шаг отмены', async ({ page }) => {
  await openApp(page);
  const before = await live(page);
  await row(page, 'Экран 9').dblclick();
  const input = tree(page).getByRole('textbox', { name: 'Имя слоя' });
  await input.fill('Главная');
  await input.press('Enter');
  await expect(row(page, 'Главная')).toBeVisible();
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+z');
  await expect(row(page, 'Экран 9')).toBeVisible();
  // Esc отменяет переименование.
  await row(page, 'Экран 9').dblclick();
  await input.fill('Не то');
  await input.press('Escape');
  await expect(row(page, 'Экран 9')).toBeVisible();
  // Поле убрано, а blur при удалении поля имени не применил.
  await page.waitForTimeout(200);
  expect((await live(page)).steps).toBe(before.steps + 1);
});

test('перетаскивание меняет порядок и вложенность', async ({ page }) => {
  await openApp(page);
  const before = await live(page);
  const from = (await row(page, 'Экран 9').boundingBox())!;
  const to = (await row(page, 'Экран 8').boundingBox())!;
  // Нижняя четверть строки «Экран 8» — под ней.
  await page.mouse.move(from.x + 60, from.y + 14);
  await page.mouse.down();
  await page.mouse.move(from.x + 60, to.y + 10, { steps: 4 });
  await page.mouse.move(from.x + 60, to.y + 25, { steps: 4 });
  await expect(row(page, 'Экран 8')).toHaveClass(/fe-layer--drop-below/);
  await page.mouse.up();
  await expect.poll(() => names(page).then((n) => n.slice(0, 2))).toEqual(['Экран 8', 'Экран 9']);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  // Середина строки фрейма — внутрь.
  const a = (await row(page, 'Экран 7').boundingBox())!;
  const b = (await row(page, 'Экран 6').boundingBox())!;
  await page.mouse.move(a.x + 60, a.y + 14);
  await page.mouse.down();
  await page.mouse.move(a.x + 60, b.y + 14, { steps: 6 });
  await page.mouse.up();
  await expect(row(page, 'Экран 6')).toHaveAttribute('aria-expanded', 'false');
  await row(page, 'Экран 6').getByRole('button', { name: 'Развернуть' }).click();
  await expect(row(page, 'Экран 7')).toHaveAttribute('aria-level', '2');
});

test('скрыть и заблокировать кнопками строки', async ({ page }) => {
  await openApp(page);
  const r = row(page, 'Экран 9');
  await r.hover();
  await r.getByRole('button', { name: 'Скрыть' }).click();
  await expect(r.getByRole('button', { name: 'Показать' })).toHaveAttribute('aria-pressed', 'true');
  await expect(r).toHaveClass(/fe-layer--hidden/);
  await r.getByRole('button', { name: 'Заблокировать' }).click();
  await expect(r.getByRole('button', { name: 'Разблокировать' })).toBeVisible();
  // Кнопки включённых флагов видны и без наведения.
  await page.mouse.move(700, 400);
  await expect(r.getByRole('button', { name: 'Показать' })).toBeVisible();
  const l = await live(page);
  expect(l.steps).toBe(2);
});

test('страницы: добавить, переключить, переименовать, удалить', async ({ page }) => {
  await openApp(page);
  const pages = page.getByRole('listbox', { name: 'Страницы' });
  await page.getByRole('button', { name: 'Добавить страницу' }).click();
  await expect(pages.getByRole('option')).toHaveCount(2);
  await expect(pages.getByRole('option', { name: /Страница 2/ })).toHaveAttribute('aria-selected', 'true');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(0);
  await pages.getByRole('option', { name: /Страница 1/ }).click();
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(10);
  await pages.getByRole('option', { name: /Страница 2/ }).dblclick();
  await pages.getByRole('textbox', { name: 'Имя страницы' }).fill('Черновик');
  await pages.getByRole('textbox', { name: 'Имя страницы' }).press('Enter');
  await expect(pages.getByRole('option', { name: /Черновик/ })).toBeVisible();
  await pages.getByRole('option', { name: /Черновик/ }).hover();
  await pages.getByRole('button', { name: 'Удалить страницу «Черновик»' }).click();
  // Удаление страницы не отменяется: сначала вопрос, Отмена — в фокусе.
  const confirm = page.getByRole('dialog', { name: 'Удалить страницу?' });
  await expect(confirm.getByRole('button', { name: 'Отмена' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(confirm).toHaveCount(0);
  await expect(pages.getByRole('option')).toHaveCount(2);
  await pages.getByRole('option', { name: /Черновик/ }).focus();
  await page.keyboard.press('Delete');
  await confirm.getByRole('button', { name: 'Удалить' }).click();
  await expect(pages.getByRole('option')).toHaveCount(1);
  // Сворачивается.
  await page.getByRole('button', { name: 'Страницы' }).click();
  await expect(pages).toHaveCount(0);
});

test('виртуальный список: в DOM только окно строк', async ({ page }) => {
  await openApp(page, 20_000);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(200);
  const count = () => tree(page).getByRole('treeitem').count();
  expect(await count()).toBeLessThanOrEqual(60);
  await tree(page).evaluate((el) => (el.scrollTop = 150 * 28));
  await expect(row(page, 'Экран 49')).toBeVisible();
  expect(await count()).toBeLessThanOrEqual(60);
  // Клавиатура: стрелки ведут фокус за пределы окна, список подкручивается, фокус
  // остаётся на своём слое (DOM-строки переиспользуются по месту).
  await tree(page).evaluate((el) => (el.scrollTop = 0));
  await row(page, 'Экран 199').focus();
  for (let i = 0; i < 70; i++) await page.keyboard.press('ArrowDown');
  await expect(page.locator(':focus')).toHaveAttribute('aria-label', 'Экран 129');
  await page.keyboard.press('Space');
  await expect(row(page, 'Экран 129')).toHaveAttribute('aria-selected', 'true');
  // Поиск считает worker: совпавшие и их предки.
  await page.getByRole('searchbox', { name: 'Поиск слоёв' }).fill('Слой 98');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(400);
});

/** Команды, которые интерфейс отправил ядру (типы), с момента вызова. */
async function recordSends(page: Page): Promise<void> {
  await page.evaluate(() => {
    const core = window.__ferrum!.core as unknown as { send(c: { type: string }): void };
    const w = window as unknown as { __sends: string[] };
    w.__sends = [];
    const send = core.send.bind(core);
    core.send = (c) => {
      w.__sends.push(c.type);
      send(c);
    };
  });
}
const sends = (page: Page) => page.evaluate(() => (window as unknown as { __sends: string[] }).__sends);
const warnings = (page: Page) => page.evaluate(() => (window as unknown as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? []);

test('фокус в поиске заранее строит кэш имён у ядра', async ({ page }) => {
  await openApp(page);
  await recordSends(page);
  const search = page.getByRole('searchbox', { name: 'Поиск слоёв' });
  await search.focus();
  await expect.poll(() => sends(page)).toContain('prepareSearch');
  expect((await sends(page)).filter((t) => t === 'filterLayers')).toHaveLength(0);
  expect(await warnings(page)).toEqual([]);
});

test('дерево слоёв: стрелки ходят по строкам и не уходят на холст', async ({ page }) => {
  await openApp(page);
  const before = await live(page);
  await row(page, 'Экран 9').focus();
  await page.keyboard.press('ArrowDown');
  await expect(page.locator(':focus')).toHaveAttribute('aria-label', 'Экран 8');
  await page.keyboard.press('ArrowUp');
  await expect(page.locator(':focus')).toHaveAttribute('aria-label', 'Экран 9');
  await page.keyboard.press('ArrowDown');
  // Выделенный фрейм стрелкой холста сдвинулся бы на 1 px — шаг отмены.
  await page.keyboard.press('Space');
  await expect(row(page, 'Экран 8')).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('ArrowRight');
  await expect(row(page, 'Экран 8')).toHaveAttribute('aria-expanded', 'true');
  await expect(tree(page).locator('[aria-level="2"]').first()).toBeVisible();
  await page.keyboard.press('ArrowLeft');
  await expect(row(page, 'Экран 8')).toHaveAttribute('aria-expanded', 'false');
  expect((await live(page)).steps).toBe(before.steps);
  // Enter и F2 — переименование; Esc возвращает фокус на строку.
  await page.keyboard.press('Enter');
  const input = tree(page).getByRole('textbox', { name: 'Имя слоя' });
  await expect(input).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(row(page, 'Экран 8')).toBeFocused();
  await page.keyboard.press('F2');
  await expect(input).toBeFocused();
  await input.fill('Шапка');
  await page.keyboard.press('Enter');
  await expect(row(page, 'Шапка')).toBeFocused();
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  // Delete — та же команда, что на холсте: удаляет выделенное, один шаг.
  await page.keyboard.press('Delete');
  await expect(row(page, 'Шапка')).toHaveCount(0);
  await liveWhen(page, `l.steps === ${before.steps + 2} && l.selectionCount === 0`);
  expect(await warnings(page)).toEqual([]);
});

test('100 000 узлов, раскрытый большой фрейм: в DOM только окно, окно просят раз в кадр', async ({ page }) => {
  test.setTimeout(180_000);
  await openApp(page, 100_000);
  const count = () => tree(page).getByRole('treeitem').count();
  const total = () => page.evaluate(() => window.__ferrum!.ui!.layers.total);
  await expect.poll(total).toBeGreaterThan(900);
  const closed = await total();
  await row(page, 'Экран 999').getByRole('button', { name: 'Развернуть' }).click();
  await expect.poll(total).toBeGreaterThan(closed + 50);
  expect(await count()).toBeLessThanOrEqual(60);
  // Прокрутка вглубь и обратно: строк в DOM не больше окна и запаса.
  for (const at of [40, 300, 700]) {
    await tree(page).evaluate((el, y) => (el.scrollTop = y), at * 28);
    await expect(page.locator('.fe-layer').first()).toBeAttached();
    await page.waitForTimeout(100);
    expect(await count()).toBeLessThanOrEqual(60);
  }
  // Пачка прокруток в одном кадре — один запрос окна и ни одного чтения вне measure.
  await recordSends(page);
  await page.evaluate(() => (window as unknown as { __ferrumWarnings?: string[] }).__ferrumWarnings?.splice(0));
  // Сколько замеров за пачку из 50 прокруток поставлено в планировщик.
  const measures = await tree(page).evaluate((el) => {
    const sch = (window.__ferrum as unknown as { scheduler: { measure(f: () => void): void } }).scheduler;
    const measure = sch.measure.bind(sch);
    let n = 0;
    sch.measure = (f) => {
      n++;
      measure(f);
    };
    for (let i = 1; i <= 50; i++) {
      el.scrollTop = 100 * 28 + i * 28;
      el.dispatchEvent(new Event('scroll'));
    }
    return new Promise<number>((done) => setTimeout(() => ((sch.measure = measure), done(n)), 300));
  });
  expect(measures).toBeLessThanOrEqual(3);
  expect((await sends(page)).filter((t) => t === 'layersWindow').length).toBeLessThanOrEqual(1);
  expect(await count()).toBeLessThanOrEqual(60);
  expect(await warnings(page)).toEqual([]);
});
