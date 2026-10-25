// Корень приложения: маршрут из адреса (`router.ts`) → экран файлов или редактор с
// файлом из хранилища. Хранилище (worker ввода-вывода) одно на страницу; worker
// ядра запускает только редактор и останавливает при уходе с него. Уход с
// редактора сначала сохраняет файл; не удалось — редактор остаётся, адрес
// возвращается к файлу.

import { createSignal, Match, onCleanup, Switch } from 'solid-js';
import { UNTITLED } from '../core/file-list';
import { App } from './app';
import { isTestMode } from './boot';
import { FilesScreen } from './files-screen';
import { IdbFileStore, type FileMeta, type Section } from './file-store';
import { parseRoute, routeHash, setHash, type Route } from './router';

type View =
  | { kind: 'wait' }
  | { kind: 'files'; section: Section; notice: { text: string; seq: number } | null }
  | { kind: 'editor'; file: FileMeta };

export function Root() {
  const test = isTestMode();
  const files = new IdbFileStore(test);
  if (test) window.__ferrumFiles = { store: files };
  const [view, setView] = createSignal<View>({ kind: 'wait' });
  let leave: (() => Promise<boolean>) | null = null;
  let current: Route | null = null;
  let seq = 0;

  let notices = 0;
  const showFiles = (section: Section, notice: string | null = null) => {
    const v = view();
    // Тот же экран остаётся (фокус и поиск не теряются); сообщение — новое.
    const n = notice ? { text: notice, seq: ++notices } : v.kind === 'files' ? v.notice : null;
    setView({ kind: 'files', section, notice: n });
  };

  async function go(route: Route): Promise<void> {
    const my = ++seq;
    if (leave) {
      const ok = await leave();
      // Запись не удалась (ошибка уже показана): остаёмся в редакторе с тем же адресом.
      if (!ok) {
        if (current) setHash(current, true);
        return;
      }
      leave = null;
    }
    if (my !== seq) return;
    try {
      if (route.kind === 'files') {
        current = route;
        showFiles(route.section);
      } else if (route.kind === 'new') {
        const file = await files.create(UNTITLED);
        if (my !== seq) return;
        current = { kind: 'file', id: file.id };
        setHash(current, true);
        setView({ kind: 'editor', file });
      } else {
        const file = route.id ? await files.get(route.id) : null;
        if (my !== seq) return;
        if (!file) {
          current = { kind: 'files', section: 'recent' };
          setHash(current, true);
          showFiles('recent', 'Файл не найден');
          return;
        }
        current = route;
        setView({ kind: 'editor', file });
      }
    } catch (e) {
      current = { kind: 'files', section: 'recent' };
      setHash(current, true);
      showFiles('recent', `Хранилище файлов недоступно: ${e instanceof Error ? e.message : String(e)}`);
    }
  }

  // Новый адрес — через `hashchange` (запись в истории); тот же адрес его не даёт —
  // переход сразу.
  const navigate = (r: Route) => {
    if (location.hash === routeHash(r)) void go(r);
    else setHash(r);
  };
  const onHash = () => void go(parseRoute(location.hash));
  window.addEventListener('hashchange', onHash);
  onCleanup(() => {
    window.removeEventListener('hashchange', onHash);
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
              current = { kind: 'files', section: 'recent' };
              setHash(current, true);
              showFiles('recent', message);
            }}
          />
        )}
      </Match>
    </Switch>
  );
}
