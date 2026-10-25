// Диалог «Настройки»: горячие клавиши (вся таблица команд по группам, запись
// сочетания с клавиатуры по физической клавише — работает на русской раскладке,
// конфликты с предложением заменить, предупреждение о сочетаниях браузера, сброс
// одной команды и всех, импорт и экспорт JSON), тема, привязка к пикселям.
// Диалог модальный: пока он открыт, клавиши холсту не пересылаются (keys.ts), фокус
// не уходит из него, а щелчок по пустому месту оставляет фокус на самом диалоге.

import { createMemo, createSignal, For, Index, onCleanup, onMount, Show, type JSX } from 'solid-js';
import { Portal } from 'solid-js/web';
import type { KeymapEntry } from '../wasm/ferrum_wasm';
import { kb } from '../core/limits';
import { useEditor } from './context';
import { downloadBytes } from './files';
import { Icon, type IconName } from './icons';
import { conflictsOf, filterGroups, groupCommands, MAX_KEYMAP_FILE, recordKey, reservedOf } from './keymap-ui';
import type { ThemeChoice } from './settings';
import { shortcutLabel } from './shortcuts';

type Tab = 'keys' | 'theme' | 'canvas';

const TABS: { id: Tab; title: string }[] = [
  { id: 'keys', title: 'Горячие клавиши' },
  { id: 'theme', title: 'Тема' },
  { id: 'canvas', title: 'Холст' },
];

/** Раскладка без отличий: так выглядит файл, если у пользователя всё по умолчанию. */
const EMPTY_KEYMAP = '{"format":"ferrum-keymap","version":1,"overrides":{}}';

export interface SettingsDialogProps {
  theme: ThemeChoice;
  onTheme: (t: ThemeChoice) => void;
  pixelSnap: boolean;
  onPixelSnap: (on: boolean) => void;
  onClose: () => void;
}

export function SettingsDialog(props: SettingsDialogProps) {
  const ed = useEditor();
  const [tab, setTab] = createSignal<Tab>('keys');
  let el!: HTMLDivElement;
  const returnFocus = document.activeElement as HTMLElement | null;
  const close = () => {
    // Вопрос о замене относится к этому диалогу и в следующем открытии не нужен.
    ed.store.setState('bindConflict', null);
    props.onClose();
    returnFocus?.focus?.();
  };
  const selectedTab = () => el.querySelector<HTMLElement>('[role="tab"][aria-selected="true"]');

  onMount(() => {
    selectedTab()?.focus();
    // Esc — на окне: фокус мог остаться на кнопке, которую уже убрали. Запись
    // сочетания забирает Esc себе раньше (захват, см. KeymapTab); вопрос о замене
    // закрывается первым.
    const key = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || e.defaultPrevented) return;
      e.preventDefault();
      if (ed.store.state.bindConflict) {
        ed.store.setState('bindConflict', null);
        el.querySelector<HTMLElement>('input[type="search"]')?.focus();
      } else close();
    };
    // Фокус не уходит из диалога (aria-modal): ушёл — возвращается к выбранной вкладке.
    const trap = (e: FocusEvent) => {
      if (e.target instanceof Node && !el.contains(e.target)) selectedTab()?.focus();
    };
    window.addEventListener('keydown', key);
    document.addEventListener('focusin', trap);
    onCleanup(() => {
      window.removeEventListener('keydown', key);
      document.removeEventListener('focusin', trap);
    });
  });

  return (
    <Portal>
      <div class="fe-scrim" onPointerDown={(e) => e.target === e.currentTarget && close()}>
        <div
          ref={el}
          class="fe-dialog fe-dialog--settings"
          role="dialog"
          aria-modal="true"
          aria-labelledby="fe-settings-title"
          tabindex="-1"
        >
          <div class="fe-dialog__head">
            <h1 class="t-heading" id="fe-settings-title">
              Настройки
            </h1>
            <button class="fe-tool fe-tool--small" aria-label="Закрыть настройки" onClick={close}>
              <Icon name="close" />
            </button>
          </div>
          <div class="fe-tabs" role="tablist" aria-label="Разделы настроек">
            <For each={TABS}>
              {(t) => (
                <button
                  class="fe-tab"
                  role="tab"
                  id={`settings-tab-${t.id}`}
                  aria-selected={tab() === t.id}
                  aria-controls={`settings-${t.id}`}
                  tabindex={tab() === t.id ? 0 : -1}
                  onClick={() => setTab(t.id)}
                  onKeyDown={(e) => {
                    const i = TABS.findIndex((x) => x.id === tab());
                    const next =
                      e.key === 'ArrowRight'
                        ? (i + 1) % TABS.length
                        : e.key === 'ArrowLeft'
                          ? (i - 1 + TABS.length) % TABS.length
                          : e.key === 'Home'
                            ? 0
                            : e.key === 'End'
                              ? TABS.length - 1
                              : -1;
                    if (next < 0) return;
                    e.preventDefault();
                    setTab(TABS[next].id);
                    el.querySelector<HTMLElement>(`#settings-tab-${TABS[next].id}`)?.focus();
                  }}
                >
                  {t.title}
                </button>
              )}
            </For>
          </div>
          <div class="fe-dialog__body" role="tabpanel" id={`settings-${tab()}`} aria-labelledby={`settings-tab-${tab()}`}>
            <Show when={tab() === 'keys'}>
              <KeymapTab />
            </Show>
            <Show when={tab() === 'theme'}>
              <Radio
                label="Тема интерфейса"
                value={props.theme}
                options={[
                  { value: 'dark', label: 'Тёмная', icon: 'moon' },
                  { value: 'light', label: 'Светлая', icon: 'sun' },
                  { value: 'system', label: 'Как в системе', icon: 'monitor' },
                ]}
                onChange={(v) => props.onTheme(v as ThemeChoice)}
              />
            </Show>
            <Show when={tab() === 'canvas'}>
              <label class="fe-check fe-check--block">
                <input type="checkbox" checked={props.pixelSnap} onChange={(e) => props.onPixelSnap(e.currentTarget.checked)} />
                <span>
                  <span class="t-strong">Привязка к пикселям</span>
                  <span class="t-muted fe-hint">Положение и размеры при перетаскивании округляются до целых пикселей</span>
                </span>
              </label>
            </Show>
          </div>
        </div>
      </div>
    </Portal>
  );
}

function Radio(props: {
  label: string;
  value: string;
  options: { value: string; label: string; icon: IconName }[];
  onChange: (v: string) => void;
}): JSX.Element {
  return (
    <div class="fe-radio" role="radiogroup" aria-label={props.label}>
      <For each={props.options}>
        {(o) => (
          <label class="fe-radio__item" classList={{ 'fe-radio__item--on': props.value === o.value }}>
            <input type="radio" name={props.label} checked={props.value === o.value} onChange={() => props.onChange(o.value)} />
            <Icon name={o.icon} />
            <span>{o.label}</span>
          </label>
        )}
      </For>
    </div>
  );
}

function KeymapTab() {
  const ed = useEditor();
  const km = () => ed.store.state.keymap;
  const [query, setQuery] = createSignal('');
  const [recording, setRecording] = createSignal<string | null>(null);
  const [note, setNote] = createSignal('');
  const groups = createMemo(() => filterGroups(groupCommands(km().commands), query(), ed.platform, ed.layout()));
  const label = (s: string) => shortcutLabel(s, ed.platform, ed.layout());
  const name = (key: string) => km().commands.find((c) => c.key === key)?.name ?? key;
  const conflict = () => ed.store.state.bindConflict;
  let fileInput!: HTMLInputElement;
  let search!: HTMLInputElement;
  let root!: HTMLDivElement;
  // Вопрос о замене исчезает вместе с кнопкой в фокусе: фокус — обратно в поиск.
  const closeConflict = () => {
    ed.store.setState('bindConflict', null);
    search.focus();
  };

  const stopRecording = (command: string) => {
    setRecording(null);
    setNote('');
    // Кнопка «+» вернулась на место: фокус — на неё, а не на body.
    queueMicrotask(() => root?.querySelector<HTMLElement>(`[data-add="${CSS.escape(command)}"]`)?.focus());
  };

  // Запись сочетания: клавиши ловятся до всех (захват на окне) и дальше не идут —
  // ни холсту, ни диалогу (Esc отменяет запись и не закрывает диалог), ни браузеру.
  const onRecord = (e: KeyboardEvent) => {
    const command = recording();
    if (!command) return;
    e.preventDefault();
    e.stopPropagation();
    const r = recordKey(e, ed.platform);
    if (r.kind === 'wait') return;
    if (r.kind === 'altgr') {
      setNote('Сочетания с AltGr не сработают. Нажмите другое');
      return;
    }
    if (r.kind === 'never') {
      setNote('Это сочетание не сработает: его забирает браузер. Нажмите другое');
      return;
    }
    stopRecording(command);
    if (r.kind === 'shortcut') ed.send({ type: 'bind', command, shortcut: r.shortcut, replace: false });
  };
  // Щелчок или уход окна из фокуса прерывают запись: она не должна висеть вечно.
  const onAway = () => {
    const command = recording();
    if (command) stopRecording(command);
  };
  onMount(() => {
    window.addEventListener('keydown', onRecord, true);
    window.addEventListener('pointerdown', onAway, true);
    window.addEventListener('blur', onAway);
  });
  onCleanup(() => {
    window.removeEventListener('keydown', onRecord, true);
    window.removeEventListener('pointerdown', onAway, true);
    window.removeEventListener('blur', onAway);
  });

  // Файл — недоверенный: размер ограничен до чтения, любая ошибка чтения — одно
  // сообщение, разбор и проверку содержимого делает ядро.
  const importFile = async (file: File | undefined) => {
    if (!file) return;
    if (file.size > MAX_KEYMAP_FILE) {
      ed.store.toast(`Файл раскладки слишком большой: больше ${kb(MAX_KEYMAP_FILE)} КБ`, 'error');
      return;
    }
    let json: string;
    try {
      json = await file.text();
    } catch {
      ed.store.toast('Не удалось прочитать файл раскладки', 'error');
      return;
    }
    ed.send({ type: 'importKeymap', json });
  };

  const focusAdd = (command: string) =>
    root.querySelector<HTMLElement>(`[data-add="${CSS.escape(command)}"]`)?.focus();
  // Убранное сочетание уносит кнопку с фокусом: фокус — соседнему сочетанию или на «+».
  const removeBinding = (c: KeymapEntry, b: string) => {
    const i = c.bindings.indexOf(b);
    const next = c.bindings[i + 1] ?? c.bindings[i - 1];
    const target = next && root.querySelector<HTMLElement>(`[data-remove="${CSS.escape(`${c.key}|${next}`)}"]`);
    if (target) target.focus();
    else focusAdd(c.key);
    ed.send({ type: 'unbind', command: c.key, shortcut: b });
  };

  const row = (c: KeymapEntry) => (
    <div class="fe-keyrow" role="row">
      <span class="fe-keyrow__name" role="cell">
        {c.name}
      </span>
      <span class="fe-keyrow__keys" role="cell">
        <For each={c.bindings}>
          {(b) => {
            const others = () => conflictsOf(c, b, km().conflicts);
            const reserved = () => reservedOf(c, km().reserved).includes(b);
            return (
              <span class="fe-key" classList={{ 'fe-key--warn': reserved() || others().length > 0 }}>
                <span class="t-num">{label(b)}</span>
                {/* Отмена по Esc: единственный выход из выделения, случайно её не убрать. */}
                <Show when={!(c.key === 'edit.escape' && b === 'Escape')}>
                  <button
                    class="fe-key__remove"
                    aria-label={`Убрать ${label(b)} у команды «${c.name}»`}
                    data-remove={`${c.key}|${b}`}
                    onClick={() => removeBinding(c, b)}
                  >
                    <Icon name="close" size={12} />
                  </button>
                </Show>
                <Show when={reserved()}>
                  <span class="fe-key__note">
                    <Icon name="warning" size={12} /> Перехватывает браузер
                  </span>
                </Show>
                <Show when={others().length > 0}>
                  <span class="fe-key__note">
                    <Icon name="warning" size={12} /> Конфликт: {others().map(name).join(', ')}
                  </span>
                </Show>
              </span>
            );
          }}
        </For>
        <Show
          when={recording() === c.key}
          fallback={
            <button
              class="fe-tool fe-tool--small fe-tool--muted"
              aria-label={`Добавить сочетание для «${c.name}»`}
              data-tip="Добавить сочетание"
              data-add={c.key}
              onClick={() => {
                setNote('');
                setRecording(c.key);
              }}
            >
              <Icon name="plus" />
            </button>
          }
        >
          <span
            class="fe-key fe-key--recording"
            data-recording="true"
            role="status"
            tabindex="-1"
            ref={(s) => queueMicrotask(() => s.focus())}
          >
            {note() || 'Нажмите сочетание…'}
          </span>
        </Show>
        <Show when={c.overridden}>
          <button
            class="fe-btn fe-btn--small"
            onClick={() => {
              // «Сбросить» исчезнет вместе с отличием: фокус — на «+» строки.
              focusAdd(c.key);
              ed.send({ type: 'resetKeymap', command: c.key });
            }}
          >
            Сбросить
          </button>
        </Show>
      </span>
    </div>
  );

  return (
    <div class="fe-keymap" ref={root}>
      <label class="fe-field fe-field--text">
        <Icon name="search" class="t-muted fe-field__icon" />
        <input
          ref={search}
          type="search"
          placeholder="Поиск команды или сочетания"
          aria-label="Поиск команд"
          value={query()}
          onInput={(e) => setQuery(e.currentTarget.value)}
          onKeyDown={(e) => {
            // Esc в непустом поиске очищает его; закрывает диалог уже следующий Esc.
            if (e.key === 'Escape' && query()) {
              e.preventDefault();
              setQuery('');
            }
          }}
        />
      </label>
      <Show when={conflict()}>
        {(cf) => (
          <div class="fe-confirm" role="alertdialog" aria-label="Сочетание занято">
            <Icon name="warning" />
            <span class="fe-row__grow">
              {label(cf().shortcut)} уже у команды «{cf().commands.map(name).join('», «')}». Назначить «{name(cf().command)}»?
            </span>
            <button
              class="fe-btn fe-btn--secondary"
              ref={(b) => queueMicrotask(() => b.focus())}
              onClick={() => {
                ed.send({ type: 'bind', command: cf().command, shortcut: cf().shortcut, replace: true });
                closeConflict();
              }}
            >
              Заменить
            </button>
            <button class="fe-btn" onClick={closeConflict}>
              Отмена
            </button>
          </div>
        )}
      </Show>
      <Show when={recording()}>
        <p class="t-muted fe-hint">Некоторые сочетания перехватывает браузер</p>
      </Show>
      <div class="fe-keymap__table" role="table" aria-label="Горячие клавиши">
        {/* По позиции: группы пересобираются при каждом событии, строки внутри — по ключу команды. */}
        <Index each={groups()}>
          {(g) => (
            <div class="fe-keymap__group">
              <h2 class="fe-section__title t-muted">{g().title}</h2>
              <div role="rowgroup" aria-label={g().title}>
                <For each={g().commands}>{row}</For>
              </div>
            </div>
          )}
        </Index>
      </div>
      <div class="fe-dialog__foot">
        <button class="fe-btn fe-btn--danger" onClick={() => ed.send({ type: 'resetKeymap', command: undefined })}>
          Сбросить все
        </button>
        <span class="fe-row__grow" />
        <input
          ref={fileInput}
          type="file"
          accept="application/json,.json"
          hidden
          onChange={(e) => {
            void importFile(e.currentTarget.files?.[0]);
            e.currentTarget.value = '';
          }}
        />
        <button class="fe-btn fe-btn--secondary" onClick={() => fileInput.click()}>
          <Icon name="upload" />
          Импорт…
        </button>
        <button
          class="fe-btn fe-btn--secondary"
          onClick={() =>
            downloadBytes('ferrum-keymap.json', new TextEncoder().encode(km().overrides || EMPTY_KEYMAP), 'application/json')
          }
        >
          <Icon name="download" />
          Экспорт
        </button>
      </div>
    </div>
  );
}
