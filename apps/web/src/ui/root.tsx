// Корень приложения: маршрут из адреса (`router.ts`) → экран файлов или редактор с
// файлом из хранилища. Открытый файл заблокирован для других вкладок (Web Locks). Хранилище (worker ввода-вывода) одно на страницу; worker
// ядра запускает только редактор и останавливает при уходе с него. Уход с
// редактора сначала сохраняет файл; не удалось — редактор остаётся, адрес
// возвращается к файлу.

import { createSignal, Match, onCleanup, Switch } from 'solid-js';
import { UNTITLED } from '../core/file-list';
import { App } from './app';
import { isTestMode } from './boot';
import { FilesScreen } from './files-screen';
import { IdbFileStore, type FileMeta, type Section } from './file-store';
import { historyIndex, parseRoute, routeHash, routeUrl, type Route } from './router';

type View =
  | { kind: 'wait' }
  | { kind: 'files'; section: Section; notice: { text: string; seq: number } | null }
  | { kind: 'editor'; file: FileMeta };

/** Блокировка файла на время редактирования (Web Locks): вторая вкладка с тем же
 * файлом получает отказ. Без Web Locks — без блокировки (запись всё равно
 * проверяет поколение). Возвращает снятие или `null`, если файл занят. */
function lockFile(id: string): Promise<(() => void) | null> {
  const locks = (navigator as Navigator & { locks?: LockManager }).locks;
  if (!locks) return Promise.resolve(() => {});
  return new Promise((resolve) => {
    void locks.request(`ferrum-file:${id}`, { ifAvailable: true }, (lock) => {
      if (!lock) {
        resolve(null);
        return;
      }
      // Блокировка держится, пока не вызвано снятие.
      return new Promise<void>((release) => resolve(() => release()));
    });
  });
}

export function Root() {
  const test = isTestMode();
  const files = new IdbFileStore(test);
  if (test) window.__ferrumFiles = { store: files };
  const [view, setView] = createSignal<View>({ kind: 'wait' });
  let leave: (() => Promise<boolean>) | null = null;
  let unlock: (() => void) | null = null;
  let current: Route | null = null;
  let seq = 0;
  // Номер текущей записи истории: отказ уйти по «назад» или «вперёд» возвращает
  // на свою запись ходом в обратную сторону, а не новой записью.
  let index = historyIndex(history.state) ?? 0;
  history.replaceState({ ferrumIndex: index }, '', location.href);
  let restoring = false;
  const replace = (r: Route) => history.replaceState({ ferrumIndex: index }, '', routeUrl(r));
  const release = () => {
    unlock?.();
    unlock = null;
  };

  let notices = 0;
  const showFiles = (section: Section, notice: string | null = null) => {
    const v = view();
    // Тот же экран остаётся (фокус и поиск не теряются); сообщение — новое.
    const n = notice ? { text: notice, seq: ++notices } : v.kind === 'files' ? v.notice : null;
    setView({ kind: 'files', section, notice: n });
  };
  const toFiles = (notice: string) => {
    current = { kind: 'files', section: 'recent' };
    replace(current);
    showFiles('recent', notice);
  };

  /** Открыть файл в редакторе: занят другой вкладкой — к экрану файлов. */
  async function edit(file: FileMeta, my: number): Promise<void> {
    const un = await lockFile(file.id);
    if (my !== seq) {
      un?.();
      return;
    }
    if (!un) {
      toFiles('Файл открыт в другой вкладке');
      return;
    }
    unlock = un;
    current = { kind: 'file', id: file.id };
    replace(current);
    setView({ kind: 'editor', file });
  }

  /** Переход к маршруту. `push` — из интерфейса: новая запись истории, когда уход
   * удался; `pop` — пришли «назад» или «вперёд» (номер записи, куда пришли). */
  async function go(route: Route, push = false, pop?: { to: number | null }): Promise<void> {
    const my = ++seq;
    const from = index;
    if (leave) {
      const ok = await leave();
      // Остались в редакторе: адрес — снова файла, история — без лишних записей.
      if (!ok) {
        if (pop?.to != null && pop.to !== from) {
          restoring = true;
          if (pop.to < from) history.forward();
          else history.back();
        } else if (current) replace(current);
        return;
      }
      leave = null;
      release();
    }
    if (my !== seq) return;
    if (push && location.hash !== routeHash(route)) {
      index += 1;
      history.pushState({ ferrumIndex: index }, '', routeUrl(route));
    } else if (pop) {
      // Запись истории без нашего номера (адрес набрали вручную) — новая.
      index = pop.to ?? index + 1;
      if (pop.to == null) replace(route);
    }
    try {
      if (route.kind === 'files') {
        current = route;
        showFiles(route.section);
      } else if (route.kind === 'new') {
        const file = await files.create(UNTITLED);
        if (my !== seq) return;
        await edit(file, my);
      } else {
        const file = route.id ? await files.get(route.id) : null;
        if (my !== seq) return;
        if (!file) toFiles('Файл не найден');
        // Файл из корзины для правки не открывается: сначала восстановить.
        else if (file.trashedAt !== undefined) toFiles('Файл в корзине — восстановите его, чтобы открыть');
        else await edit(file, my);
      }
    } catch (e) {
      toFiles(`Хранилище файлов недоступно: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  // Переход из интерфейса — новая запись истории (`pushState` не даёт `popstate`).
  const navigate = (r: Route) => void go(r, true);
  // «Назад», «вперёд» и адрес, набранный вручную.
  const onPop = (e: PopStateEvent) => {
    if (restoring) {
      restoring = false;
      return;
    }
    void go(parseRoute(location.hash), false, { to: historyIndex(e.state) });
  };
  window.addEventListener('popstate', onPop);
  onCleanup(() => {
    window.removeEventListener('popstate', onPop);
    release();
    files.dispose();
  });
  void go(parseRoute(location.hash));

  return (
    <Switch>
      <Match when={view().kind === 'files'}>
        <FilesScreen
          files={files}
          section={(view() as Extract<View, { kind: 'files' }>).section}
          notice={(view() as Extract<View, { kind: 'files' }>).notice}
          navigate={navigate}
        />
      </Match>
      <Match when={(() => { const v = view(); return v.kind === 'editor' ? v : null; })()} keyed>
        {(v) => (
          <App
            file={v.file}
            files={files}
            navigate={navigate}
            onLeave={(fn) => (leave = fn)}
            onOpenFailed={(message) => {
              // Испорченный файл: запись не трогаем, сохранять нечего.
              leave = null;
              release();
              toFiles(message);
            }}
          />
        )}
      </Match>
    </Switch>
  );
}
