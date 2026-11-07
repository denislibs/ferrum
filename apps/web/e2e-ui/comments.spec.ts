// Окно, поле и панель комментариев (спека подсистемы 4, раздел 10.3) на стенде интерфейса (`harness.tsx`): ядро и
// `sync.worker` подменены, события и ответы — из теста.

import { expect, test, type Page } from '@playwright/test';
import type { CommentDto, ThreadView } from '@ferrum/protocol';
import type { CommentRequest, CommentResult, CommentsToMain } from '../src/core/comments-store';
import type { CoreEvent } from '../src/wasm/ferrum_wasm';

const ME = 'u1';
const author = (id: string, name: string) => ({ id, name });

const comment = (thread: string, id: string, by = author(ME, 'Я'), body = 'текст', mentions: Record<string, string> = {}): CommentDto => ({
  id,
  thread,
  author: by,
  body,
  createdAt: Date.now(),
  editedAt: null,
  mentions,
});

const thread = (id: string, o: Partial<ThreadView> = {}): ThreadView => {
  const first = o.first ?? comment(id, `${id}c`);
  return {
    id,
    pin: { page: '1@1', node: null, x: 0, y: 0, wx: 0, wy: 0 },
    author: first.author,
    createdAt: first.createdAt,
    resolvedAt: null,
    resolvedBy: null,
    seq: 1,
    first,
    replies: 0,
    lastAt: first.createdAt,
    participants: [first.author.id ?? ''],
    unread: false,
    ...o,
  };
};

async function mount(page: Page, o: { link?: string; mode?: boolean; panelHeight?: number } = {}): Promise<void> {
  await page.goto('/e2e-ui/comments.html');
  await page.waitForFunction(() => !!window.harness);
  await page.evaluate((opts) => window.harness.mountComments(opts), o);
  await emit(page, { t: 'me', user: ME, role: 'editor' });
}
const core = (page: Page, e: CoreEvent) => page.evaluate((ev) => window.harness.core(ev), e);
const emit = (page: Page, m: CommentsToMain) => page.evaluate((msg) => window.harness.comments(msg), m);
const answer = (page: Page, k: CommentRequest['k'], r: CommentResult) =>
  page.evaluate(([kk, rr]) => window.harness.answer(kk, rr), [k, r] as const);
const toasts = (page: Page) => page.evaluate(() => window.harness.toasts);

/** Открыть обсуждение щелчком по метке (событие ядра) и прислать его комментарии. */
async function openThread(page: Page, t: ThreadView, comments: CommentDto[] = [t.first], at = { sx: 200, sy: 200 }): Promise<void> {
  await core(page, { type: 'commentOpened', id: t.id, ...at });
  await emit(page, { t: 'detail', thread: t.id, view: t, comments });
  await expect(page.getByRole('dialog', { name: 'Обсуждение' })).toBeVisible();
}

const place = (sx: number, sy: number): CoreEvent => ({
  type: 'commentPlace',
  page: '1@1',
  node: undefined,
  x: sx,
  y: sy,
  wx: sx,
  wy: sy,
  sx,
  sy,
});

test('Esc в правке своего комментария отменяет правку, окно остаётся', async ({ page }) => {
  await mount(page);
  await openThread(page, thread('t1'));
  const dialog = page.getByRole('dialog', { name: 'Обсуждение' });
  await dialog.getByRole('button', { name: 'Изменить' }).click();
  const edit = dialog.getByRole('textbox', { name: 'Изменить комментарий' });
  await expect(edit).toBeFocused();
  await page.keyboard.press('Escape');
  await expect(edit).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('textbox', { name: 'Ответ' })).toBeVisible();
});

test('Esc при списке упоминаний закрывает список, второй — окно; фокус возвращается холсту', async ({ page }) => {
  await mount(page);
  await answer(page, 'mentions', { ok: true, value: [{ id: 'u2', name: 'Борис', email: 'b@example.com' }] });
  await openThread(page, thread('t1'));
  const dialog = page.getByRole('dialog', { name: 'Обсуждение' });
  const field = dialog.getByRole('textbox', { name: 'Ответ' });
  await field.click();
  await page.keyboard.type('привет @Бо');
  await expect(page.getByRole('listbox', { name: 'Упомянуть' })).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(page.getByRole('listbox', { name: 'Упомянуть' })).toHaveCount(0);
  await expect(dialog).toBeVisible();
  await expect(field).toHaveValue('привет @Бо');
  await page.keyboard.press('Escape');
  await expect(dialog).toHaveCount(0);
  await expect(page.locator('canvas')).toBeFocused();
});

test('щелчок, закрывший окно, не ставит комментарий, но и не съедает следующий', async ({ page }) => {
  await mount(page);
  // Щелчки — в стороне от окна (оно справа снизу от метки в 200,200).
  const canvas = page.locator('canvas');
  // Закрывший щелчок дал место (обычный щелчок в режиме комментариев) — оно не ставится.
  await openThread(page, thread('t1'));
  await canvas.click({ position: { x: 700, y: 520 } });
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await core(page, place(700, 520));
  await expect(page.getByRole('dialog')).toHaveCount(0);
  // Закрывший щелчок события не дал (окно открыто из панели вне режима, протяжка, панорама): следующий щелчок ставит.
  await openThread(page, thread('t2'));
  await canvas.click({ position: { x: 700, y: 520 } });
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await canvas.click({ position: { x: 720, y: 540 } });
  await core(page, place(720, 540));
  await expect(page.getByRole('dialog', { name: 'Новый комментарий' })).toBeVisible();
});

test('окно у метки у правого нижнего края не выходит за холст', async ({ page }) => {
  await mount(page);
  // Холст: 100,40 — 800×600 на стенде.
  await openThread(page, thread('t1', { replies: 3 }), [comment('t1', 'a'), comment('t1', 'b'), comment('t1', 'c'), comment('t1', 'd')], {
    sx: 790,
    sy: 590,
  });
  const box = (await page.getByRole('dialog', { name: 'Обсуждение' }).boundingBox())!;
  const canvas = (await page.locator('canvas').boundingBox())!;
  expect(box.x).toBeGreaterThanOrEqual(canvas.x);
  expect(box.y).toBeGreaterThanOrEqual(canvas.y);
  expect(box.x + box.width).toBeLessThanOrEqual(canvas.x + canvas.width);
  await expect
    .poll(async () => {
      const b = (await page.getByRole('dialog', { name: 'Обсуждение' }).boundingBox())!;
      return b.y + b.height <= canvas.y + canvas.height;
    })
    .toBe(true);
});

test('панель: при росте колонки рисуются и нижние строки', async ({ page }) => {
  await mount(page, { panelHeight: 300 });
  const upserts = Array.from({ length: 60 }, (_, i) => thread(`t${i}`, { lastAt: 1000 - i, seq: i + 1 }));
  await emit(page, { t: 'threads', reset: true, upserts, removed: [] });
  const rows = page.locator('.fe-comments__row');
  await expect(rows.first()).toBeVisible();
  const before = await rows.count();
  await page.evaluate(() => window.harness.setPanelHeight(2000));
  // Колонка в 2000 px показывает строки до ~24-й: прежнее окно (на 300 px) кончалось раньше.
  await expect(page.getByRole('button', { name: /^Я/ }).nth(Math.ceil(1800 / 76))).toBeAttached();
  expect(await rows.count()).toBeGreaterThan(before);
});

test('«не прочитано» в строке панели озвучивается', async ({ page }) => {
  await mount(page);
  await emit(page, { t: 'threads', reset: true, upserts: [thread('t1', { unread: true }), thread('t2')], removed: [] });
  await expect(page.getByRole('button', { name: /не прочитано/ })).toHaveCount(1);
});

test('упоминание без имени — «@Участник» и в тексте, и в правке', async ({ page }) => {
  await mount(page);
  const c = comment('t1', 't1c', author(ME, 'Я'), 'для @[u3] ок', { u3: '' });
  await openThread(page, thread('t1', { first: c }));
  const dialog = page.getByRole('dialog', { name: 'Обсуждение' });
  await expect(dialog.locator('.fe-comment__body')).toHaveText('для @Участник ок');
  await dialog.getByRole('button', { name: 'Изменить' }).click();
  await expect(dialog.getByRole('textbox', { name: 'Изменить комментарий' })).toHaveValue('для @Участник ок');
});

test('автор, удаливший своё обсуждение, не получает тост «Обсуждение удалено»; удалённое другим — получает', async ({ page }) => {
  await mount(page);
  await openThread(page, thread('t1'));
  const dialog = page.getByRole('dialog', { name: 'Обсуждение' });
  await dialog.getByRole('button', { name: 'Удалить' }).click();
  await dialog.getByRole('button', { name: 'Удалить обсуждение?' }).click();
  await expect.poll(() => page.evaluate(() => window.harness.requests.some((r) => r.k === 'delete'))).toBe(true);
  await emit(page, { t: 'detail', thread: 't1', view: null, comments: [] });
  await expect(dialog).toHaveCount(0);
  // Чужое удаление открытого обсуждения.
  await openThread(page, thread('t2'));
  await emit(page, { t: 'detail', thread: 't2', view: null, comments: [] });
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(await toasts(page)).toEqual(['Обсуждение удалено']);
});

test('ссылка на обсуждение включает режим комментариев и открывает его', async ({ page }) => {
  await mount(page, { link: 't1', mode: false });
  await emit(page, { t: 'threads', reset: true, upserts: [thread('t1')], removed: [] });
  const sent = await page.evaluate(() => window.harness.sent);
  expect(sent).toContainEqual({ type: 'setTool', tool: 'comment' });
  expect(sent.findIndex((c) => c.type === 'setTool')).toBeLessThan(sent.findIndex((c) => c.type === 'commentFocus'));
});

test('после ответа фокус остаётся в поле', async ({ page }) => {
  await mount(page);
  await openThread(page, thread('t1'));
  const field = page.getByRole('dialog', { name: 'Обсуждение' }).getByRole('textbox', { name: 'Ответ' });
  await field.click();
  await page.keyboard.type('ещё');
  // Пока сервер не ответил, поле недоступно (и браузер снимает с него фокус).
  await page.evaluate(() => window.harness.hold('reply'));
  await page.keyboard.press('Enter');
  await expect(field).toBeDisabled();
  await expect(field).not.toBeFocused();
  await page.evaluate(() => window.harness.release());
  await expect(field).toHaveValue('');
  await expect(field).toBeFocused();
});

test('«Аккаунт»: настройка писем не загрузилась — сообщение и «Повторить»', async ({ page }) => {
  let fail = true;
  await page.route('**/api/me/prefs', (route) =>
    fail
      ? route.fulfill({ status: 503, body: '' })
      : route.fulfill({ status: 200, contentType: 'application/json', body: '{"commentMail":false}' }),
  );
  await page.goto('/e2e-ui/comments.html');
  await page.waitForFunction(() => !!window.harness);
  await page.evaluate(() => window.harness.mountAccount());
  const toggle = page.getByRole('checkbox', { name: /Письма о комментариях/ });
  await expect(page.getByRole('alert')).toContainText('Настройка писем не загрузилась');
  await expect(toggle).toBeDisabled();
  fail = false;
  await page.getByRole('button', { name: 'Повторить' }).click();
  await expect(page.getByRole('alert')).toHaveCount(0);
  await expect(toggle).toBeEnabled();
  await expect(toggle).not.toBeChecked();
});
