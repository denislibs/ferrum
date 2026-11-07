// Стенд интерфейса комментариев без сервера и ядра (`playwright.ui.config.ts`, `vite` в режиме разработки): окно, поле и
// панель (`ui/server/comments.tsx`) и вкладка «Аккаунт» монтируются отдельно; ядро и `sync.worker` заменены записью
// команд и событиями из теста. Поведение DOM (Esc, фокус, размеры, щелчки) проверяется здесь, сценарии со стендом —
// в `e2e-server`.

import '../src/ui/styles/theme.css';
import '../src/ui/styles/base.css';
import '../src/ui/styles/auth.css';
import '../src/ui/styles/server.css';
import { createEffect, createRoot, createSignal } from 'solid-js';
import { render } from 'solid-js/web';
import type { CommentRequest, CommentResult, CommentsToMain } from '../src/core/comments-store';
import { AccountTab } from '../src/ui/auth/account';
import type { Editor } from '../src/ui/context';
import { CommentsUi } from '../src/ui/server/comments';
import type { Command, CoreEvent } from '../src/wasm/ferrum_wasm';

export interface CommentsHarness {
  /** Команды ядру и запросы в `sync.worker` по порядку; тосты. */
  sent: Command[];
  requests: CommentRequest[];
  toasts: string[];
  /** Ответ на запросы вида `k` (по умолчанию — успех без значения). */
  answer(k: CommentRequest['k'], r: CommentResult): void;
  /** Запросы вида `k` ждут `release()`: ответ сервера не мгновенный. */
  hold(k: CommentRequest['k']): void;
  release(): void;
  core(e: CoreEvent): void;
  comments(m: CommentsToMain): void;
  mountComments(o?: { link?: string; mode?: boolean; panelHeight?: number }): void;
  setPanelHeight(px: number): void;
  mountAccount(): void;
}

declare global {
  interface Window {
    harness: CommentsHarness;
  }
}

const stage = document.getElementById('stage')!;
const coreListeners = new Set<(e: CoreEvent) => void>();
const commentListeners = new Set<(m: CommentsToMain) => void>();
const answers = new Map<string, CommentResult>();
const held = new Set<string>();
let waiting: (() => void)[] = [];
const [panelHeight, setPanelHeight] = createSignal(300);

const h: CommentsHarness = {
  sent: [],
  requests: [],
  toasts: [],
  answer: (k, r) => void answers.set(k, r),
  hold: (k) => void held.add(k),
  release() {
    held.clear();
    const now = waiting;
    waiting = [];
    now.forEach((go) => go());
  },
  core: (e) => coreListeners.forEach((l) => l(e)),
  comments: (m) => commentListeners.forEach((l) => l(m)),
  setPanelHeight: (px) => setPanelHeight(px),
  mountComments(o = {}) {
    if (o.panelHeight !== undefined) setPanelHeight(o.panelHeight);
    // Холст — не у края общего предка: окно у метки считается от его положения.
    stage.style.cssText = 'position: relative; width: 1200px; height: 700px;';
    const canvas = document.createElement('canvas');
    canvas.tabIndex = 0;
    canvas.style.cssText = 'position: absolute; left: 100px; top: 40px; width: 800px; height: 600px; background: #eee;';
    stage.append(canvas);
    const aside = document.createElement('aside');
    aside.style.cssText = 'position: absolute; right: 0; top: 0; width: 260px; display: flex; flex-direction: column;';
    const host = document.createElement('div');
    host.className = 'fe-comments-host';
    aside.append(host);
    stage.append(aside);
    const [mode] = createSignal(o.mode ?? true);
    const editor = {
      core: {
        on(l: (e: CoreEvent) => void) {
          coreListeners.add(l);
          return () => coreListeners.delete(l);
        },
      },
      send: (cmd: Command) => void h.sent.push(cmd),
    } as unknown as Editor;
    // Высота колонки панели — из теста (окно браузера выросло).
    createRoot(() => createEffect(() => (aside.style.height = `${panelHeight()}px`)));
    render(
      () => (
        <CommentsUi
          editor={editor}
          canvas={canvas}
          request={(req) => {
            h.requests.push(req);
            const result = answers.get(req.k) ?? { ok: true };
            if (!held.has(req.k)) return Promise.resolve(result);
            return new Promise<CommentResult>((done) => waiting.push(() => done(result)));
          }}
          subscribe={(l) => {
            commentListeners.add(l);
            return () => commentListeners.delete(l);
          }}
          panelHost={() => host}
          mode={mode}
          page={() => '1@1'}
          link={o.link}
          toast={(text) => void h.toasts.push(text)}
        />
      ),
      stage,
    );
  },
  mountAccount() {
    render(() => <AccountTab />, stage);
  },
};
window.harness = h;
