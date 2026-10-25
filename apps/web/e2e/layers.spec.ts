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
  // Строка ушла внутрь свёрнутого фрейма (ждём обновления списка, потом раскрываем).
  await expect(row(page, 'Экран 7')).toHaveCount(0);
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
  await page.getByRole('button', { name: 'Страницы', exact: true }).click();
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

// ——— Fix round 1 ———

test('смена страницы сбрасывает поиск и прокрутку', async ({ page }) => {
  await openApp(page, 20_000);
  const total = () => page.evaluate(() => window.__ferrum!.ui!.layers.total);
  const search = page.getByRole('searchbox', { name: 'Поиск слоёв' });
  await expect.poll(total).toBe(200);
  await tree(page).evaluate((el) => (el.scrollTop = 3000));
  await search.fill('Слой 98');
  await expect.poll(total).toBe(400);
  await page.getByRole('button', { name: 'Добавить страницу' }).click();
  const pages = page.getByRole('listbox', { name: 'Страницы' });
  await expect(pages.getByRole('option', { name: /Страница 2/ })).toHaveAttribute('aria-selected', 'true');
  await expect(search).toHaveValue('');
  await pages.getByRole('option', { name: /Страница 1/ }).click();
  await expect.poll(total).toBe(200);
  await expect(search).toHaveValue('');
  expect(await tree(page).evaluate((el) => el.scrollTop)).toBe(0);
});

test('в дереве одна точка Tab, дерево помечено как мультивыбор', async ({ page }) => {
  await openApp(page);
  await expect(tree(page)).toHaveAttribute('aria-multiselectable', 'true');
  await row(page, 'Экран 9').click();
  await row(page, 'Экран 8').click({ modifiers: ['ControlOrMeta'] });
  await row(page, 'Экран 7').click({ modifiers: ['ControlOrMeta'] });
  await liveWhen(page, 'l.selectionCount === 3');
  expect(await tree(page).locator('[role="treeitem"][tabindex="0"]').count()).toBe(1);
  await page.getByRole('searchbox', { name: 'Поиск слоёв' }).focus();
  await page.keyboard.press('Tab');
  await expect(page.locator(':focus')).toHaveAttribute('role', 'treeitem');
  await page.keyboard.press('Tab');
  expect(await page.evaluate(() => !!document.activeElement?.closest('[role="tree"]'))).toBe(false);
});

test('переименование не теряется, когда окно строк сдвинулось', async ({ page }) => {
  await openApp(page, 20_000);
  const before = await live(page);
  await row(page, 'Экран 199').dblclick();
  const input = tree(page).getByRole('textbox', { name: 'Имя слоя' });
  await input.fill('Новое имя');
  await tree(page).hover();
  await page.mouse.wheel(0, 4000);
  await expect(row(page, 'Экран 199')).toHaveCount(0);
  await liveWhen(page, `l.steps === ${before.steps + 1}`);
  await expect(input).toHaveCount(0);
  await page.getByRole('searchbox', { name: 'Поиск слоёв' }).focus();
  await tree(page).evaluate((el) => (el.scrollTop = 0));
  await expect(row(page, 'Новое имя')).toBeVisible();
  await expect(tree(page).getByRole('textbox')).toHaveCount(0);
  await expect(page.getByRole('searchbox', { name: 'Поиск слоёв' })).toBeFocused();
});

test('щелчок по другой строке при переименовании применяет имя и выделяет её', async ({ page }) => {
  await openApp(page);
  const before = await live(page);
  await row(page, 'Экран 9').dblclick();
  await tree(page).getByRole('textbox', { name: 'Имя слоя' }).fill('Первая');
  await row(page, 'Экран 7').click();
  await expect(row(page, 'Первая')).toBeVisible();
  await expect(row(page, 'Экран 7')).toHaveAttribute('aria-selected', 'true');
  await liveWhen(page, `l.steps === ${before.steps + 1} && l.selectionCount === 1`);
});

test('перетаскивание: Esc отменяет, невалидная цель без индикатора, невыделенная строка выделяется', async ({ page }) => {
  await openApp(page);
  const before = await live(page);
  const from = (await row(page, 'Экран 9').boundingBox())!;
  const to = (await row(page, 'Экран 5').boundingBox())!;
  // Esc во время жеста.
  await page.mouse.move(from.x + 60, from.y + 14);
  await page.mouse.down();
  await page.mouse.move(from.x + 60, to.y + 25, { steps: 6 });
  await expect(row(page, 'Экран 5')).toHaveClass(/fe-layer--drop-below/);
  await page.keyboard.press('Escape');
  await expect(tree(page).locator('[class*="fe-layer--drop-"]')).toHaveCount(0);
  await page.mouse.up();
  expect((await names(page)).slice(0, 2)).toEqual(['Экран 9', 'Экран 8']);
  expect((await live(page)).steps).toBe(before.steps);
  // Невыделенная строка при перетаскивании выделяется и переносится сама.
  await row(page, 'Экран 3').click();
  await expect(row(page, 'Экран 3')).toHaveAttribute('aria-selected', 'true');
  await expect(row(page, 'Экран 9')).toHaveAttribute('aria-selected', 'false');
  const a = (await row(page, 'Экран 9').boundingBox())!;
  const b = (await row(page, 'Экран 8').boundingBox())!;
  await page.mouse.move(a.x + 60, a.y + 14);
  await page.mouse.down();
  await page.mouse.move(a.x + 60, b.y + 25, { steps: 6 });
  await page.mouse.up();
  await expect.poll(() => names(page).then((n) => n.slice(0, 2))).toEqual(['Экран 8', 'Экран 9']);
  await expect(row(page, 'Экран 9')).toHaveAttribute('aria-selected', 'true');
  await expect(row(page, 'Экран 3')).toHaveAttribute('aria-selected', 'false');
  const steps = (await liveWhen(page, `l.steps === ${before.steps + 1}`)).steps;
  // Цель — потомок самой строки: индикатора нет, перенос не происходит.
  await row(page, 'Экран 9').getByRole('button', { name: 'Развернуть' }).click();
  const child = tree(page).locator('[aria-level="2"]').first();
  await expect(child).toBeVisible();
  const p = (await row(page, 'Экран 9').boundingBox())!;
  const c = (await child.boundingBox())!;
  await page.mouse.move(p.x + 60, p.y + 14);
  await page.mouse.down();
  await page.mouse.move(p.x + 60, c.y + 14, { steps: 6 });
  await expect(tree(page).locator('[class*="fe-layer--drop-"]')).toHaveCount(0);
  await page.mouse.up();
  await page.waitForTimeout(200);
  expect((await live(page)).steps).toBe(steps);
});

// ——— Fix round 2 ———

test('событие страниц во время переименования не сбрасывает набранное', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Добавить страницу' }).click();
  const pages = page.getByRole('listbox', { name: 'Страницы' });
  await expect(pages.getByRole('option')).toHaveCount(2);
  await pages.getByRole('option', { name: /Страница 2/ }).dblclick();
  const input = pages.getByRole('textbox', { name: 'Имя страницы' });
  await input.fill('Набрано');
  // Событие страниц, пока поле открыто: переименовать другую страницу через ядро.
  await page.evaluate(() => {
    const p = window.__ferrum!.ui!.pages.list[0];
    window.__ferrum!.core.send({ type: 'renamePage', id: p.id, name: 'Первая' });
  });
  await expect(pages.getByRole('option', { name: /Первая/ })).toBeVisible();
  await expect(input).toHaveValue('Набрано');
  await input.press('Enter');
  await expect(pages.getByRole('option', { name: /Набрано/ })).toBeVisible();
});

test('событие слоёв во время переименования не сбрасывает набранное', async ({ page }) => {
  await openApp(page);
  await row(page, 'Экран 9').dblclick();
  const input = tree(page).getByRole('textbox', { name: 'Имя слоя' });
  await input.fill('Набрано');
  const seq = await page.evaluate(() => window.__ferrum!.ui!.layers.rows.length);
  expect(seq).toBeGreaterThan(0);
  // Выделение другого слоя через ядро: приходит событие слоёв с новыми объектами.
  await page.evaluate(() => {
    const r = window.__ferrum!.ui!.layers.rows[3];
    window.__ferrum!.core.send({ type: 'select', ids: [r.id], mode: 'replace' });
  });
  await expect(row(page, 'Экран 6')).toHaveAttribute('aria-selected', 'true');
  await expect(input).toHaveValue('Набрано');
  await input.press('Enter');
  await expect(row(page, 'Набрано')).toBeVisible();
});

// ——— Итоговая волна исправлений ———

test('во время перетаскивания строк клавиши не действуют; бросок переносит взятые', async ({ page }) => {
  await openApp(page);
  await row(page, 'Экран 9').click();
  const before = await liveWhen(page, 'l.selectionCount === 1');
  const from = (await row(page, 'Экран 9').boundingBox())!;
  const to = (await row(page, 'Экран 7').boundingBox())!;
  await page.mouse.move(from.x + 60, from.y + 14);
  await page.mouse.down();
  await page.mouse.move(from.x + 60, to.y + 25, { steps: 6 });
  await expect(row(page, 'Экран 7')).toHaveClass(/fe-layer--drop-below/);
  // Выделить всё и удалить — во время жеста ничего не делают.
  await page.keyboard.press('ControlOrMeta+a');
  await page.keyboard.press('Delete');
  await page.keyboard.press('Backspace');
  await page.waitForTimeout(200);
  let now = await live(page);
  expect([now.selectionCount, now.nodes, now.steps]).toEqual([1, before.nodes, before.steps]);
  await page.mouse.up();
  await expect.poll(() => names(page).then((n) => n.slice(0, 3))).toEqual(['Экран 8', 'Экран 7', 'Экран 9']);
  now = await liveWhen(page, `l.steps === ${before.steps + 1}`);
  expect([now.selectionCount, now.nodes]).toEqual([1, before.nodes]);
  // После броска клавиши снова идут холсту.
  await page.locator('canvas').focus();
  await page.keyboard.press('Delete');
  await liveWhen(page, `l.steps === ${before.steps + 2} && l.selectionCount === 0`);
});

test('имя страницы: предел длины, фокус возвращается к странице после Enter и Esc', async ({ page }) => {
  await openApp(page);
  const pages = page.getByRole('listbox', { name: 'Страницы' });
  const option = pages.getByRole('option', { name: /Страница 1/ });
  await option.focus();
  await page.keyboard.press('F2');
  const input = pages.getByRole('textbox', { name: 'Имя страницы' });
  await expect(input).toBeFocused();
  await expect(input).toHaveAttribute('maxlength', '256');
  await input.fill('Первая');
  await page.keyboard.press('Enter');
  const renamed = pages.getByRole('option', { name: /Первая/ });
  await expect(renamed).toBeFocused();
  await page.keyboard.press('F2');
  await input.fill('Не то');
  await page.keyboard.press('Escape');
  await expect(renamed).toBeFocused();
});

test('переименование слоя применяется, когда переходят на другую страницу', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Добавить страницу' }).click();
  const pages = page.getByRole('listbox', { name: 'Страницы' });
  await pages.getByRole('option', { name: /Страница 1/ }).click();
  await row(page, 'Экран 9').dblclick();
  await tree(page).getByRole('textbox', { name: 'Имя слоя' }).fill('Главная');
  await pages.getByRole('option', { name: /Страница 2/ }).click();
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(0);
  await pages.getByRole('option', { name: /Страница 1/ }).click();
  await expect(row(page, 'Главная')).toBeVisible();
});

// ——— Остатки повторного ревью ———

for (const stop of ['lostpointercapture', 'blur', 'buttons'] as const) {
  test(`застрявшее перетаскивание строки (${stop}) кончается: клавиши снова работают, ничего не перенесено`, async ({ page }) => {
    await openApp(page);
    await row(page, 'Экран 9').click();
    const before = await liveWhen(page, 'l.selectionCount === 1');
    const from = (await row(page, 'Экран 9').boundingBox())!;
    const to = (await row(page, 'Экран 7').boundingBox())!;
    await page.mouse.move(from.x + 60, from.y + 14);
    await page.mouse.down();
    await page.mouse.move(from.x + 60, to.y + 25, { steps: 6 });
    await expect(row(page, 'Экран 7')).toHaveClass(/fe-layer--drop-below/);
    // Захват потерян без pointerup (Cmd+Tab посреди жеста, системное окно).
    await page.evaluate(
      ({ stop, y }) => {
        const el = document.querySelector<HTMLElement>('[role="treeitem"][aria-label="Экран 9"]')!;
        if (stop === 'lostpointercapture') el.dispatchEvent(new PointerEvent('lostpointercapture', { pointerId: 1, bubbles: true }));
        else if (stop === 'blur') window.dispatchEvent(new Event('blur'));
        else el.dispatchEvent(new PointerEvent('pointermove', { pointerId: 1, isPrimary: true, buttons: 0, clientY: y, bubbles: true }));
      },
      { stop, y: to.y + 25 },
    );
    await expect(tree(page).locator('[class*="fe-layer--drop-"]')).toHaveCount(0);
    // Клавиши снова доходят: Ctrl+A выделяет всё.
    await page.keyboard.press('ControlOrMeta+a');
    await liveWhen(page, 'l.selectionCount > 1');
    await page.mouse.up();
    await page.waitForTimeout(200);
    expect((await names(page)).slice(0, 3)).toEqual(['Экран 9', 'Экран 8', 'Экран 7']);
    expect((await live(page)).steps).toBe(before.steps);
  });
}
