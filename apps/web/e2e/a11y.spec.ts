// Доступность: роли основных виджетов, имена у всех кнопок, контраст текста по
// токенам в обеих темах, кольцо фокуса, prefers-reduced-motion.

import { expect, test, type Page } from '@playwright/test';
import { openApp } from './helpers';

/** Контраст двух цветов `#rgb` / `#rrggbb` по WCAG (сборка сокращает `#ffffff`). */
function contrast(a: string, b: string): number {
  const lum = (text: string) => {
    const hex = text.length === 4 ? `#${[...text.slice(1)].map((c) => c + c).join('')}` : text;
    const [r, g, bl] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
    const f = (c: number) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
    return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(bl);
  };
  const [x, y] = [lum(a), lum(b)].sort((m, n) => n - m);
  return (x + 0.05) / (y + 0.05);
}

const token = (page: Page, name: string) =>
  page.evaluate((n) => getComputedStyle(document.documentElement).getPropertyValue(`--${n}`).trim(), name);

test('роли: тулбар, дерево, вкладки, список страниц, меню, диалог', async ({ page }) => {
  await openApp(page);
  await expect(page.getByRole('toolbar', { name: 'Инструменты' })).toBeVisible();
  await expect(page.getByRole('tree', { name: 'Слои' })).toBeVisible();
  await expect(page.getByRole('tablist', { name: 'Режим панели' })).toBeVisible();
  await expect(page.getByRole('listbox', { name: 'Страницы' })).toBeVisible();
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await expect(page.getByRole('menu', { name: 'Меню файла' })).toBeVisible();
  await page.getByRole('menuitem', { name: /Настройки/ }).click();
  await expect(page.getByRole('dialog', { name: 'Настройки' })).toBeVisible();
});

test('у каждой кнопки есть имя', async ({ page }) => {
  await openApp(page);
  await page.getByRole('treeitem', { name: 'Экран 9', exact: true }).click();
  const unnamed = await page.evaluate(() =>
    Array.from(document.querySelectorAll('button')).filter(
      (b) => !(b.getAttribute('aria-label') || b.textContent?.trim()),
    ).map((b) => b.outerHTML.slice(0, 80)),
  );
  expect(unnamed).toEqual([]);
});

for (const theme of ['dark', 'light'] as const) {
  test(`контраст текста и кольца фокуса по токенам (${theme})`, async ({ page }) => {
    await openApp(page, 0);
    await page.evaluate((t) => (document.documentElement.dataset.theme = t), theme);
    const t = Object.fromEntries(
      await Promise.all(
        ['ink', 'ink-muted', 'surface', 'surface-raised', 'surface-hover', 'canvas', 'select', 'oxide', 'on-oxide'].map(
          async (n) => [n, await token(page, n)] as const,
        ),
      ),
    );
    for (const bg of ['surface', 'surface-raised', 'surface-hover']) {
      expect(contrast(t.ink, t[bg]), `ink на ${bg}`).toBeGreaterThanOrEqual(4.5);
      expect(contrast(t['ink-muted'], t[bg]), `ink-muted на ${bg}`).toBeGreaterThanOrEqual(4.5);
    }
    expect(contrast(t.ink, t.canvas)).toBeGreaterThanOrEqual(4.5);
    // Кольцо фокуса select — не ниже 3:1 на поверхностях.
    for (const bg of ['surface', 'surface-raised']) expect(contrast(t.select, t[bg])).toBeGreaterThanOrEqual(3);
    expect(contrast(t['on-oxide'], t.oxide)).toBeGreaterThanOrEqual(4.5);
  });
}

test('prefers-reduced-motion: без анимаций', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await openApp(page);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const anim = await page.getByRole('menu').evaluate((el) => getComputedStyle(el).animationName);
  expect(anim).toBe('none');
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Меню файла' }).click();
  expect(await page.getByRole('menu').evaluate((el) => getComputedStyle(el).animationName)).toBe('fe-pop');
});

// --- Поведение составных виджетов (после доработок задач 6–12) ---------------

test('тулбар и вкладки: одна точка Tab, стрелки переводят фокус', async ({ page }) => {
  await openApp(page);
  for (const [role, name] of [['toolbar', 'Инструменты'], ['tablist', 'Режим панели']] as const) {
    const widget = page.getByRole(role, { name });
    const items = widget.locator(role === 'toolbar' ? 'button' : '[role="tab"]');
    const stops = await items.evaluateAll((els) => els.filter((e) => (e as HTMLElement).tabIndex === 0).length);
    expect(stops, `точек Tab в ${role}`).toBe(1);
    await items.first().focus();
    await page.keyboard.press('ArrowRight');
    const moved = await items.evaluateAll((els) => els.indexOf(document.activeElement as HTMLElement));
    expect(moved, `стрелка вправо в ${role}`).toBe(1);
    // Фокус и точка Tab переехали вместе.
    expect(await items.nth(1).evaluate((e) => (e as HTMLElement).tabIndex)).toBe(0);
    await page.keyboard.press('End');
    expect(await items.evaluateAll((els) => els.indexOf(document.activeElement as HTMLElement))).toBe((await items.count()) - 1);
    await page.keyboard.press('Home');
    expect(await items.evaluateAll((els) => els.indexOf(document.activeElement as HTMLElement))).toBe(0);
  }
  // Вкладка выбрана ровно одна.
  const tabs = page.getByRole('tablist', { name: 'Режим панели' }).getByRole('tab');
  expect(await tabs.evaluateAll((els) => els.filter((e) => e.getAttribute('aria-selected') === 'true').length)).toBe(1);
});

test('дерево слоёв: одна точка Tab, множественный выбор, уровни и состояния', async ({ page }) => {
  await openApp(page);
  const tree = page.getByRole('tree', { name: 'Слои' });
  await expect(tree).toHaveAttribute('aria-multiselectable', 'true');
  const items = tree.getByRole('treeitem');
  expect(await items.evaluateAll((els) => els.filter((e) => (e as HTMLElement).tabIndex === 0).length)).toBe(1);
  await items.first().focus();
  await page.keyboard.press('ArrowDown');
  // Фокус остаётся в дереве, точка Tab — на строке с фокусом.
  const focused = tree.locator('[role="treeitem"]:focus');
  await expect(focused).toHaveCount(1);
  expect(await focused.evaluate((e) => (e as HTMLElement).tabIndex)).toBe(0);
  expect(await items.evaluateAll((els) => els.filter((e) => (e as HTMLElement).tabIndex === 0).length)).toBe(1);
  // Каждая строка: уровень, выбранность; у фреймов — состояние раскрытия.
  const bad = await items.evaluateAll((els) =>
    els
      .filter((e) => !e.getAttribute('aria-level') || !['true', 'false'].includes(e.getAttribute('aria-selected') ?? ''))
      .map((e) => e.outerHTML.slice(0, 80)),
  );
  expect(bad).toEqual([]);
  await expect(tree.getByRole('treeitem', { name: 'Экран 9', exact: true })).toHaveAttribute('aria-expanded', 'false');
  // Кнопки строки вне порядка Tab: их достаёт мышь и клавиши дерева.
  const rowButtons = await tree.locator('[role="treeitem"] button').evaluateAll((els) => els.filter((e) => (e as HTMLElement).tabIndex >= 0).length);
  expect(rowButtons).toBe(0);
});

test('диалог настроек: модальный, фокус внутри, Esc закрывает', async ({ page }) => {
  await openApp(page);
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menuitem', { name: /Настройки/ }).click();
  const dialog = page.getByRole('dialog', { name: 'Настройки' });
  await expect(dialog).toHaveAttribute('aria-modal', 'true');
  await expect.poll(() => dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true);
  // Tab не уводит фокус из диалога.
  for (let i = 0; i < 12; i++) await page.keyboard.press('Tab');
  expect(await dialog.evaluate((el) => el.contains(document.activeElement))).toBe(true);
  // Разделы — вкладки с одной точкой Tab.
  const tabs = dialog.getByRole('tablist').getByRole('tab');
  expect(await tabs.evaluateAll((els) => els.filter((e) => (e as HTMLElement).tabIndex === 0).length)).toBe(1);
  expect(await tabs.evaluateAll((els) => els.filter((e) => e.getAttribute('aria-selected') === 'true').length)).toBe(1);
  // Кнопки диалога тоже с именами.
  const unnamed = await dialog.getByRole('button').evaluateAll((els) =>
    els.filter((b) => !(b.getAttribute('aria-label') || b.textContent?.trim())).map((b) => b.outerHTML.slice(0, 80)),
  );
  expect(unnamed).toEqual([]);
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
});

test('палитра цвета: диалог без модальности, ползунки со значениями, Esc закрывает', async ({ page }) => {
  await openApp(page);
  await page.getByRole('treeitem', { name: 'Экран 9', exact: true }).click();
  await page.getByRole('button', { name: 'Цвет заливки' }).click();
  const picker = page.getByRole('dialog', { name: 'Цвет заливки' });
  await expect(picker).toBeVisible();
  // Непрозрачный фон страницы остаётся доступным: палитра не модальна.
  await expect(picker).not.toHaveAttribute('aria-modal', 'true');
  for (const name of ['Насыщенность и яркость', 'Тон', 'Прозрачность']) {
    const slider = picker.getByRole('slider', { name });
    await expect(slider).toHaveAttribute('aria-valuenow', /^\d+$/);
    await expect(slider).toHaveAttribute('aria-valuemin', '0');
    expect(await slider.evaluate((e) => (e as HTMLElement).tabIndex)).toBe(0);
  }
  const unnamed = await picker.getByRole('button').evaluateAll((els) =>
    els.filter((b) => !(b.getAttribute('aria-label') || b.textContent?.trim())).map((b) => b.outerHTML.slice(0, 80)),
  );
  expect(unnamed).toEqual([]);
  await page.keyboard.press('Escape');
  await expect(picker).toHaveCount(0);
});

test('prefers-reduced-motion: ни у одного элемента нет анимации и перехода', async ({ page }) => {
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await openApp(page);
  await page.getByRole('treeitem', { name: 'Экран 9', exact: true }).click();
  await page.getByRole('button', { name: 'Меню файла' }).click();
  const scan = () =>
    page.evaluate(() =>
      Array.from(document.querySelectorAll<HTMLElement>('body *'))
        .filter((el) => {
          const s = getComputedStyle(el);
          const dur = (v: string) => v.split(',').some((x) => parseFloat(x) > 0.001);
          return (s.animationName !== 'none' && dur(s.animationDuration)) || dur(s.transitionDuration);
        })
        .map((el) => el.className.toString().slice(0, 40)),
    );
  // Открыта анимированная поверхность (меню) и палитра — и всё равно ничего не движется.
  await page.getByRole('menu').waitFor();
  expect(await scan()).toEqual([]);
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'Цвет заливки' }).click();
  await expect(page.getByRole('dialog', { name: 'Цвет заливки' })).toBeVisible();
  expect(await scan()).toEqual([]);
  // Положительный контроль: без запроса уменьшить движение та же сканировка находит анимацию.
  await page.keyboard.press('Escape');
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menu').waitFor();
  expect((await scan()).length, 'сканировка видит анимацию меню').toBeGreaterThan(0);
});
