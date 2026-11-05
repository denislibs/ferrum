// Команды и проекты на экране файлов (спека подсистемы 4, раздел 6.2): строки панели разделов и диалог
// команды с участниками и проектами. Список команд один на страницу: он приходит с сервера и обновляется
// при каждом изменении, которое сделано здесь, и при возврате на экран.

import { MAX_TEAM_NAME_CHARS, parseEmail, type TeamDto, type TeamMembersDto } from '@ferrum/protocol';
import { createSignal, For, onCleanup, Show } from 'solid-js';
import type { Section } from '../../core/file-list';
import { ConfirmDialog } from '../dialog';
import type { NavEntry } from '../extensions';
import type { MenuItem } from '../menu';
import type { FileMeta } from '../../core/file-list';
import { Modal } from './modal';
import {
  apiFailureText,
  createProject,
  createTeam,
  deleteProject,
  deleteTeam,
  getMembers,
  getTeams,
  inviteMember,
  moveFile,
  removeMember,
  renameTeam,
  type ApiResult,
} from './server-api';

/** `null` — список ещё ни разу не пришёл. */
const [teams, setTeams] = createSignal<TeamDto[] | null>(null);
/** Последнее чтение не удалось: прежний список остаётся, панель и диалог говорят об этом. */
const [teamsFailure, setTeamsFailure] = createSignal<string | null>(null);
/** Поколение: ответ на прежний запрос списка не затирает новый. */
let teamsGeneration = 0;

export async function refreshTeams(): Promise<void> {
  const my = ++teamsGeneration;
  const r = await getTeams();
  if (my !== teamsGeneration) return;
  if (!r.ok) {
    setTeamsFailure(apiFailureText(r.failure));
    return;
  }
  setTeamsFailure(null);
  setTeams(r.value.teams);
}

export function teamNavEntries(): NavEntry[] {
  const out: NavEntry[] = [];
  for (const t of teams() ?? []) {
    out.push({ kind: 'label', title: t.name });
    for (const p of t.projects) out.push({ kind: 'section', id: `project:${p.id}`, title: p.name, icon: 'file' });
  }
  out.push({ kind: 'section', id: 'shared', title: 'Доступные мне', icon: 'file' });
  return out;
}

export function teamSectionTitle(section: Section): string | null {
  if (section === 'shared') return 'Доступные мне';
  if (!section.startsWith('project:')) return null;
  const id = section.slice(8);
  const list = teams();
  // Список ещё не пришёл (или не пришёл совсем): имени пока нет, и говорить, что проекта нет, рано.
  if (!list) return 'Проект';
  for (const t of list) {
    const p = t.projects.find((x) => x.id === id);
    if (p) return p.name;
  }
  return 'Проект удалён или недоступен';
}

/** Панель разделов: кнопка команд. Команды читаются при появлении экрана файлов. */
export function TeamsEntry() {
  const [open, setOpen] = createSignal(false);
  void refreshTeams();
  return (
    <div class="fe-files__extra">
      <button class="fe-btn fe-files__extra-btn" onClick={() => setOpen(true)}>
        Команды…
      </button>
      <Show when={teamsFailure()}>
        <p class="fe-files__extra-note" role="alert">
          Команды не загрузились{' '}
          <button class="fe-btn" onClick={() => void refreshTeams()}>
            Повторить
          </button>
        </p>
      </Show>
      <Show when={open()}>
        <TeamsDialog onClose={() => setOpen(false)} />
      </Show>
    </div>
  );
}

function TeamsDialog(props: { onClose: () => void }) {
  const [selected, setSelected] = createSignal<string | null>(null);
  const [name, setName] = createSignal('');
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  let closed = false;
  onCleanup(() => (closed = true));
  const team = () => teams()?.find((t) => t.id === selected()) ?? null;
  /** Вопрос перед необратимым действием: окно поверх диалога, вне его обработчиков клавиш. */
  const [ask, setAsk] = createSignal<Ask | null>(null);
  void refreshTeams();
  const add = async (e: Event) => {
    e.preventDefault();
    if (!name().trim()) return;
    setBusy(true);
    setError(null);
    const r = await createTeam(name());
    if (closed) return;
    setBusy(false);
    if (!r.ok) {
      setError(apiFailureText(r.failure));
      return;
    }
    setName('');
    // Страница откроется, когда команда появится в списке; не загрузился — сообщение и «Повторить».
    setSelected(r.value.id);
    await refreshTeams();
  };
  return (
    <>
      <Modal title="Команды" onClose={props.onClose}>
        <Show when={error()}>
          <p class="fe-share__error" role="alert">
            {error()}
          </p>
        </Show>
        <Show when={teamsFailure()}>
          <p class="fe-share__error" role="alert">
            {teamsFailure()}{' '}
            <button class="fe-btn fe-btn--secondary" onClick={() => void refreshTeams()}>
              Повторить
            </button>
          </p>
        </Show>
        <Show
          when={team()}
          fallback={
            <>
              <Show
                when={teams()}
                fallback={
                  <Show when={!teamsFailure()}>
                    <p class="t-muted">Загрузка…</p>
                  </Show>
                }
              >
                {(list) => (
                  <ul class="fe-share__people" aria-label="Ваши команды">
                    <For each={list()} fallback={<li class="t-muted">Команд пока нет</li>}>
                      {(t) => (
                        <li class="fe-share__person">
                          <span class="fe-share__who t-strong">{t.name}</span>
                          <button class="fe-btn fe-btn--secondary" onClick={() => setSelected(t.id)}>
                            Открыть
                          </button>
                        </li>
                      )}
                    </For>
                  </ul>
                )}
              </Show>
              <form class="fe-share__invite" novalidate onSubmit={(e) => void add(e)}>
                <div class="fe-field fe-field--text fe-share__email">
                  <input
                    aria-label="Имя новой команды"
                    placeholder="Новая команда"
                    maxlength={MAX_TEAM_NAME_CHARS}
                    value={name()}
                    onInput={(e) => setName(e.currentTarget.value)}
                  />
                </div>
                <button class="fe-btn fe-btn--primary" type="submit" disabled={busy()}>
                  Создать
                </button>
              </form>
            </>
          }
        >
          {/* Без `keyed`: новый объект команды из списка не пересоздаёт страницу, ввод и фокус остаются. */}
          {(t) => <TeamPage team={t} ask={setAsk} onBack={() => setSelected(null)} />}
        </Show>
        <div class="fe-dialog__actions">
          <button class="fe-btn" onClick={props.onClose}>
            Закрыть
          </button>
        </div>
      </Modal>
      <Show when={ask()}>
        {(a) => (
          <ConfirmDialog
            title={a().title}
            text={a().text}
            confirm={a().confirm}
            cancel="Отмена"
            onCancel={() => setAsk(null)}
            onConfirm={() => {
              const go = a().go;
              setAsk(null);
              go();
            }}
          />
        )}
      </Show>
    </>
  );
}

interface Ask {
  title: string;
  text: string;
  confirm: string;
  go: () => void;
}

function TeamPage(props: { team: () => TeamDto; ask: (a: Ask) => void; onBack: () => void }) {
  const id = props.team().id;
  const owner = () => props.team().role === 'owner';
  const [members, setMembers] = createSignal<TeamMembersDto['members'] | null>(null);
  const [email, setEmail] = createSignal('');
  const [project, setProject] = createSignal('');
  const [teamName, setTeamName] = createSignal(props.team().name);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  let generation = 0;
  let closed = false;
  onCleanup(() => (closed = true));
  async function run<T>(work: () => Promise<ApiResult<T>>, done?: (v: T) => void): Promise<void> {
    const my = ++generation;
    setBusy(true);
    setError(null);
    const r = await work();
    if (closed || my !== generation) return;
    setBusy(false);
    if (r.ok) done?.(r.value);
    else setError(apiFailureText(r.failure));
  }
  const showMembers = (v: TeamMembersDto) => setMembers(v.members);
  const loadMembers = () => void run(() => getMembers(id), showMembers);
  loadMembers();
  return (
    <>
      <button class="fe-btn" onClick={props.onBack}>
        ← Все команды
      </button>
      <Show when={error()}>
        <p class="fe-share__error" role="alert">
          {error()}
        </p>
      </Show>
      <Show when={owner()} fallback={<h3 class="t-strong">{props.team().name}</h3>}>
        <form
          class="fe-share__invite"
          novalidate
          onSubmit={(e) => {
            e.preventDefault();
            if (teamName().trim())
              void run(
                () => renameTeam(id, teamName()),
                () => void refreshTeams(),
              );
          }}
        >
          <div class="fe-field fe-field--text fe-share__email">
            <input
              aria-label="Имя команды"
              maxlength={MAX_TEAM_NAME_CHARS}
              value={teamName()}
              onInput={(e) => setTeamName(e.currentTarget.value)}
            />
          </div>
          <button class="fe-btn fe-btn--secondary" type="submit" disabled={busy()}>
            Переименовать
          </button>
        </form>
      </Show>
      <h3 class="t-strong">Проекты</h3>
      <ul class="fe-share__people" aria-label="Проекты команды">
        <For each={props.team().projects} fallback={<li class="t-muted">Проектов пока нет</li>}>
          {(p) => (
            <li class="fe-share__person">
              <span class="fe-share__who">{p.name}</span>
              <Show when={owner()}>
                <button
                  class="fe-btn"
                  aria-label={`Удалить проект: ${p.name}`}
                  disabled={busy()}
                  onClick={() =>
                    props.ask({
                      title: 'Удалить проект?',
                      text: `Проект «${p.name}» будет удалён без возможности восстановления`,
                      confirm: 'Удалить',
                      go: () =>
                        void run(
                          () => deleteProject(p.id),
                          () => void refreshTeams(),
                        ),
                    })
                  }
                >
                  Удалить
                </button>
              </Show>
            </li>
          )}
        </For>
      </ul>
      <form
        class="fe-share__invite"
        novalidate
        onSubmit={(e) => {
          e.preventDefault();
          if (!project().trim()) return;
          void run(
            () => createProject(id, project()),
            () => {
              setProject('');
              void refreshTeams();
            },
          );
        }}
      >
        <div class="fe-field fe-field--text fe-share__email">
          <input
            aria-label="Имя нового проекта"
            placeholder="Новый проект"
            maxlength={MAX_TEAM_NAME_CHARS}
            value={project()}
            onInput={(e) => setProject(e.currentTarget.value)}
          />
        </div>
        <button class="fe-btn fe-btn--secondary" type="submit" disabled={busy()}>
          Добавить проект
        </button>
      </form>
      <h3 class="t-strong">Участники</h3>
      <ul class="fe-share__people" aria-label="Участники команды">
        <For
          each={members() ?? []}
          fallback={
            <li class="t-muted">
              <Show when={!busy() && error()} fallback="Загрузка…">
                Список не загрузился{' '}
                <button class="fe-btn fe-btn--secondary" onClick={loadMembers}>
                  Повторить
                </button>
              </Show>
            </li>
          }
        >
          {(m) => (
            <li class="fe-share__person">
              <span class="fe-share__who t-muted">{m.email}</span>
              <span class="t-muted">{m.role === 'owner' ? 'Владелец' : 'Участник'}</span>
              <Show when={m.role !== 'owner' && owner()}>
                <button
                  class="fe-btn"
                  aria-label={`Убрать: ${m.email}`}
                  disabled={busy()}
                  onClick={() =>
                    props.ask({
                      title: 'Убрать участника?',
                      text: `${m.email} потеряет доступ к файлам команды, а его файлы перейдут владельцу команды`,
                      confirm: 'Убрать',
                      go: () => void run(() => removeMember(id, m.id), showMembers),
                    })
                  }
                >
                  Убрать
                </button>
              </Show>
            </li>
          )}
        </For>
      </ul>
      <Show when={owner()}>
        <form
          class="fe-share__invite"
          novalidate
          onSubmit={(e) => {
            e.preventDefault();
            const parsed = parseEmail(email());
            if (!parsed) {
              setError('Проверьте адрес');
              return;
            }
            void run(
              () => inviteMember(id, parsed),
              (v) => {
                setEmail('');
                showMembers(v);
              },
            );
          }}
        >
          <div class="fe-field fe-field--text fe-share__email">
            <input
              type="email"
              aria-label="Адрес нового участника"
              placeholder="Электронная почта"
              autocomplete="off"
              value={email()}
              onInput={(e) => setEmail(e.currentTarget.value)}
            />
          </div>
          <button class="fe-btn fe-btn--primary" type="submit" disabled={busy()}>
            Пригласить
          </button>
        </form>
        <button
          class="fe-btn"
          disabled={busy()}
          onClick={() =>
            props.ask({
              title: 'Удалить команду?',
              text: `Команда «${props.team().name}» будет удалена без возможности восстановления`,
              confirm: 'Удалить',
              go: () =>
                void run(
                  () => deleteTeam(id),
                  () => {
                    void refreshTeams();
                    props.onBack();
                  },
                ),
            })
          }
        >
          Удалить команду
        </button>
      </Show>
    </>
  );
}

/** Пункт «Переместить…» меню файла: владельцу, когда есть куда. Диалог живёт в подвале панели,
 * потому что меню закрывается раньше. */
const [moving, setMoving] = createSignal<FileMeta | null>(null);

export function fileMenuItems(file: FileMeta): MenuItem[] {
  if (file.role !== 'owner') return [];
  return [{ label: 'Переместить…', onSelect: () => setMoving(file) }];
}

export function MoveDialog() {
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const go = async (file: FileMeta, project: string | null) => {
    setBusy(true);
    setError(null);
    const r = await moveFile(file.id, project);
    setBusy(false);
    if (r.ok) {
      setMoving(null);
      onMoved?.();
    } else setError(apiFailureText(r.failure));
  };
  return (
    <Show when={moving()} keyed>
      {(file) => (
        <Modal title={`Переместить: ${file.name}`} onClose={() => setMoving(null)}>
          <Show when={error()}>
            <p class="fe-share__error" role="alert">
              {error()}
            </p>
          </Show>
          <ul class="fe-share__people" aria-label="Куда переместить">
            <li class="fe-share__person">
              <span class="fe-share__who">Черновики</span>
              <button class="fe-btn fe-btn--secondary" disabled={busy() || !file.projectId} onClick={() => void go(file, null)}>
                Сюда
              </button>
            </li>
            <For each={(teams() ?? []).flatMap((t) => t.projects.map((p) => ({ ...p, team: t.name })))}>
              {(p) => (
                <li class="fe-share__person">
                  <span class="fe-share__who">
                    {p.team}: {p.name}
                  </span>
                  <button class="fe-btn fe-btn--secondary" disabled={busy() || file.projectId === p.id} onClick={() => void go(file, p.id)}>
                    Сюда
                  </button>
                </li>
              )}
            </For>
          </ul>
          <div class="fe-dialog__actions">
            <button class="fe-btn" onClick={() => setMoving(null)}>
              Отмена
            </button>
          </div>
        </Modal>
      )}
    </Show>
  );
}

let onMoved: (() => void) | undefined;
/** Кому сказать, что список изменился. */
export function setOnMoved(fn: () => void): void {
  onMoved = fn;
}
