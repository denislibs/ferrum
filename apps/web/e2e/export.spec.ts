// Экспорт SVG и PDF (подсистема 3, этап 5): формат в разделе «Экспорт» панели,
// у SVG — «Текст кривыми» и «Атрибут id», масштаб — только у PNG. SVG разбирается
// браузером как SVG того же размера, PDF — по страницам (`/MediaBox`) и шрифтам.
// Экспорт идёт в worker'е: основной поток только скачивает готовые байты.

import { expect, test, type Download, type Page } from '@playwright/test';
import { openApp } from './helpers';

test.afterEach(async ({ page }) => {
  expect(await page.evaluate(() => (window as { __ferrumWarnings?: string[] }).__ferrumWarnings ?? [])).toEqual([]);
});

const selectRow = (page: Page, name: string) =>
  page.getByRole('tree', { name: 'Слои' }).getByRole('treeitem', { name, exact: true }).click();

async function bytesOf(d: Download): Promise<Buffer> {
  return Buffer.concat(await (await d.createReadStream()).toArray());
}

async function exportAs(page: Page, format: 'PNG' | 'SVG' | 'PDF'): Promise<Download> {
  await page.getByRole('combobox', { name: 'Формат экспорта' }).selectOption({ label: format });
  const download = page.waitForEvent('download');
  await page.getByRole('button', { name: /Экспортировать/ }).click();
  return download;
}

test('SVG выделенного фрейма: размер, «Текст кривыми» и «Атрибут id» @firefox', async ({ page }) => {
  await openApp(page, 100);
  await selectRow(page, 'Экран 0');
  await page.getByRole('combobox', { name: 'Формат экспорта' }).selectOption({ label: 'SVG' });
  // У SVG масштаба нет, есть свои настройки (как в Figma).
  await expect(page.getByRole('radiogroup', { name: 'Масштаб экспорта' })).toBeHidden();
  await expect(page.getByRole('checkbox', { name: 'Текст кривыми' })).toBeChecked();
  await page.getByRole('checkbox', { name: 'Атрибут id' }).check();
  const file = await exportAs(page, 'SVG');
  expect(file.suggestedFilename()).toBe('Экран 0.svg');
  const text = (await bytesOf(file)).toString('utf8');
  const parsed = await page.evaluate((t) => {
    const doc = new DOMParser().parseFromString(t, 'image/svg+xml');
    const root = doc.documentElement;
    return {
      error: !!doc.querySelector('parsererror'),
      size: [root.getAttribute('width'), root.getAttribute('height')],
      clip: doc.querySelectorAll('clipPath').length,
      ids: doc.querySelectorAll('[id="Экран_0"]').length,
    };
  }, text);
  expect(parsed).toEqual({ error: false, size: ['1440', '1024'], clip: expect.any(Number), ids: 1 });
  expect(parsed.clip).toBeGreaterThan(0);
  await expect(page.getByRole('status').filter({ hasText: 'Экспортировано: Экран 0.svg' })).toBeVisible();
});

test('PDF без выделения — фрейм страницей своего размера; с выделением — выделенный', async ({ page }) => {
  await openApp(page, 100);
  let file = await exportAs(page, 'PDF');
  expect(file.suggestedFilename()).toBe('Страница 1.pdf');
  let pdf = (await bytesOf(file)).toString('latin1');
  expect(pdf.startsWith('%PDF-')).toBe(true);
  const pages = (s: string) => s.match(/\/Type\/Page\//g)?.length ?? 0;
  const frames = await page.evaluate(() => window.__ferrum!.ui!.layers.rows.filter((r) => r.depth === 0 && r.kind === 'frame').length);
  expect(pages(pdf)).toBe(frames);
  expect(pdf).toContain('/MediaBox[0 0 1440 1024]');
  await selectRow(page, 'Экран 0');
  file = await exportAs(page, 'PDF');
  expect(file.suggestedFilename()).toBe('Экран 0.pdf');
  pdf = (await bytesOf(file)).toString('latin1');
  expect(pages(pdf)).toBe(1);
  // Назад к PNG — снова масштаб и прежний файл.
  file = await exportAs(page, 'PNG');
  expect(file.suggestedFilename()).toBe('Экран 0@1x.png');
  await expect(page.getByRole('radiogroup', { name: 'Масштаб экспорта' })).toBeVisible();
});
