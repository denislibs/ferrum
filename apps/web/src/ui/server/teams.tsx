// Команды и проекты на экране файлов (спека подсистемы 4, раздел 6.2): строки панели разделов и диалог
// команды с участниками и проектами. Список команд один на страницу: он приходит с сервера и обновляется
// при каждом изменении, которое сделано здесь, и при возврате на экран.

import { MAX_TEAM_NAME_CHARS, parseEmail, type TeamDto, type TeamMembersDto } from '@ferrum/protocol';
import { createSignal, For, onCleanup, Show } from 'solid-js';
import type { Section } from '../../core/file-list';
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

const [teams, setTeams] = createSignal<TeamDto[]>([]);
/** Поколение: ответ на прежний запрос списка не затирает новый. */
let teamsGeneration = 0;

export async function refreshTeams(): Promise<string | null> {
  const my = ++teamsGeneration;
  const r = await getTeams();
  if (my !== teamsGeneration) return null;
  if (!r.ok) return apiFailureText(r.failure);
  setTeams(r.value.teams);
  return null;
}

export function teamNavEntries(): NavEntry[] {
  const out: NavEntry[] = [];
  for (const t of teams()) {
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
  for (const t of teams()) {
    const p = t.projects.find((x) => x.id === id);
    if (p) return p.name;
  }
  return 'Проект';
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
  const team = () => teams().find((t) => t.id === selected()) ?? null;
  void refreshTeams().then((e) => !closed && e && setError(e));
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
    await refreshTeams();
    if (!closed) setSelected(r.value.id);
  };
  return (
    <Modal title="Команды" onClose={props.onClose}>
      <Show when={error()}>
        <p class="fe-share__error" role="alert">
          {error()}
        </p>
      </Show>
      <Show
        when={team()}
        fallback={
          <>
            <ul class="fe-share__people" aria-label="Ваши команды">
              <For each={teams()} fallback={<li class="t-muted">Команд пока нет</li>}>
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
        keyed
      >
        {(t) => <TeamPage team={t} onBack={() => setSelected(null)} />}
      </Show>
      <div class="fe-dialog__actions">
        <button class="fe-btn" onClick={props.onClose}>
          Закрыть
        </button>
      </div>
    </Modal>
  );
}

function TeamPage(props: { team: TeamDto; onBack: () => void }) {
  const id = props.team.id;
  const owner = () => props.team.role === 'owner';
  const [members, setMembers] = createSignal<TeamMembersDto['members'] | null>(null);
  const [email, setEmail] = createSignal('');
  const [project, setProject] = createSignal('');
  const [teamName, setTeamName] = createSignal(props.team.name);
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
  void run(() => getMembers(id), showMembers);
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
      <Show when={owner()} fallback={<h3 class="t-strong">{props.team.name}</h3>}>
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
        <For each={props.team.projects} fallback={<li class="t-muted">Проектов пока нет</li>}>
          {(p) => (
            <li class="fe-share__person">
              <span class="fe-share__who">{p.name}</span>
              <Show when={owner()}>
                <button
                  class="fe-btn"
                  aria-label={`Удалить проект: ${p.name}`}
                  disabled={busy()}
                  onClick={() =>
                    void run(
                      () => deleteProject(p.id),
                      () => void refreshTeams(),
                    )
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
        <For each={members() ?? []}>
          {(m) => (
            <li class="fe-share__person">
              <span class="fe-share__who t-muted">{m.email}</span>
              <span class="t-muted">{m.role === 'owner' ? 'Владелец' : 'Участник'}</span>
              <Show when={m.role !== 'owner' && owner()}>
                <button
                  class="fe-btn"
                  aria-label={`Убрать: ${m.email}`}
                  disabled={busy()}
                  onClick={() => void run(() => removeMember(id, m.id), showMembers)}
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
            void run(
              () => deleteTeam(id),
              () => {
                void refreshTeams();
                props.onBack();
              },
            )
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
            <For each={teams().flatMap((t) => t.projects.map((p) => ({ ...p, team: t.name })))}>
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
