// Комментарии в редакторе (спека подсистемы 4, раздел 10.3): окно обсуждения у метки, поле нового комментария и панель обсуждений
// справа в режиме комментариев. Метки, их раскладку и попадание по ним ведёт ядро; данные — `sync.worker` (`CommentStore`).
// Здесь только то, что видно: основной поток получает изменившиеся обсуждения пачкой и не пересобирает панель целиком — строки
// по обсуждению, список виртуальный.
//
// Текст комментариев и имена — только текстом (`{…}` Solid), без `innerHTML`.

import { splitCommentBody, type CommentDto, type FileRole, type MentionCandidate, type PinDto, type ThreadView } from '@ferrum/protocol';
import { batch, createMemo, createSignal, For, onCleanup, onMount, Show } from 'solid-js';
import { createStore, reconcile } from 'solid-js/store';
import { Portal } from 'solid-js/web';
import type { CommentsToMain } from '../../core/comments-store';
import type { CommentsProps } from '../extensions';
import { apiFailureText, type ApiFailure } from './server-api';
// Стили комментариев — отсюда, а не из `main.tsx`: в автономную сборку этот файл не попадает, и стили с ним.
import '../styles/comments.css';
import {
  authorName,
  bodyState,
  DEFAULT_FILTER,
  editableBody,
  insertMention,
  listWindow,
  mentionAt,
  mentionName,
  panelThreads,
  popoverPlace,
  storedBody,
  timeLabel,
  type PanelFilter,
} from './comments-logic';

/** Высота строки панели: список виртуальный, строки одной высоты. */
export const PANEL_ROW = 76;
/** Подсказка упоминаний ждёт паузы в наборе. */
const MENTION_DEBOUNCE_MS = 150;

const failText = (f: string) => apiFailureText(f as ApiFailure);

function Body(props: { comment: CommentDto }) {
  return (
    <p class="fe-comment__body">
      <For each={splitCommentBody(props.comment.body, props.comment.mentions)}>
        {(part) => ('mention' in part ? <span class="fe-comment__mention">@{mentionName(part.name)}</span> : <>{part.text}</>)}
      </For>
    </p>
  );
}

interface ComposerProps {
  label: string;
  placeholder: string;
  initial?: { text: string; picked: { id: string; name: string }[] };
  submit: string;
  mentions(q: string): Promise<MentionCandidate[]>;
  onSubmit(body: string): Promise<boolean>;
  onCancel(): void;
}

/** Поле комментария: Enter — отправить, Shift+Enter — новая строка, Esc — отмена; `@` — список людей с доступом к файлу. */
function Composer(props: ComposerProps) {
  const [text, setText] = createSignal(props.initial?.text ?? '');
  const [busy, setBusy] = createSignal(false);
  const [people, setPeople] = createSignal<MentionCandidate[]>([]);
  const [active, setActive] = createSignal(0);
  let picked = [...(props.initial?.picked ?? [])];
  let field!: HTMLTextAreaElement;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let asked = 0;
  const state = createMemo(() => bodyState(storedBody(text(), picked)));
  onMount(() => {
    field.focus();
    field.setSelectionRange(field.value.length, field.value.length);
  });
  onCleanup(() => clearTimeout(timer));

  const lookup = () => {
    clearTimeout(timer);
    const at = mentionAt(field.value, field.selectionStart);
    if (!at) {
      setPeople([]);
      return;
    }
    const mine = ++asked;
    timer = setTimeout(() => {
      void props.mentions(at.query).then((list) => {
        if (mine !== asked) return;
        setPeople(list);
        setActive(0);
      });
    }, MENTION_DEBOUNCE_MS);
  };

  const pick = (p: MentionCandidate) => {
    const at = mentionAt(field.value, field.selectionStart);
    if (!at) return;
    const name = p.name.trim() || 'Участник';
    const next = insertMention(field.value, at.start, field.selectionStart, name);
    if (!picked.some((x) => x.id === p.id)) picked.push({ id: p.id, name });
    setText(next.text);
    field.value = next.text;
    field.setSelectionRange(next.caret, next.caret);
    setPeople([]);
    asked++;
  };

  const send = async () => {
    const body = storedBody(text(), picked).trim();
    if (!bodyState(body).ok || busy()) return;
    // Пока идёт запрос, поле недоступно и теряет фокус: после ответа он возвращается, чтобы писать дальше.
    const focused = document.activeElement === field;
    setBusy(true);
    const ok = await props.onSubmit(body);
    setBusy(false);
    if (ok) {
      setText('');
      picked = [];
      field.value = '';
    }
    if (focused && field.isConnected) field.focus();
  };

  const onKey = (e: KeyboardEvent) => {
    // Клавиши поля — его: холст и команды их не получают.
    e.stopPropagation();
    const list = people();
    if (list.length > 0 && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
      e.preventDefault();
      setActive((i) => (i + (e.key === 'ArrowDown' ? 1 : list.length - 1)) % list.length);
      return;
    }
    if (list.length > 0 && (e.key === 'Enter' || e.key === 'Tab')) {
      e.preventDefault();
      pick(list[active()]);
      return;
    }
    // Esc отменяет ближайшее: список людей, иначе само поле (правку — без закрытия окна). Обработчик окна это событие
    // пропускает (`onKey` в `CommentsUi`).
    if (e.key === 'Escape') {
      e.preventDefault();
      if (list.length > 0) {
        setPeople([]);
        asked++;
      } else props.onCancel();
      return;
    }
    if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
      e.preventDefault();
      void send();
    }
  };

  return (
    <div class="fe-composer">
      <textarea
        ref={field}
        class="fe-composer__field"
        aria-label={props.label}
        placeholder={props.placeholder}
        rows={2}
        value={text()}
        disabled={busy()}
        onInput={(e) => {
          setText(e.currentTarget.value);
          lookup();
        }}
        onKeyDown={onKey}
        onKeyUp={(e) => e.stopPropagation()}
      />
      <Show when={people().length > 0}>
        <ul class="fe-composer__people" role="listbox" aria-label="Упомянуть">
          <For each={people()}>
            {(p, i) => (
              <li
                role="option"
                aria-selected={i() === active()}
                class="fe-composer__person"
                classList={{ 'is-active': i() === active() }}
                onPointerDown={(e) => {
                  e.preventDefault();
                  pick(p);
                }}
              >
                <span class="t-strong">{p.name.trim() || 'Участник'}</span>
                <Show when={p.email}>{(m) => <span class="t-muted">{m()}</span>}</Show>
              </li>
            )}
          </For>
        </ul>
      </Show>
      <div class="fe-composer__row">
        <Show when={state().length > 3800}>
          <span class="t-muted" classList={{ 'fe-composer__over': state().over }}>
            {state().length} / 4000
          </span>
        </Show>
        <button class="fe-btn fe-btn--primary" disabled={!state().ok || busy()} onClick={() => void send()}>
          {props.submit}
        </button>
      </div>
    </div>
  );
}

export function CommentsUi(props: CommentsProps) {
  const [threads, setThreads] = createStore<Record<string, ThreadView>>({});
  const [loaded, setLoaded] = createSignal(false);
  const [me, setMe] = createSignal<{ user: string; role: FileRole } | null>(null);
  const [open, setOpen] = createSignal<string | null>(null);
  const [detail, setDetail] = createSignal<{ view: ThreadView; comments: CommentDto[] } | null>(null);
  const [draft, setDraft] = createSignal<PinDto | null>(null);
  const [anchor, setAnchor] = createSignal({ x: 0, y: 0 });
  const [editing, setEditing] = createSignal<string | null>(null);
  const [confirmDelete, setConfirmDelete] = createSignal<string | null>(null);
  const [filter, setFilter] = createSignal<PanelFilter>(DEFAULT_FILTER);
  const [top, setTop] = createSignal(0);
  const [height, setHeight] = createSignal(600);
  /** Холст (в координатах общего с окном предка) и размер окна: по ним окно держится в пределах холста. */
  const measureCanvas = () => ({
    left: props.canvas.offsetLeft,
    top: props.canvas.offsetTop,
    width: props.canvas.clientWidth,
    height: props.canvas.clientHeight,
  });
  const [box, setBox] = createSignal(measureCanvas());
  const [size, setSize] = createSignal({ width: 320, height: 0 });
  const [now, setNow] = createSignal(Date.now());
  let link = props.link;
  let popover: HTMLDivElement | undefined;
  let panel: HTMLDivElement | undefined;
  /** Щелчок по холсту закрыл окно: место нового комментария из того же щелчка не ставится (как в Figma). Снимается
   * событием этого щелчка (`commentPlace`/`commentOpened`) или следующим нажатием: щелчок мог не дать события вовсе
   * (протяжка метки, панорама, другой инструмент) — тогда следующий щелчок ставит комментарий. */
  let swallowPlace = false;
  /** Первый комментарий этого обсуждения удаляю я: исчезновение обсуждения — моё действие, без тоста. */
  let removing: string | null = null;

  const tick = setInterval(() => setNow(Date.now()), 30_000);
  onCleanup(() => clearInterval(tick));

  const close = () => {
    // Фокус был в окне — после закрытия он у холста: клавиши холста (C, Shift+C, Esc) снова работают. Если щелчок
    // пришёлся на другое поле, браузер переведёт фокус туда уже после этого.
    const inside = !!popover?.contains(document.activeElement);
    batch(() => {
      setOpen(null);
      setDetail(null);
      setDraft(null);
      setEditing(null);
      setConfirmDelete(null);
    });
    props.editor.send({ type: 'commentOpen' });
    props.editor.send({ type: 'commentDraft' });
    void props.request({ k: 'open', thread: null });
    if (inside) props.canvas.focus({ preventScroll: true });
  };

  const openThread = (id: string, sx?: number, sy?: number) => {
    batch(() => {
      setDraft(null);
      setOpen(id);
      setEditing(null);
      setConfirmDelete(null);
      setDetail(null);
      if (sx !== undefined && sy !== undefined) setAnchor({ x: sx, y: sy });
    });
    props.editor.send({ type: 'commentDraft' });
    props.editor.send({ type: 'commentOpen', id });
    void props.request({ k: 'open', thread: id }).then((r) => {
      if (!r.ok && open() === id) {
        props.toast(r.failure === 'NOT_FOUND' ? 'Обсуждение не найдено или нет доступа' : failText(r.failure));
        close();
      }
    });
  };

  const onMessage = (m: CommentsToMain) => {
    if (m.t === 'me') setMe({ user: m.user, role: m.role });
    else if (m.t === 'threads') {
      batch(() => {
        if (m.reset) {
          const all: Record<string, ThreadView> = {};
          for (const t of m.upserts) all[t.id] = t;
          setThreads(reconcile(all));
        } else {
          for (const t of m.upserts) setThreads(t.id, reconcile(t));
          for (const id of m.removed) setThreads(id, undefined!);
        }
        setLoaded(true);
      });
      // Ссылка на обсуждение (`?comment=`): после первого списка — страница, камера, окно.
      if (m.reset && link) {
        const id = link;
        link = undefined;
        const t = threads[id];
        if (!t) props.toast('Обсуждение не найдено или нет доступа');
        else {
          if (t.resolvedAt !== null) setResolved(true);
          // Ссылка открывает режим комментариев с панелью, как в Figma **(поведение)**.
          props.editor.send({ type: 'setTool', tool: 'comment' });
          props.editor.send({ type: 'commentFocus', id });
        }
      }
    } else if (m.t === 'detail') {
      if (m.thread !== open()) return;
      if (!m.view) {
        if (removing !== m.thread) props.toast('Обсуждение удалено');
        removing = null;
        close();
      } else setDetail({ view: m.view, comments: m.comments });
    }
  };
  onCleanup(props.subscribe(onMessage));

  const setResolved = (on: boolean) => {
    setFilter((f) => ({ ...f, resolved: on }));
    props.editor.send({ type: 'commentShowResolved', on });
  };

  onCleanup(
    props.editor.core.on((e) => {
      switch (e.type) {
        case 'commentPlace':
          if (swallowPlace) {
            swallowPlace = false;
            return;
          }
          batch(() => {
            setOpen(null);
            setDetail(null);
            setDraft({ page: e.page, node: e.node ?? null, x: e.x, y: e.y, wx: e.wx, wy: e.wy });
            setAnchor({ x: e.sx, y: e.sy });
          });
          props.editor.send({ type: 'commentOpen' });
          props.editor.send({ type: 'commentDraft', draft: { page: e.page, node: e.node, x: e.x, y: e.y, wx: e.wx, wy: e.wy } });
          void props.request({ k: 'open', thread: null });
          break;
        case 'commentOpened':
          swallowPlace = false;
          openThread(e.id, e.sx, e.sy);
          break;
        case 'commentAnchor':
          if (e.visible) setAnchor({ x: e.sx, y: e.sy });
          break;
        case 'commentMoved':
          void props
            .request({ k: 'move', thread: e.id, pin: { page: e.page, node: e.node ?? null, x: e.x, y: e.y, wx: e.wx, wy: e.wy } })
            .then((r) => {
              if (!r.ok) props.toast(failText(r.failure));
            });
          break;
        case 'tool':
          // Вышли из режима — новое не пишется (открытое обсуждение остаётся, как в Figma).
          if (e.tool !== 'comment' && draft()) close();
          break;
      }
    }),
  );

  // Щелчок мимо окна закрывает его; Esc закрывает окно раньше, чем ядро выйдет из режима.
  const onPointer = (e: PointerEvent) => {
    if (!open() && !draft()) {
      swallowPlace = false;
      return;
    }
    const t = e.target as Node;
    if (popover?.contains(t) || panel?.contains(t)) return;
    swallowPlace = t === props.canvas;
    close();
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key !== 'Escape' || (!open() && !draft())) return;
    // Esc в поле сначала решает поле (список людей, отмена правки): перехват на `window` его не глотает.
    if (e.target instanceof Element && e.target.closest('.fe-composer') && popover?.contains(e.target)) return;
    e.preventDefault();
    e.stopPropagation();
    close();
  };
  document.addEventListener('pointerdown', onPointer, true);
  window.addEventListener('keydown', onKey, true);
  onCleanup(() => {
    document.removeEventListener('pointerdown', onPointer, true);
    window.removeEventListener('keydown', onKey, true);
  });

  const mentions = async (q: string): Promise<MentionCandidate[]> => {
    const r = await props.request({ k: 'mentions', q });
    return r.ok ? ((r.value as MentionCandidate[]) ?? []) : [];
  };

  const write = async (req: Parameters<CommentsProps['request']>[0]): Promise<boolean> => {
    const r = await props.request(req);
    if (!r.ok) props.toast(r.failure === 'THREAD_FULL' ? 'В обсуждении слишком много ответов' : failText(r.failure));
    return r.ok;
  };

  const create = async (body: string) => {
    const pin = draft();
    if (!pin) return false;
    const r = await props.request({ k: 'create', pin, body });
    if (!r.ok) {
      props.toast(failText(r.failure));
      return false;
    }
    const id = (r.value as { thread: { id: string } }).thread.id;
    openThread(id);
    return true;
  };

  const canEdit = (c: CommentDto) => c.author.id !== null && c.author.id === me()?.user;
  const canDelete = (c: CommentDto) => canEdit(c) || me()?.role === 'owner';
  const canResolve = (t: ThreadView) => me()?.role === 'owner' || me()?.role === 'editor' || t.author.id === me()?.user;

  // Окно у метки: справа сверху от неё, целиком в пределах холста; холст и окно меняют размер — окно следует.
  const sizes = new ResizeObserver((entries) => {
    for (const en of entries) {
      if (en.target === props.canvas) setBox(measureCanvas());
      else if (en.target === popover) setSize({ width: popover.offsetWidth, height: popover.offsetHeight });
    }
  });
  sizes.observe(props.canvas);
  onCleanup(() => sizes.disconnect());
  const popoverRef = (el: HTMLDivElement) => {
    if (popover && popover !== el) sizes.unobserve(popover);
    popover = el;
    sizes.observe(el);
  };
  const position = createMemo(() => {
    const p = popoverPlace(anchor(), box(), size());
    return { left: `${p.left}px`, top: `${p.top}px` };
  });

  const rows = createMemo(() => panelThreads(Object.values(threads), filter(), props.page(), me()?.user ?? ''));
  const win = createMemo(() => listWindow(top(), height(), PANEL_ROW, rows().length));

  return (
    <>
      <Show when={draft()}>
        <div ref={popoverRef} class="fe-thread" role="dialog" aria-label="Новый комментарий" style={position()}>
          <Composer
            label="Комментарий"
            placeholder="Добавьте комментарий. @ — упомянуть"
            submit="Отправить"
            mentions={mentions}
            onSubmit={create}
            onCancel={close}
          />
        </div>
      </Show>
      <Show when={open()}>
        {(id) => (
          <div ref={popoverRef} class="fe-thread" role="dialog" aria-label="Обсуждение" style={position()}>
            <div class="fe-thread__head">
              <span class="t-strong">Обсуждение</span>
              <Show when={detail()?.view}>
                {(v) => (
                  <Show when={canResolve(v())}>
                    <button
                      class="fe-btn fe-btn--ghost"
                      onClick={() => void write({ k: 'resolve', thread: id(), resolved: v().resolvedAt === null })}
                    >
                      {v().resolvedAt === null ? 'Закрыть обсуждение' : 'Открыть снова'}
                    </button>
                  </Show>
                )}
              </Show>
              <button class="fe-btn fe-btn--ghost" aria-label="Закрыть окно" onClick={close}>
                ×
              </button>
            </div>
            <Show when={detail()} fallback={<p class="t-muted fe-thread__loading">Загрузка…</p>}>
              {(d) => (
                <>
                  <Show when={d().view.resolvedAt !== null}>
                    <p class="t-muted fe-thread__resolved">Обсуждение закрыто: {authorName(d().view.resolvedBy)}</p>
                  </Show>
                  <ol class="fe-thread__list">
                    <For each={d().comments}>
                      {(c, i) => (
                        <li class="fe-comment">
                          <div class="fe-comment__meta">
                            <span class="t-strong">{authorName(c.author)}</span>
                            <span class="t-muted">
                              {timeLabel(c.createdAt, now())}
                              {c.editedAt !== null ? ' · изменено' : ''}
                            </span>
                          </div>
                          <Show
                            when={editing() === c.id}
                            fallback={
                              <>
                                <Body comment={c} />
                                <div class="fe-comment__actions">
                                  <Show when={canEdit(c)}>
                                    <button class="fe-link" onClick={() => setEditing(c.id)}>
                                      Изменить
                                    </button>
                                  </Show>
                                  <Show when={canDelete(c)}>
                                    <button
                                      class="fe-link"
                                      onClick={() => {
                                        if (confirmDelete() !== c.id) {
                                          setConfirmDelete(c.id);
                                          return;
                                        }
                                        setConfirmDelete(null);
                                        const thread = id();
                                        if (i() === 0) removing = thread;
                                        void write({ k: 'delete', thread, comment: c.id }).then((ok) => {
                                          if (!ok && removing === thread) removing = null;
                                        });
                                      }}
                                    >
                                      {confirmDelete() === c.id ? (i() === 0 ? 'Удалить обсуждение?' : 'Удалить комментарий?') : 'Удалить'}
                                    </button>
                                  </Show>
                                </div>
                              </>
                            }
                          >
                            <Composer
                              label="Изменить комментарий"
                              placeholder=""
                              initial={editableBody(c.body, c.mentions)}
                              submit="Сохранить"
                              mentions={mentions}
                              onSubmit={async (body) => {
                                const ok = await write({ k: 'edit', thread: id(), comment: c.id, body });
                                if (ok) setEditing(null);
                                return ok;
                              }}
                              onCancel={() => setEditing(null)}
                            />
                          </Show>
                        </li>
                      )}
                    </For>
                  </ol>
                  <Show when={editing() === null}>
                    <Composer
                      label="Ответ"
                      placeholder="Ответить. @ — упомянуть"
                      submit="Ответить"
                      mentions={mentions}
                      onSubmit={(body) => write({ k: 'reply', thread: id(), body })}
                      onCancel={close}
                    />
                  </Show>
                </>
              )}
            </Show>
          </div>
        )}
      </Show>
      <Show when={props.mode() && props.panelHost()}>
        {(host) => (
          <Portal mount={host()}>
            <section ref={panel} class="fe-comments" aria-label="Комментарии">
              <div class="fe-comments__head">
                <span class="t-strong">Комментарии</span>
                <select
                  aria-label="Какие обсуждения"
                  value={filter().scope}
                  onChange={(e) => setFilter((f) => ({ ...f, scope: e.currentTarget.value as PanelFilter['scope'] }))}
                >
                  <option value="page">Эта страница</option>
                  <option value="all">Все страницы</option>
                </select>
                <select
                  aria-label="Порядок"
                  value={filter().sort}
                  onChange={(e) => setFilter((f) => ({ ...f, sort: e.currentTarget.value as PanelFilter['sort'] }))}
                >
                  <option value="date">По дате</option>
                  <option value="unread">Сначала непрочитанные</option>
                </select>
              </div>
              <div class="fe-comments__filters">
                <label>
                  <input
                    type="checkbox"
                    checked={filter().mine}
                    onChange={(e) => setFilter((f) => ({ ...f, mine: e.currentTarget.checked }))}
                  />
                  Только мои
                </label>
                <label>
                  <input type="checkbox" checked={filter().resolved} onChange={(e) => setResolved(e.currentTarget.checked)} />
                  Показать закрытые
                </label>
              </div>
              <div
                class="fe-comments__scroll"
                ref={(el) => {
                  // Высота окна списка — по живому размеру: панель растёт вместе с окном браузера.
                  const ro = new ResizeObserver(() => setHeight(el.clientHeight || 600));
                  ro.observe(el);
                  onCleanup(() => ro.disconnect());
                }}
                onScroll={(e) => setTop(e.currentTarget.scrollTop)}
              >
                <Show when={loaded()} fallback={<p class="t-muted fe-comments__empty">Загрузка…</p>}>
                  <Show when={rows().length > 0} fallback={<p class="t-muted fe-comments__empty">Обсуждений нет</p>}>
                    <ul class="fe-comments__list" role="list" style={{ height: `${rows().length * PANEL_ROW}px` }}>
                      <For each={rows().slice(win().start, win().end)}>
                        {(t, i) => (
                          <li
                            class="fe-comments__row"
                            classList={{ 'is-unread': t.unread, 'is-resolved': t.resolvedAt !== null, 'is-open': open() === t.id }}
                            style={{ top: `${(win().start + i()) * PANEL_ROW}px`, height: `${PANEL_ROW}px` }}
                          >
                            <button class="fe-comments__item" onClick={() => props.editor.send({ type: 'commentFocus', id: t.id })}>
                              <span class="fe-comments__who">
                                <span class="t-strong">{authorName(t.author)}</span>
                                <span class="t-muted">{timeLabel(t.lastAt, now())}</span>
                                <Show when={t.unread}>
                                  <span class="fe-comments__dot" aria-hidden="true" />
                                  <span class="fe-visually-hidden">не прочитано</span>
                                </Show>
                              </span>
                              <span class="fe-comments__text">
                                <Body comment={t.first} />
                              </span>
                              <span class="t-muted fe-comments__replies">
                                {t.replies > 0 ? `Ответов: ${t.replies}` : ''}
                                {t.resolvedAt !== null ? ' · закрыто' : ''}
                              </span>
                            </button>
                          </li>
                        )}
                      </For>
                    </ul>
                  </Show>
                </Show>
              </div>
            </section>
          </Portal>
        )}
      </Show>
    </>
  );
}
