// Корень приложения: маршрут из адреса (`router.ts`) → экран файлов или редактор с
// файлом из хранилища. Открытый файл заблокирован для других вкладок (Web Locks). Хранилище (worker ввода-вывода) одно на страницу; worker
// ядра запускает только редактор и останавливает при уходе с него. Уход с
// редактора сначала сохраняет файл; не удалось — редактор остаётся, адрес
// возвращается к файлу.

import { createSignal, Match, onCleanup, Switch } from 'solid-js';
import { UNTITLED } from '../core/file-list';
import { App } from './app';
import { dismissSplash, isTestMode } from './boot';
import { FilesScreen } from './files-screen';
import { ConfirmDialog } from './dialog';
import { IdbFileStore, type FileMeta, type Section } from './file-store';
import { storage } from './settings';
import { openSettings, type SettingsStore } from './settings-store';
import type { Backend } from '../wasm/ferrum_wasm';
import { allowRestart, type Restart } from './restart';
import { historyIndex, parseRoute, routeHash, routeUrl, type Route } from './router';

type View =
  | { kind: 'wait' }
  | { kind: 'files'; section: Section; notice: { text: string; seq: number } | null }
  /** `restart` — редактор запущен заново после сбоя; `backend` — бэкенд рендера
   * (после потери GPU — запасной, и для следующих перезапусков тоже). */
  | { kind: 'editor'; file: FileMeta; restart?: Restart; backend?: Backend }
  /** Файл на диске без доступа (браузер перезапущен): вопрос, ответ — по щелчку. */
  | { kind: 'access'; file: FileMeta; answer: (ok: boolean) => void };

/** Сколько ждать блокировку файла, прежде чем сказать «открыт в другой вкладке»:
 * своя прежняя блокировка (F5, файлы → тот же файл) снимается не мгновенно. */
export const LOCK_WAIT_MS = 1000;
const LOCK_RETRY_MS = 100;

type Lock = { release: () => void } | { busy: true } | { error: string };

/** Блокировка файла на время редактирования (Web Locks): вторая вкладка с тем же
 * файлом получает отказ. Без Web Locks — без блокировки (запись всё равно
 * проверяет поколение). Занят — несколько попыток в пределах `LOCK_WAIT_MS`;
 * отказ самого API — ошибка, а не вечное ожидание. */
async function lockFile(id: string): Promise<Lock> {
  const locks = (navigator as Navigator & { locks?: LockManager }).locks;
  if (!locks) return { release: () => {} };
  const once = () =>
    new Promise<Lock>((resolve) => {
      locks
        .request(`ferrum-file:${id}`, { ifAvailable: true }, (lock) => {
          if (!lock) {
            resolve({ busy: true });
            return;
          }
          // Блокировка держится, пока не вызвано снятие.
          return new Promise<void>((release) => resolve({ release: () => release() }));
        })
        .catch((e: unknown) => resolve({ error: e instanceof Error ? e.message : String(e) }));
    });
  const until = performance.now() + LOCK_WAIT_MS;
  for (;;) {
    const r = await once();
    if (!('busy' in r) || performance.now() >= until) return r;
    await new Promise((t) => setTimeout(t, LOCK_RETRY_MS));
  }
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
  /** Куда пришли «назад»/«вперёд» (`null` — запись не наша); `undefined` — не ходили. */
  let popped: number | null | undefined;
  let leaving = false;
  let wanted: { route: Route; push: boolean; pop: boolean } | null = null;
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

  /** Моменты перезапусков редактора — для предела `allowRestart`. */
  const restarts: number[] = [];
  /** Тот же файл — в новом редакторе (сбой ядра, потеря GPU). Блокировка файла
   * остаётся за вкладкой; метаданные — заново (имя могли сменить, файл — убрать в
   * корзину). `false` — перезапусков слишком много или редактор уже уходит. */
  const restart = (file: FileMeta, r: Restart, backend: Backend | undefined): boolean => {
    // Уход уже удался (блокировка снята, корень открывает другое): перезапуск открыл
    // бы файл без блокировки — остаётся плашка, пока не сменится экран.
    if (leave === null) return false;
    if (!allowRestart(restarts, performance.now())) return false;
    // Ядро остановлено, сохранять нечего: переход до нового редактора уходит сразу —
    // через `release()`, и `seq` отменяет перезапуск.
    leave = async () => true;
    const my = ++seq;
    const show = (m: FileMeta) => setView({ kind: 'editor', file: m, restart: r, backend: r.backend ?? backend });
    const away = (notice: string) => {
      leave = null;
      release();
      toFiles(notice);
    };
    files.get(file.id).then(
      (m) => {
        if (my !== seq) return;
        if (!m) away('Файл не найден');
        // Как при открытии: файл из корзины — сначала восстановить (правки в журнале).
        else if (m.trashedAt !== undefined) away('Файл в корзине — восстановите его, чтобы открыть');
        else show(m);
      },
      () => my === seq && show(file),
    );
    return true;
  };

  /** Открыть файл в редакторе: занят другой вкладкой — к экрану файлов. */
  async function edit(file: FileMeta, my: number): Promise<void> {
    // Файл на диске: доступ к нему после перезапуска браузера снова разрешает
    // пользователь — щелчком в вопросе (запросу нужно действие пользователя).
    if (file.disk && files.disk && (await files.disk.access(file.id).catch(() => 'denied' as const)) !== 'granted') {
      const disk = files.disk;
      // Вопрос — поверх заставки нельзя: она перехватывала бы щелчки.
      dismissSplash(true);
      const ok = await new Promise<boolean>((answer) => setView({ kind: 'access', file, answer }));
      const granted = ok && (await disk.requestAccess(file.id).catch(() => false));
      if (my !== seq) return;
      if (!granted) {
        toFiles('Нет доступа к файлу на диске — разрешите его или откройте файл заново');
        return;
      }
    }
    const lock = await lockFile(file.id);
    if (my !== seq) {
      if ('release' in lock) lock.release();
      return;
    }
    if ('busy' in lock) {
      toFiles('Файл открыт в другой вкладке');
      return;
    }
    if ('error' in lock) {
      toFiles(`Не удалось открыть файл: блокировка недоступна (${lock.error})`);
      return;
    }
    unlock = lock.release;
    current = { kind: 'file', id: file.id };
    replace(current);
    setView({ kind: 'editor', file });
  }

  /** Переход к маршруту. `push` — из интерфейса: новая запись истории, когда уход
   * удался; `pop` — пришли «назад» или «вперёд» (номер записи, куда пришли). */
  async function go(route: Route, push = false, pop?: { to: number | null }): Promise<void> {
    if (pop) popped = pop.to;
    if (leave) {
      // Пока открыт вопрос об уходе, новые переходы («назад» ещё раз) только
      // запоминаются: решает ответ, и переход — последний запрошенный.
      wanted = { route, push, pop: pop !== undefined };
      if (leaving) return;
      leaving = true;
      const ok = await leave();
      leaving = false;
      const w = wanted;
      wanted = null;
      if (!ok) {
        // Остались в редакторе: назад к своей записи истории одним ходом (сколько
        // бы раз ни нажали «назад»), без новых записей.
        if (popped != null && popped !== index) {
          restoring = true;
          history.go(index - popped);
        } else if (current) replace(current);
        popped = undefined;
        return;
      }
      leave = null;
      release();
      route = w!.route;
      push = w!.push;
      if (!w!.pop) popped = undefined;
    }
    const my = ++seq;
    if (push && location.hash !== routeHash(route)) {
      index += 1;
      history.pushState({ ferrumIndex: index }, '', routeUrl(route));
    } else if (popped !== undefined) {
      // Запись истории без нашего номера (адрес набрали вручную) — новая.
      index = popped ?? index + 1;
      if (popped == null) replace(route);
    }
    popped = undefined;
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
    // Настройки ещё читаются (экрана нет): первый переход возьмёт адрес, куда пришли.
    if (!settings) {
      early = { to: historyIndex(e.state) };
      return;
    }
    void go(parseRoute(location.hash), false, { to: historyIndex(e.state) });
  };
  window.addEventListener('popstate', onPop);
  // Настройки — до первого экрана: тема, вид списка и раскладка нужны сразу (экраны
  // получают их готовыми). Хранилище настроек живёт у корня: перезапуск редактора
  // после сбоя ядра или потери GPU его не теряет.
  let settings: SettingsStore | null = null;
  let early: { to: number | null } | undefined;
  let disposed = false;
  void openSettings(files, storage()).then((s) => {
    if (disposed) {
      s.dispose();
      return;
    }
    settings = s;
    void go(parseRoute(location.hash), false, early);
  });
  onCleanup(() => {
    disposed = true;
    window.removeEventListener('popstate', onPop);
    release();
    settings?.dispose();
    files.dispose();
  });

  return (
    <Switch>
      <Match when={(() => { const v = view(); return v.kind === 'access' ? v : null; })()} keyed>
        {(v) => (
          <ConfirmDialog
            title="Разрешить доступ к файлу на диске?"
            text={`Ferrum откроет «${v.file.disk}» и будет сохранять изменения в него`}
            confirm="Разрешить"
            cancel="Отмена"
            onConfirm={() => v.answer(true)}
            onCancel={() => v.answer(false)}
          />
        )}
      </Match>
      <Match when={view().kind === 'files'}>
        <FilesScreen
          files={files}
          settings={settings!}
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
            settings={settings!}
            restart={v.restart}
            backend={v.backend}
            onRestart={(r) => restart(v.file, r, v.backend)}
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
