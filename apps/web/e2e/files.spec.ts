// Экран файлов и хранилище (этап 6, часть 1): пустой экран, новый файл, превью и
// «Изменён только что», переименование, Ctrl+S, импорт, корзина, поиск, вид
// сетка/список, назад браузера, вторая вкладка, неизвестный и испорченный файл,
// клавиатура, отказ записи; экран не запускает ядро и не даёт длинных задач на
// 1000 записях.

import { expect, test, type Page } from '@playwright/test';
import { appScreen, drag, expectCore, live, liveWhen } from './helpers';

/** Тестовый режим и учёт worker'ов: какие созданы и какие остановлены. */
async function setup(page: Page): Promise<void> {
  await page.addInitScript(() => {
    (window as { __ferrumTest?: boolean }).__ferrumTest = true;
    const w = window as unknown as { __workers: { url: string; terminated: boolean }[] };
    w.__workers = [];
    const Base = window.Worker;
    window.Worker = class extends Base {
      constructor(url: string | URL, opts?: WorkerOptions) {
        super(url, opts);
        const rec = { url: String(url), terminated: false };
        w.__workers.push(rec);
        const terminate = this.terminate.bind(this);
        this.terminate = () => {
          rec.terminated = true;
          terminate();
        };
      }
    };
    // Выбор файла — через <input type=file> (его ловит filechooser Playwright).
    delete (window as unknown as { showOpenFilePicker?: unknown }).showOpenFilePicker;
  });
}

const workers = (page: Page) =>
  page.evaluate(() => (window as unknown as { __workers: { url: string; terminated: boolean }[] }).__workers);

async function openFiles(page: Page, url = '/'): Promise<void> {
  await setup(page);
  await page.goto(url);
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
}

const files = (page: Page) => page.getByRole('main').getByRole('listbox');
const card = (page: Page, name: string) => files(page).getByRole('option', { name, exact: true });

/** Редактор открыт и ядро готово. */
async function editorReady(page: Page): Promise<void> {
  await page.waitForFunction(() => window.__ferrum?.ready === true && window.__ferrum.ui?.ready === true, null, { timeout: 90_000 });
  await expectCore(page);
  await page.waitForFunction(() => (window.__ferrum!.core!.live?.frames ?? 0) > 0);
}

/** Прямоугольник инструментом R на пустом холсте. */
async function drawRect(page: Page): Promise<void> {
  const before = (await live(page)).nodes;
  await page.locator('canvas').click({ position: { x: 10, y: 10 } });
  await page.keyboard.press('KeyR');
  const l = await live(page);
  await drag(page, await appScreen(page, l, l.cameraX - 50, l.cameraY - 40), 100, 80);
  await liveWhen(page, `l.nodes === ${before + 1}`);
}

const fileMenu = async (page: Page, item: string) => {
  await page.getByRole('button', { name: 'Меню файла' }).click();
  await page.getByRole('menu', { name: 'Меню файла' }).getByRole('menuitem', { name: item }).click();
};

const fileId = (page: Page) => /#\/file\/([\w-]+)$/.exec(page.url())?.[1] ?? '';

/** Записи в хранилище — через тестовый хук (тот же путь, что у экрана файлов). */
const create = async (page: Page, name: string) => {
  const id = await page.evaluate((n) => window.__ferrumFiles!.store.create(n).then((m) => m.id), name);
  // Следующая запись — позже по времени: порядок «новые сверху» однозначен.
  await page.waitForTimeout(5);
  return id;
};

test('пустой экран → «Новый файл» → прямоугольник → «Все файлы»: карточка с превью и «Изменён только что»', async ({ page }) => {
  await openFiles(page);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await expect(page.getByText('Здесь пока нет файлов')).toBeVisible();
  // Экран файлов не запускает ядро: только worker хранилища.
  expect((await workers(page)).map((w) => w.url).filter((u) => u.includes('core.worker'))).toEqual([]);
  expect((await workers(page)).some((w) => w.url.includes('io.worker'))).toBe(true);
  await page.getByRole('main').getByRole('button', { name: 'Новый файл' }).first().click();
  await expect(page).toHaveURL(/#\/file\/[\w-]+$/);
  await editorReady(page);
  const id = fileId(page);
  await drawRect(page);
  await fileMenu(page, 'Все файлы');
  await expect(page).toHaveURL(/#\/$/);
  const c = card(page, 'Без названия');
  await expect(c).toBeVisible();
  await expect(c).toContainText('Изменён только что');
  // Уход сохранил файл и остановил ядро.
  const core = (await workers(page)).filter((w) => w.url.includes('core.worker'));
  expect(core.map((w) => w.terminated)).toEqual([true]);
  // Превью — картинка не одного цвета (прямоугольник на прозрачном фоне).
  const img = c.locator('img');
  await expect(img).toHaveAttribute('src', /^blob:/);
  const colours = await img.evaluate(async (el: HTMLImageElement) => {
    await el.decode();
    const bmp = await createImageBitmap(await (await fetch(el.src)).blob());
    const cv = new OffscreenCanvas(bmp.width, bmp.height);
    const ctx = cv.getContext('2d')!;
    ctx.drawImage(bmp, 0, 0);
    const d = ctx.getImageData(0, 0, bmp.width, bmp.height).data;
    const set = new Set<string>();
    for (let i = 0; i < d.length; i += 4 * 97) set.add(`${d[i]},${d[i + 1]},${d[i + 2]},${d[i + 3]}`);
    return { size: [bmp.width, bmp.height], distinct: set.size };
  });
  expect(colours.size).toEqual([480, 320]);
  expect(colours.distinct).toBeGreaterThan(1);
  // Запись в хранилище — с байтами и тем же id.
  const meta = await page.evaluate((i) => window.__ferrumFiles!.store.get(i), id);
  expect(meta?.size).toBeGreaterThan(0);
  expect(meta?.thumbVersion).toBeGreaterThan(0);
});

test('переименование карточки сохраняется после перезагрузки; в редакторе — то же имя', async ({ page }) => {
  await openFiles(page);
  await create(page, 'Макет');
  const c = card(page, 'Макет');
  await c.locator('.fe-card__name').dblclick();
  const input = page.getByRole('textbox', { name: 'Имя файла' });
  await expect(input).toBeFocused();
  await input.fill('Лендинг');
  await input.press('Enter');
  await expect(card(page, 'Лендинг')).toBeVisible();
  await page.reload();
  await expect(card(page, 'Лендинг')).toBeVisible();
  await expect(card(page, 'Макет')).toHaveCount(0);
  // F2 на карточке тоже переименовывает; Esc — без правки.
  await card(page, 'Лендинг').focus();
  await page.keyboard.press('F2');
  await page.getByRole('textbox', { name: 'Имя файла' }).fill('Не то');
  await page.keyboard.press('Escape');
  await expect(card(page, 'Лендинг')).toBeVisible();
  // Открыть: имя документа — имя файла.
  await card(page, 'Лендинг').click();
  await editorReady(page);
  await expect(page.locator('.fe-panel-head__name')).toHaveText('Лендинг');
  // Переименование в редакторе обновляет запись.
  await page.locator('.fe-panel-head__name').dblclick();
  await page.getByRole('textbox', { name: 'Имя файла' }).fill('Лендинг v2');
  await page.getByRole('textbox', { name: 'Имя файла' }).press('Enter');
  const id = fileId(page);
  await expect.poll(() => page.evaluate((i) => window.__ferrumFiles!.store.get(i).then((m) => m?.name), id)).toBe('Лендинг v2');
});

test('Ctrl+S сохраняет: перезагрузка файла показывает прямоугольник', async ({ page }) => {
  await setup(page);
  await page.goto('/#/new');
  await editorReady(page);
  const id = fileId(page);
  expect(id).not.toBe('');
  await drawRect(page);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.uiEvents!.some((e) => e.type === 'saveRequested'))).toBe(true);
  // Снимок — сразу по сочетанию, а не через 2 с отложенного сохранения.
  await expect
    .poll(() => page.evaluate(() => window.__ferrum!.uiEvents!.some((e) => e.type === 'snapshot')), { timeout: 1500 })
    .toBe(true);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  await page.reload();
  await editorReady(page);
  expect(fileId(page)).toBe(id);
  await liveWhen(page, 'l.nodes === 1');
  expect(await page.evaluate(() => window.__ferrum!.ui!.layers.total)).toBe(1);
  expect(await page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
});

test('отложенное сохранение через 2 с после правки', async ({ page }) => {
  await setup(page);
  await page.goto('/#/new');
  await editorReady(page);
  const id = fileId(page);
  await drawRect(page);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified), { timeout: 10_000 }).toBe(false);
  const meta = await page.evaluate((i) => window.__ferrumFiles!.store.get(i), id);
  expect(meta?.size).toBeGreaterThan(0);
});

test('«Импорт…» открывает файл .ferrum; испорченный — сообщение, запись остаётся', async ({ page }) => {
  await setup(page);
  await page.goto('/?nodes=300#/new');
  await editorReady(page);
  // Байты файла — «Сохранить копию на диск…»: признак «изменён» она не снимает.
  const download = page.waitForEvent('download');
  await fileMenu(page, 'Сохранить копию на диск…');
  const bytes = Buffer.concat(await (await (await download).createReadStream()).toArray());
  await fileMenu(page, 'Все файлы');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  const chooser = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Импорт…' }).click();
  await (await chooser).setFiles({ name: 'проект.ferrum', mimeType: 'application/octet-stream', buffer: bytes });
  await editorReady(page);
  await liveWhen(page, 'l.nodes === 300');
  await expect(page.locator('.fe-panel-head__name')).toHaveText('проект');
  // Испорченный файл: экран файлов с сообщением, запись не удалена.
  await fileMenu(page, 'Все файлы');
  const bad = page.waitForEvent('filechooser');
  await page.getByRole('button', { name: 'Импорт…' }).click();
  await (await bad).setFiles({ name: 'битый.ferrum', mimeType: 'application/octet-stream', buffer: Buffer.from('не ferrum') });
  await expect(page.locator('.fe-toast', { hasText: /^Ошибка:\s*Не удалось открыть файл: / })).toHaveCount(1);
  await expect(page).toHaveURL(/#\/$/);
  await expect(card(page, 'битый')).toBeVisible();
  await expect(card(page, 'проект')).toBeVisible();
});

test('удалить → корзина → восстановить; удалить навсегда и очистить корзину — с подтверждением', async ({ page }) => {
  await openFiles(page);
  const id = await create(page, 'Черновик');
  await create(page, 'Второй');
  // Список перестроился под обе записи — щёлкаем уже по своей карточке.
  await expect(files(page).getByRole('option')).toHaveCount(2);
  await card(page, 'Черновик').click({ button: 'right' });
  await page.getByRole('menu', { name: 'Действия с файлом' }).getByRole('menuitem', { name: 'Удалить' }).click();
  await expect(card(page, 'Черновик')).toHaveCount(0);
  await page.getByRole('listbox', { name: 'Разделы' }).getByRole('option', { name: 'Корзина' }).click();
  await expect(page).toHaveURL(/#\/trash$/);
  await expect(card(page, 'Черновик')).toContainText('Удалён только что');
  await card(page, 'Черновик').locator('.fe-card__more').click();
  await page.getByRole('menuitem', { name: 'Восстановить' }).click();
  await expect(page.getByText('Корзина пуста')).toBeVisible();
  await page.getByRole('listbox', { name: 'Разделы' }).getByRole('option', { name: 'Недавние' }).click();
  await expect(card(page, 'Черновик')).toBeVisible();
  // Удалить навсегда: «Отмена» оставляет, «Удалить» — запись пропадает из хранилища.
  await card(page, 'Черновик').focus();
  await page.keyboard.press('Delete');
  await expect(card(page, 'Черновик')).toHaveCount(0);
  await page.goto('/#/trash');
  await card(page, 'Черновик').click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Удалить навсегда' }).click();
  const dialog = page.getByRole('dialog', { name: 'Удалить файл навсегда?' });
  await expect(dialog).toContainText('Файл «Черновик» будет удалён без возможности восстановления');
  await expect(dialog.getByRole('button', { name: 'Отмена' })).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(card(page, 'Черновик')).toBeVisible();
  await card(page, 'Черновик').click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Удалить навсегда' }).click();
  await dialog.getByRole('button', { name: 'Удалить' }).click();
  await expect(card(page, 'Черновик')).toHaveCount(0);
  expect(await page.evaluate((i) => window.__ferrumFiles!.store.get(i), id)).toBeNull();
  // Очистить корзину.
  await page.goto('/#/');
  await card(page, 'Второй').click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Удалить' }).click();
  await page.goto('/#/trash');
  await expect(card(page, 'Второй')).toBeVisible();
  await page.getByRole('button', { name: 'Очистить корзину' }).click();
  await page.getByRole('dialog', { name: 'Очистить корзину?' }).getByRole('button', { name: 'Очистить' }).click();
  await expect(page.getByText('Корзина пуста')).toBeVisible();
  await expect(page.getByRole('button', { name: 'Очистить корзину' })).toBeDisabled();
});

test('поиск фильтрует по имени без учёта регистра', async ({ page }) => {
  await openFiles(page);
  for (const n of ['Альфа', 'Бета', 'альфа-2']) await create(page, n);
  await expect(files(page).getByRole('option')).toHaveCount(3);
  const search = page.getByRole('searchbox', { name: 'Поиск' });
  await search.fill('АЛЬ');
  await expect(files(page).getByRole('option')).toHaveCount(2);
  await expect(card(page, 'Бета')).toHaveCount(0);
  await search.fill('нет такого');
  await expect(page.getByText('Ничего не найдено')).toBeVisible();
  await search.press('Escape');
  await expect(search).toHaveValue('');
  await expect(files(page).getByRole('option')).toHaveCount(3);
});

test('сетка и список переключаются и запоминаются; старые настройки читаются', async ({ page }) => {
  await page.addInitScript(() => {
    if (!localStorage.getItem('ferrum.settings.v1')) localStorage.setItem('ferrum.settings.v1', JSON.stringify({ theme: 'light', leftWidth: 300 }));
  });
  await openFiles(page);
  await create(page, 'Макет');
  await expect(page.locator('.fe-card')).toHaveCount(1);
  const views = page.getByRole('radiogroup', { name: 'Вид' });
  await expect(views.getByRole('radio', { name: 'Сетка' })).toHaveAttribute('aria-checked', 'true');
  await views.getByRole('radio', { name: 'Список' }).click();
  await expect(page.locator('.fe-file-row')).toHaveCount(1);
  await expect(page.locator('.fe-file-row')).toContainText('Изменён только что');
  await expect(page.locator('.fe-file-row .fe-file-row__size')).toHaveText('—');
  await page.reload();
  await expect(page.locator('.fe-file-row')).toHaveCount(1);
  const stored = await page.evaluate(() => JSON.parse(localStorage.getItem('ferrum.settings.v1')!));
  expect(stored).toMatchObject({ theme: 'light', leftWidth: 300, filesView: 'list' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  // Стрелки в группе вида.
  await views.getByRole('radio', { name: 'Список' }).focus();
  await page.keyboard.press('ArrowLeft');
  await expect(views.getByRole('radio', { name: 'Сетка' })).toBeFocused();
  await expect(page.locator('.fe-card')).toHaveCount(1);
});

test('назад браузера из редактора возвращает к файлам и останавливает ядро', async ({ page }) => {
  await openFiles(page);
  await create(page, 'Макет');
  await card(page, 'Макет').click();
  await editorReady(page);
  await page.goBack();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  await expect.poll(async () => (await workers(page)).filter((w) => w.url.includes('core.worker')).map((w) => w.terminated)).toEqual([true]);
  // Вперёд — снова редактор с тем же файлом.
  await page.goForward();
  await editorReady(page);
  await expect(page.locator('.fe-panel-head__name')).toHaveText('Макет');
  // Кнопка «Назад к файлам» в шапке.
  await page.getByRole('button', { name: 'Назад к файлам' }).click();
  await expect(card(page, 'Макет')).toBeVisible();
});

test('«Новый файл» из редактора сохраняет текущий и открывает другой — без вопроса', async ({ page }) => {
  await setup(page);
  await page.goto('/#/new');
  await editorReady(page);
  const first = fileId(page);
  await drawRect(page);
  await fileMenu(page, 'Новый файл');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await expect.poll(() => fileId(page)).not.toBe(first);
  await editorReady(page);
  await liveWhen(page, 'l.nodes === 0');
  // Первый сохранён при уходе: назад — прямоугольник на месте.
  await page.goBack();
  await editorReady(page);
  expect(fileId(page)).toBe(first);
  await liveWhen(page, 'l.nodes === 1');
});

test('вторая вкладка видит новый файл без перезагрузки', async ({ page, context }) => {
  await openFiles(page);
  const other = await context.newPage();
  await setup(other);
  await other.goto('/');
  await expect(other.getByText('Здесь пока нет файлов')).toBeVisible();
  await create(page, 'Из первой вкладки');
  await expect(card(other, 'Из первой вкладки')).toBeVisible();
  // Переименование и удаление — тоже.
  await page.evaluate(async () => {
    const s = window.__ferrumFiles!.store;
    const { rows } = await s.list('recent', '', 0, 10);
    await s.rename(rows[0].id, 'Переименован');
  });
  await expect(card(other, 'Переименован')).toBeVisible();
});

test('неизвестный файл — экран файлов с сообщением «Файл не найден»', async ({ page }) => {
  await openFiles(page, '/#/file/no-such-file');
  await expect(page.locator('.fe-toast', { hasText: 'Файл не найден' })).toHaveCount(1);
  await expect(page).toHaveURL(/#\/$/);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Недавние');
  expect((await workers(page)).filter((w) => w.url.includes('core.worker'))).toEqual([]);
});

test('клавиатура: Tab до карточек, стрелки по сетке, Enter открывает', async ({ page }) => {
  await openFiles(page);
  for (const n of ['Третий', 'Второй', 'Первый']) await create(page, n);
  await expect(files(page).getByRole('option')).toHaveCount(3);
  await page.getByRole('searchbox', { name: 'Поиск' }).focus();
  for (let i = 0; i < 10; i++) {
    await page.keyboard.press('Tab');
    const role = await page.evaluate(() => [document.activeElement?.getAttribute('role'), document.activeElement?.closest('main') !== null]);
    if (role[0] === 'option' && role[1]) break;
  }
  await expect(card(page, 'Первый')).toBeFocused();
  await page.keyboard.press('ArrowRight');
  await expect(card(page, 'Второй')).toBeFocused();
  await expect(card(page, 'Второй')).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('End');
  await expect(card(page, 'Третий')).toBeFocused();
  await page.keyboard.press('Home');
  await page.keyboard.press('ArrowRight');
  // Разделы — список с выбором: одна точка Tab, стрелки меняют раздел.
  const id = await page.evaluate(async () => (await window.__ferrumFiles!.store.list('recent', '', 1, 1)).rows[0].id);
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(new RegExp(`#/file/${id}$`));
  await editorReady(page);
  await expect(page.locator('.fe-panel-head__name')).toHaveText('Второй');
});

test('разделы: стрелки переключают, фокус остаётся в списке разделов', async ({ page }) => {
  await openFiles(page);
  const nav = page.getByRole('listbox', { name: 'Разделы' });
  await nav.getByRole('option', { name: 'Недавние' }).focus();
  await page.keyboard.press('ArrowDown');
  await expect(page).toHaveURL(/#\/drafts$/);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Черновики');
  await expect(nav.getByRole('option', { name: 'Черновики' })).toBeFocused();
  await expect(nav.getByRole('option', { name: 'Черновики' })).toHaveAttribute('aria-selected', 'true');
});

test('ошибка записи — одно уведомление, документ открыт и изменён; уход не теряет правки', async ({ page }) => {
  await setup(page);
  await page.goto('/#/new');
  await editorReady(page);
  const id = fileId(page);
  await page.evaluate(() => window.__ferrumFiles!.store.failWrites(true));
  await drawRect(page);
  await page.keyboard.press('ControlOrMeta+KeyS');
  const toast = page.locator('.fe-toast', { hasText: 'Не удалось сохранить файл: недостаточно места в хранилище браузера' });
  await expect(toast).toHaveCount(1);
  // Повтор (Ctrl+S, уход к файлам) — без второго уведомления; уход — с вопросом,
  // «Остаться» оставляет редактор.
  await page.keyboard.press('ControlOrMeta+KeyS');
  await fileMenu(page, 'Все файлы');
  const ask = page.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' });
  await expect(ask).toContainText('недостаточно места в хранилище браузера');
  await expect(ask.getByRole('button', { name: 'Остаться' })).toBeFocused();
  await ask.getByRole('button', { name: 'Остаться' }).click();
  await expect(ask).toHaveCount(0);
  await expect(toast).toHaveCount(1);
  expect(fileId(page)).toBe(id);
  expect(await page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  expect((await live(page)).nodes).toBe(1);
  // Место появилось — запись проходит, «изменён» снят.
  await page.evaluate(() => window.__ferrumFiles!.store.failWrites(false));
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
});

test('1000 записей: виртуальный список, без длинных задач основного потока', async ({ page }) => {
  await openFiles(page);
  await page.evaluate(() => {
    const w = window as unknown as { __long: number[] };
    w.__long = [];
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) w.__long.push(e.duration);
    }).observe({ type: 'longtask', buffered: false });
  });
  await page.evaluate(() => window.__ferrumFiles!.store.seed(1000));
  await expect(card(page, 'Без названия 1')).toBeVisible();
  const scroller = page.locator('.fe-files__scroll');
  for (const y of [5000, 20000, 1e9, 0]) {
    await scroller.evaluate((el, y) => (el.scrollTop = y), y);
    await page.waitForTimeout(150);
  }
  await scroller.evaluate((el) => (el.scrollTop = el.scrollHeight));
  await expect(card(page, 'Без названия 1000')).toBeVisible();
  // В DOM — только видимые строки с запасом, а не тысяча карточек (окно 1280×720:
  // 4 столбца, 3–4 строки видно).
  const inDom = await files(page).getByRole('option').count();
  expect(inDom).toBeGreaterThan(0);
  expect(inDom).toBeLessThanOrEqual(6 * 4);
  await page.getByRole('radiogroup', { name: 'Вид' }).getByRole('radio', { name: 'Список' }).click();
  await page.getByRole('searchbox', { name: 'Поиск' }).fill('99');
  await expect(card(page, 'Без названия 99')).toBeVisible();
  const long = await page.evaluate(() => (window as unknown as { __long: number[] }).__long);
  expect(long).toEqual([]);
});

test('превью 100 000 узлов: время в worker\'е ядра и запись без длинных задач @perf', async ({ page }, info) => {
  test.skip(info.project.name !== 'chromium', 'замер — в проекте chromium');
  await setup(page);
  await page.goto('/?nodes=100000#/new');
  await editorReady(page);
  // Снимок с превью и без: разница — время превью (лучшее из трёх, машина бывает занята).
  const timing = await page.evaluate(async () => {
    const core = window.__ferrum!.core!;
    const once = (thumbnail: boolean) =>
      new Promise<number>((resolve) => {
        const t = performance.now();
        const off = core.on((e) => {
          if (e.type === 'snapshot') {
            off();
            resolve(performance.now() - t);
          }
        });
        core.send({ type: 'snapshot', thumbnail });
      });
    const best = async (thumbnail: boolean) => {
      let min = Infinity;
      for (let i = 0; i < 3; i++) min = Math.min(min, await once(thumbnail));
      return min;
    };
    const plain = await best(false);
    const withThumb = await best(true);
    return { plain, withThumb };
  });
  const thumb = timing.withThumb - timing.plain;
  console.log(
    `снимок 100 000 узлов: ${timing.plain.toFixed(0)} мс, с превью: ${timing.withThumb.toFixed(0)} мс, превью ≈ ${thumb.toFixed(0)} мс`,
  );
  expect(thumb).toBeLessThan(200);
  // Запись в хранилище (байты и превью — передачей через основной поток): без длинных задач.
  await page.evaluate(() => {
    const w = window as unknown as { __long: number[] };
    w.__long = [];
    new PerformanceObserver((list) => {
      for (const e of list.getEntries()) w.__long.push(e.duration);
    }).observe({ type: 'longtask', buffered: false });
  });
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setDocumentName', name: 'Большой' }));
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+KeyS');
  const id = fileId(page);
  await expect
    .poll(() => page.evaluate((i) => window.__ferrumFiles!.store.get(i).then((m) => m?.size ?? 0), id), { timeout: 30_000 })
    .toBeGreaterThan(1024 * 1024);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  const meta = await page.evaluate((i) => window.__ferrumFiles!.store.get(i), id);
  expect(meta?.thumbVersion).toBeGreaterThan(0);
  expect(await page.evaluate(() => (window as unknown as { __long: number[] }).__long)).toEqual([]);
});

test('вторая вкладка с тем же файлом: «Файл открыт в другой вкладке»; чужая запись — отказ, а не перезапись', async ({ page, context }) => {
  await setup(page);
  await page.goto('/#/new');
  await editorReady(page);
  const id = fileId(page);
  const other = await context.newPage();
  await setup(other);
  await other.goto(`/#/file/${id}`);
  await expect(other.locator('.fe-toast', { hasText: 'Файл открыт в другой вкладке' })).toHaveCount(1);
  await expect(other).toHaveURL(/#\/$/);
  expect((await workers(other)).filter((w) => w.url.includes('core.worker'))).toEqual([]);
  // Запись в обход блокировки (как из вкладки без Web Locks): поколение файла выросло.
  await other.evaluate(async (i) => {
    const s = window.__ferrumFiles!.store;
    const m = (await s.get(i))!;
    await s.write(i, new ArrayBuffer(8), null, m.version, null);
    await s.rename(i, 'Чужая');
  }, id);
  await drawRect(page);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect(
    page.locator('.fe-toast', { hasText: 'Файл изменён в другой вкладке — сохраните копию на диск или откройте файл заново' }),
  ).toHaveCount(1);
  expect(await page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  const stored = await page.evaluate((i) => window.__ferrumFiles!.store.get(i), id);
  expect([stored?.name, stored?.size]).toEqual(['Чужая', 8]);
  // Первая вкладка ушла к файлам (с вопросом — запись не прошла): блокировка снята.
  await page.getByRole('button', { name: 'Назад к файлам' }).click();
  await page.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' }).getByRole('button', { name: 'Уйти' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
  await other.goto(`/#/file/${id}`);
  await expect(other).toHaveURL(new RegExp(`#/file/${id}$`));
});

test('испорченная сохранённая раскладка не мешает открыть файл из хранилища', async ({ page }) => {
  await setup(page);
  await page.goto('/#/new');
  await editorReady(page);
  const id = fileId(page);
  await drawRect(page);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => page.evaluate((i) => window.__ferrumFiles!.store.get(i).then((m) => m?.size ?? 0), id)).toBeGreaterThan(0);
  // Раскладка в настройках, которую ядро отвергнет до открытия файла.
  await page.evaluate(() => localStorage.setItem('ferrum.settings.v1', JSON.stringify({ keymap: 'не раскладка' })));
  await page.reload();
  await editorReady(page);
  expect(fileId(page)).toBe(id);
  await liveWhen(page, 'l.nodes === 1');
  await expect(page.locator('.fe-toast', { hasText: 'Не удалось импортировать раскладку' })).toHaveCount(1);
});

test('ядро остановилось с несохранёнными правками: уйти можно, но с вопросом; история без лишних записей', async ({ page }) => {
  await setup(page);
  await page.goto('/#/');
  // История: файлы (0) → черновики (1) → файл (2).
  await page.getByRole('listbox', { name: 'Разделы' }).getByRole('option', { name: 'Черновики' }).click();
  await expect(page).toHaveURL(/#\/drafts$/);
  await page.getByRole('main').getByRole('button', { name: 'Новый файл' }).first().click();
  await editorReady(page);
  const id = fileId(page);
  await drawRect(page);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  await page.evaluate(() => (window.__ferrum!.core! as unknown as { fail(m: string): void }).fail('проверка'));
  await expect(page.getByRole('alert')).toContainText('Ядро остановлено: проверка');
  const ask = page.getByRole('dialog', { name: 'Изменения не сохранены — уйти?' });
  // «Назад» дважды, пока открыт вопрос: вопрос один, «Остаться» — снова на файле.
  await page.goBack();
  await expect(ask).toContainText('Ядро остановлено — изменения не сохранены');
  await page.goBack();
  await expect(ask).toHaveCount(1);
  await ask.getByRole('button', { name: 'Остаться' }).click();
  await expect(ask).toHaveCount(0);
  await expect(page).toHaveURL(new RegExp(`#/file/${id}$`));
  await page.waitForTimeout(300);
  await expect(page).toHaveURL(new RegExp(`#/file/${id}$`));
  // Записей-двойников нет: «назад» ведёт на предыдущую запись — черновики.
  await page.goBack();
  await expect(page).toHaveURL(/#\/drafts$/);
  await ask.getByRole('button', { name: 'Уйти' }).click();
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Черновики');
  await expect(card(page, 'Без названия')).toBeVisible();
});

test('своя вкладка: файлы → тот же файл и перезагрузка не упираются в блокировку; отказ Web Locks — сообщение', async ({ page }) => {
  await openFiles(page);
  await create(page, 'Макет');
  await card(page, 'Макет').click();
  await editorReady(page);
  await page.getByRole('button', { name: 'Назад к файлам' }).click();
  await card(page, 'Макет').click();
  await editorReady(page);
  await expect(page.locator('.fe-toast', { hasText: 'Файл открыт в другой вкладке' })).toHaveCount(0);
  await page.reload();
  await editorReady(page);
  expect(page.url()).toMatch(/#\/file\//);
  // API блокировок отказывает: не вечное ожидание, а экран файлов с сообщением.
  const other = await page.context().newPage();
  await setup(other);
  await other.addInitScript(() => {
    navigator.locks.request = (() => Promise.reject(new DOMException('запрещено', 'SecurityError'))) as LockManager['request'];
  });
  const id = fileId(page);
  await other.goto(`/#/file/${id}`);
  await expect(other.locator('.fe-toast', { hasText: 'Не удалось открыть файл: блокировка недоступна' })).toHaveCount(1);
  await expect(other).toHaveURL(/#\/$/);
});

test('переименование и удаление в другой вкладке: имя не затирается, удалённый — одно сообщение', async ({ page, context }) => {
  await setup(page);
  await page.goto('/#/new');
  await editorReady(page);
  const id = fileId(page);
  const other = await context.newPage();
  await setup(other);
  await other.goto('/');
  await other.evaluate((i) => window.__ferrumFiles!.store.rename(i, 'Новое имя'), id);
  await drawRect(page);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(false);
  expect(await page.evaluate((i) => window.__ferrumFiles!.store.get(i).then((m) => m?.name), id)).toBe('Новое имя');
  // Удалили навсегда: запись не воскресает, одно сообщение, документ изменён.
  await other.evaluate((i) => window.__ferrumFiles!.store.remove(i), id);
  await page.locator('canvas').focus();
  await page.keyboard.press('Delete');
  await page.keyboard.press('ControlOrMeta+KeyA');
  await page.keyboard.press('Delete');
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  await page.keyboard.press('ControlOrMeta+KeyS');
  const gone = page.locator('.fe-toast', { hasText: 'Файл удалён в другой вкладке — сохраните копию на диск' });
  await expect(gone).toHaveCount(1);
  await page.keyboard.press('ControlOrMeta+KeyS');
  await page.waitForTimeout(300);
  await expect(gone).toHaveCount(1);
  expect(await page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  expect(await page.evaluate((i) => window.__ferrumFiles!.store.get(i), id)).toBeNull();
});

test('отложенное открытие по имени отменяется действием с другой карточкой', async ({ page }) => {
  await openFiles(page);
  await create(page, 'Первый');
  await create(page, 'Второй');
  await expect(files(page).getByRole('option')).toHaveCount(2);
  await card(page, 'Первый').locator('.fe-card__name').click();
  await card(page, 'Второй').click({ button: 'right' });
  await expect(page.getByRole('menu', { name: 'Действия с файлом' })).toBeVisible();
  await page.waitForTimeout(400);
  await expect(page).not.toHaveURL(/#\/file\//);
});

test('заставка: уход к экрану файлов убирает её сразу, не дожидаясь 2 с', async ({ page }) => {
  // Обычный режим: у редактора заставка держится 2 с с открытия страницы.
  await page.goto('/#/new');
  await page.waitForFunction(() => window.__ferrum?.ui?.ready === true, null, { timeout: 90_000 });
  const t = await page.evaluate(() => performance.now());
  test.skip(t > 1500, `ядро готово поздно (${Math.round(t)} мс): проверять нечего`);
  await expect(page.locator('#fe-splash')).toHaveCount(1);
  await page.evaluate(() => (location.hash = '#/'));
  await expect(page.locator('#fe-splash')).toHaveCount(0, { timeout: 1000 });
  expect(await page.evaluate(() => performance.now())).toBeLessThan(2000);
});

test('файл из корзины по адресу не открывается для правки', async ({ page }) => {
  await openFiles(page);
  const id = await create(page, 'Удалённый');
  await page.evaluate((i) => window.__ferrumFiles!.store.trash(i), id);
  await page.goto(`/#/file/${id}`);
  await expect(page.locator('.fe-toast', { hasText: 'Файл в корзине — восстановите его, чтобы открыть' })).toHaveCount(1);
  await expect(page).toHaveURL(/#\/$/);
  expect((await workers(page)).filter((w) => w.url.includes('core.worker'))).toEqual([]);
});

test('щелчок по имени карточки открывает файл; двойной — переименовывает', async ({ page }) => {
  await openFiles(page);
  const id = await create(page, 'Макет');
  await card(page, 'Макет').locator('.fe-card__name').dblclick();
  await expect(page.getByRole('textbox', { name: 'Имя файла' })).toBeFocused();
  await page.keyboard.press('Escape');
  await page.waitForTimeout(400);
  await expect(page).not.toHaveURL(/#\/file\//);
  await card(page, 'Макет').locator('.fe-card__name').click();
  await expect(page).toHaveURL(new RegExp(`#/file/${id}$`));
});

test('«Скачать копию (.ferrum)» и «Открыть в новой вкладке»', async ({ page, context }) => {
  await context.addInitScript(() => ((window as { __ferrumTest?: boolean }).__ferrumTest = true));
  await page.goto('/?nodes=50#/new');
  await editorReady(page);
  const id = fileId(page);
  await page.evaluate(() => window.__ferrum!.core!.send({ type: 'setDocumentName', name: 'Отчёт' }));
  await page.locator('canvas').focus();
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(() => page.evaluate((i) => window.__ferrumFiles!.store.get(i).then((m) => m?.size ?? 0), id)).toBeGreaterThan(0);
  await page.getByRole('button', { name: 'Назад к файлам' }).click();
  await card(page, 'Отчёт').click({ button: 'right' });
  const download = page.waitForEvent('download');
  await page.getByRole('menuitem', { name: 'Скачать копию (.ferrum)' }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe('Отчёт.ferrum');
  const size = (await (await file.createReadStream()).toArray()).reduce((n, b: Buffer) => n + b.length, 0);
  const meta = await page.evaluate((i) => window.__ferrumFiles!.store.get(i), id);
  expect(size).toBe(meta?.size);
  await card(page, 'Отчёт').click({ button: 'right' });
  const opened = context.waitForEvent('page');
  await page.getByRole('menuitem', { name: 'Открыть в новой вкладке' }).click();
  const tab = await opened;
  await tab.waitForFunction(() => window.__ferrum?.ui?.ready === true, null, { timeout: 90_000 });
  expect(tab.url()).toMatch(new RegExp(`#/file/${id}$`));
  await tab.waitForFunction(() => (window.__ferrum!.core!.live?.nodes ?? 0) === 50);
});

test('Ctrl/Cmd+S не открывает диалог сохранения страницы: на экране файлов и в поле панели', async ({ page }) => {
  await openFiles(page);
  // Слушатель ставится до нажатия (и ждётся), результат читается после.
  const listen = () =>
    page.evaluate(() => {
      const w = window as unknown as { __saveKey?: boolean };
      delete w.__saveKey;
      const on = (e: KeyboardEvent) => {
        if (e.code !== 'KeyS') return;
        window.removeEventListener('keydown', on);
        setTimeout(() => (w.__saveKey = e.defaultPrevented));
      };
      window.addEventListener('keydown', on);
    });
  const prevented = () => page.evaluate(() => (window as unknown as { __saveKey?: boolean }).__saveKey);
  await listen();
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(prevented).toBe(true);
  await page.getByRole('main').getByRole('button', { name: 'Новый файл' }).first().click();
  await editorReady(page);
  await drawRect(page);
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified)).toBe(true);
  await page.getByRole('searchbox', { name: 'Поиск слоёв' }).focus();
  await listen();
  await page.keyboard.press('ControlOrMeta+KeyS');
  await expect.poll(prevented).toBe(true);
  // И сохраняет сразу (не через 2 с): фокус в поле, клавиша до ядра не дошла.
  await expect.poll(() => page.evaluate(() => window.__ferrum!.ui!.modified), { timeout: 1500 }).toBe(false);
});
