// API файлов, команд и «Поделиться» (спека подсистемы 4, раздел 6): адреса, виды запросов и ответов,
// разбор. Разбор не доверяет данным: неверное — `null`. Его используют и сервер (запросы), и клиент
// (ответы: неожиданный вид — это сбой сети, а не `undefined` в интерфейсе).

import { AUTH_ERROR_CODES, parseEmail } from './auth';

export const API_FILES = '/api/files';
export const API_TEAMS = '/api/teams';
export const API_PROJECTS = '/api/projects';

/** Все коды ошибок API: новый код добавляется только здесь, клиент сверяет ответ с этим списком. */
export const API_ERROR_CODES = [
  ...AUTH_ERROR_CODES,
  /** Доступ к файлу есть, но роли не хватает. Нет доступа совсем — `NOT_FOUND`, как у несуществующего. */
  'FORBIDDEN',
  /** Файл записали после того поколения, на котором основана запись. */
  'CONFLICT',
  'TOO_LARGE',
  /** Содержимое — не контейнер `.ferrum` нужного вида. */
  'BAD_CONTAINER',
  /** Байты картинки не совпали с хэшем в адресе. */
  'HASH_MISMATCH',
  /** В команде есть проекты, в проекте — файлы. */
  'NOT_EMPTY',
  /** Содержимое ссылается на картинку, которой у файла нет: сначала её байты, потом запись. */
  'MISSING_IMAGES',
  /** Картинок в файле больше `MAX_IMAGE_REFS`. */
  'TOO_MANY_IMAGES',
  /** Слишком много одновременных загрузок. */
  'BUSY',
  /** «Скачать копию»: журнал документа, который видят люди, в файл не входит (больше предела сервера, сбой сжатия). Файл
   * был бы неполной копией; правки целы на сервере. */
  'INCOMPLETE',
  /** В обсуждении уже `THREAD_COMMENTS_MAX` комментариев или `THREAD_BYTES_MAX` байт текста: ответ или правка не приняты. */
  'THREAD_FULL',
  /** В файле уже `COMMENT_THREADS_MAX` обсуждений: новое не принято. */
  'FILE_FULL',
  /** У человека в файле уже `COMMENT_THREADS_PER_USER_MAX` обсуждений: новое не принято. */
  'AUTHOR_FULL',
] as const;
export type ApiErrorCode = (typeof API_ERROR_CODES)[number];
export interface ApiErrorBody {
  error: ApiErrorCode;
}

/** Предел контейнера — как `MAX_OPEN_BYTES` клиента: больше ядро не откроет. */
export const MAX_CONTENT_BYTES = 512 * 1024 * 1024;
/** Предел картинки — как `MAX_DATA_BYTES` клиента. */
export const MAX_IMAGE_BYTES = 64 * 1024 * 1024;
export const MAX_THUMB_BYTES = 8 * 1024 * 1024;
/** Строк в одном окне списка не больше этого (спека ядра, раздел 10). */
export const MAX_FILES_WINDOW = 200;
export const MAX_FILE_NAME_CHARS = 256;
/** Имя команды и проекта. */
export const MAX_TEAM_NAME_CHARS = 100;
export const MAX_QUERY_CHARS = 256;
/** Токен ссылки «Поделиться»: 32 байта в base64url. */
const LINK_TOKEN_RE = /^[A-Za-z0-9_-]{43}$/;
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const HASH_RE = /^[0-9a-f]{64}$/;
// eslint-disable-next-line no-control-regex
const CONTROL_RE = /[\u0000-\u001f\u007f]/;

export type FileRole = 'owner' | 'editor' | 'viewer';
/** Роль, которую можно выдать: владелец у файла один. */
export type ShareRole = 'editor' | 'viewer';
export const ROLE_RANK: Record<FileRole, number> = { viewer: 1, editor: 2, owner: 3 };
export const RANK_ROLE: readonly (FileRole | null)[] = [null, 'viewer', 'editor', 'owner'];

export type FileSection = 'recent' | 'drafts' | 'shared' | 'trash' | `project:${string}`;

export interface FileMetaDto {
  id: string;
  name: string;
  createdAt: number;
  editedAt: number;
  /** Последнее открытие этим человеком; 0 — ещё не открывал. */
  openedAt: number;
  size: number;
  trashedAt?: number;
  thumbVersion: number;
  version: number;
  /** Действующая роль этого человека. */
  role: FileRole;
  projectId?: string;
}

export interface FileListDto {
  total: number;
  rows: FileMetaDto[];
}

/** Лёгкая проверка: роль и поколение. Её результат — и отказ на сохранении, и опрос открытого файла. */
export interface FileAccessDto {
  role: FileRole;
  version: number;
  /** Можно звать людей и видеть их адреса: прямая роль или через команду не ниже редактора.
   * Пришедшему только по ссылке — нет, даже редактору. Сервер шлёт всегда. */
  canShare?: boolean;
}

export interface SharePersonDto {
  /** Запись списка: и у человека с доступом, и у приглашённого без учётной записи. */
  id: string;
  email: string;
  /** Имя — только если человек уже открывал файл: иначе список выдавал бы, есть ли адрес в системе. */
  name: string | null;
  role: FileRole;
}

export interface ShareLinkDto {
  enabled: boolean;
  role: ShareRole;
  /** Токен для адреса; `null` — восстановить нельзя (сменился секрет сервера), нужна новая ссылка. */
  token: string | null;
}

export interface ShareDto {
  people: SharePersonDto[];
  /** Только владельцу; `null` — ссылка ещё не создана или роли не хватает. */
  link: ShareLinkDto | null;
  canManage: boolean;
}

export interface ProjectDto {
  id: string;
  teamId: string;
  name: string;
}
export interface TeamDto {
  id: string;
  name: string;
  role: 'owner' | 'member';
  projects: ProjectDto[];
}
export interface TeamListDto {
  teams: TeamDto[];
}
export interface TeamMemberDto {
  id: string;
  email: string;
  role: 'owner' | 'member';
}
export interface TeamMembersDto {
  members: TeamMemberDto[];
}

function record(v: unknown): Record<string, unknown> | null {
  return typeof v === 'object' && v !== null && !Array.isArray(v) ? (v as Record<string, unknown>) : null;
}

export const isUuid = (v: unknown): v is string => typeof v === 'string' && UUID_RE.test(v);
export const isHash = (v: unknown): v is string => typeof v === 'string' && HASH_RE.test(v);

function parseLabel(v: unknown, max: number): string | null {
  if (typeof v !== 'string') return null;
  const s = v.trim();
  return s.length > 0 && Array.from(s).length <= max && !CONTROL_RE.test(s) ? s : null;
}

export const parseFileName = (v: unknown): string | null => parseLabel(v, MAX_FILE_NAME_CHARS);
export const parseTeamName = (v: unknown): string | null => parseLabel(v, MAX_TEAM_NAME_CHARS);

export function parseSection(v: unknown): FileSection | null {
  if (v === 'recent' || v === 'drafts' || v === 'shared' || v === 'trash') return v;
  if (typeof v === 'string' && v.startsWith('project:') && isUuid(v.slice(8))) return v as FileSection;
  return null;
}

function wholeInRange(s: string | null, max: number): number | null {
  if (s === null || !/^\d{1,16}$/.test(s)) return null;
  const n = Number(s);
  return n <= max ? n : null;
}

export interface ListQuery {
  section: FileSection;
  query: string;
  start: number;
  count: number;
}

export function parseListQuery(q: URLSearchParams): ListQuery | null {
  const section = parseSection(q.get('section'));
  const start = wholeInRange(q.get('start'), Number.MAX_SAFE_INTEGER);
  const count = wholeInRange(q.get('count'), MAX_FILES_WINDOW);
  const query = q.get('q') ?? '';
  if (!section || start === null || count === null) return null;
  if (Array.from(query).length > MAX_QUERY_CHARS || CONTROL_RE.test(query)) return null;
  return { section, query: query.trim(), start, count };
}

export function parseCreateFile(v: unknown): { name: string; projectId: string | null } | null {
  const r = record(v);
  const name = parseFileName(r?.name);
  if (!r || !name) return null;
  if (r.projectId === undefined || r.projectId === null) return { name, projectId: null };
  return isUuid(r.projectId) ? { name, projectId: r.projectId } : null;
}

export function parseRename(v: unknown): { name: string } | null {
  const name = parseFileName(record(v)?.name);
  return name ? { name } : null;
}

export function parseMove(v: unknown): { projectId: string | null } | null {
  const r = record(v);
  if (!r) return null;
  if (r.projectId === null) return { projectId: null };
  return isUuid(r.projectId) ? { projectId: r.projectId } : null;
}

const isShareRole = (v: unknown): v is ShareRole => v === 'editor' || v === 'viewer';

export function parseInvite(v: unknown): { email: string; role: ShareRole } | null {
  const r = record(v);
  const email = parseEmail(r?.email);
  return r && email && isShareRole(r.role) ? { email, role: r.role } : null;
}

export function parseRoleChange(v: unknown): { role: ShareRole } | null {
  const role = record(v)?.role;
  return isShareRole(role) ? { role } : null;
}

export function parseLinkUpdate(v: unknown): { enabled: boolean; role: ShareRole } | null {
  const r = record(v);
  return r && typeof r.enabled === 'boolean' && isShareRole(r.role) ? { enabled: r.enabled, role: r.role } : null;
}

export function parseRedeem(v: unknown): { token: string } | null {
  const token = record(v)?.token;
  return typeof token === 'string' && LINK_TOKEN_RE.test(token) ? { token } : null;
}

/** Токен из адреса `#/file/<id>?link=…`: неверный отбрасывается. */
export const parseLinkToken = (v: unknown): string | null => (typeof v === 'string' && LINK_TOKEN_RE.test(v) ? v : null);

export function parseTeamCreate(v: unknown): { name: string } | null {
  const name = parseTeamName(record(v)?.name);
  return name ? { name } : null;
}

export function parseTeamInvite(v: unknown): { email: string } | null {
  const email = parseEmail(record(v)?.email);
  return email ? { email } : null;
}

/** Заголовки записи файла: поколение, на котором она основана, номер этой записи и предыдущей. */
export interface WriteHeaders {
  base: number;
  write: string;
  /** Запись, ответ на которую потерялся: своя удачная запись — не `CONFLICT`. */
  previous: string | null;
  lost: number;
}
export const WRITE_ID_HEADER = 'x-ferrum-write';
export const PREVIOUS_WRITE_HEADER = 'x-ferrum-previous-write';
export const VERSION_HEADER = 'x-ferrum-version';
/** Кто работает с файлом, как считает вкладка. Cookie общая на браузер: после смены входа в другой вкладке запрос
 * ушёл бы от имени другого человека, поэтому сервер сверяет заголовок с сессией и отвечает 401. */
export const USER_HEADER = 'x-ferrum-user';
/** Сколько картинок из ссылок содержимого потеряно ещё до этой записи: байтов нет и у клиента, а правки
 * из-за этого не должны застревать (ядро показывает заглушку). Сверх этого числа запись отказывает. */
export const LOST_IMAGES_HEADER = 'x-ferrum-lost-images';

export function parseWriteHeaders(h: { get(name: string): string | null }): WriteHeaders | null {
  const m = /^"(\d{1,15})"$/.exec(h.get('if-match') ?? '');
  const write = h.get(WRITE_ID_HEADER);
  const previous = h.get(PREVIOUS_WRITE_HEADER);
  const lost = h.get(LOST_IMAGES_HEADER);
  if (!m || !isUuid(write) || (previous !== null && !isUuid(previous)) || (lost !== null && !/^\d{1,6}$/.test(lost))) return null;
  return { base: Number(m[1]), write, previous, lost: Number(lost ?? 0) };
}

const isNum = (v: unknown): v is number => typeof v === 'number' && Number.isFinite(v);
const isStr = (v: unknown): v is string => typeof v === 'string';
const isFileRole = (v: unknown): v is FileRole => v === 'owner' || v === 'editor' || v === 'viewer';
const isArrayOf = <T>(v: unknown, of: (x: unknown) => x is T): v is T[] => Array.isArray(v) && v.every(of);

export function isFileMeta(v: unknown): v is FileMetaDto {
  const m = record(v);
  return (
    !!m &&
    isStr(m.id) &&
    isStr(m.name) &&
    isNum(m.createdAt) &&
    isNum(m.editedAt) &&
    isNum(m.openedAt) &&
    isNum(m.size) &&
    isNum(m.thumbVersion) &&
    isNum(m.version) &&
    isFileRole(m.role) &&
    (m.trashedAt === undefined || isNum(m.trashedAt)) &&
    (m.projectId === undefined || isStr(m.projectId))
  );
}

export function isFileList(v: unknown): v is FileListDto {
  const m = record(v);
  return !!m && isNum(m.total) && isArrayOf(m.rows, isFileMeta);
}

export function isFileAccess(v: unknown): v is FileAccessDto {
  const m = record(v);
  return !!m && isFileRole(m.role) && isNum(m.version) && (m.canShare === undefined || typeof m.canShare === 'boolean');
}

function isSharePerson(v: unknown): v is SharePersonDto {
  const m = record(v);
  return !!m && isStr(m.id) && isStr(m.email) && (m.name === null || isStr(m.name)) && isFileRole(m.role);
}

export function isShareLink(v: unknown): v is ShareLinkDto {
  const m = record(v);
  return !!m && typeof m.enabled === 'boolean' && isShareRole(m.role) && (m.token === null || isStr(m.token));
}

export function isShare(v: unknown): v is ShareDto {
  const m = record(v);
  return !!m && isArrayOf(m.people, isSharePerson) && (m.link === null || isShareLink(m.link)) && typeof m.canManage === 'boolean';
}

function isProject(v: unknown): v is ProjectDto {
  const m = record(v);
  return !!m && isStr(m.id) && isStr(m.teamId) && isStr(m.name);
}

export function isProjectDto(v: unknown): v is ProjectDto {
  return isProject(v);
}

export function isTeam(v: unknown): v is TeamDto {
  const m = record(v);
  return !!m && isStr(m.id) && isStr(m.name) && (m.role === 'owner' || m.role === 'member') && isArrayOf(m.projects, isProject);
}

export function isTeamList(v: unknown): v is TeamListDto {
  const m = record(v);
  return !!m && isArrayOf(m.teams, isTeam);
}

function isTeamMember(v: unknown): v is TeamMemberDto {
  const m = record(v);
  return !!m && isStr(m.id) && isStr(m.email) && (m.role === 'owner' || m.role === 'member');
}

export function isTeamMembers(v: unknown): v is TeamMembersDto {
  const m = record(v);
  return !!m && isArrayOf(m.members, isTeamMember);
}
