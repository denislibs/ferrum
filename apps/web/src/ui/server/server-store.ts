// `FileStore` на сервере (спека подсистемы 4, раздел 6.2): тот же интерфейс, что у IndexedDB, поэтому
// экран файлов и редактор не знают, откуда файлы. Настройки интерфейса и файлы прежней версии остаются
// в браузере: их ведёт `IdbFileStore`, а всё о серверных файлах — отдельный worker.

import { MAX_FILE_NAME_CHARS } from '@ferrum/protocol';
import type { FileMeta, IoResults, JournalMark, Section } from '../../core/file-list';
import type { AccessState } from '../../core/server-files';
import type { FromServer, ServerOp, ServerResults, ToServer } from '../../core/server-messages';
import { IdbFileStore, type FileStore } from '../file-store';
import type { SettingsBackend } from '../settings-store';

export class ServerFileStore implements FileStore, SettingsBackend {
  /** Файлы, настройки и шрифты в браузере. */
  readonly local = new IdbFileStore(false);
  readonly fullHistory = true;
  readonly remote = true;
  readonly importLabel = 'Открыть с диска…';
  private readonly worker: Worker;
  private seq = 0;
  private readonly pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();
  private readonly listeners = new Set<() => void>();
  private readonly watchers = new Map<string, Set<(s: AccessState) => void>>();
  private broken: string | null = null;
  private readonly onVisibility = () => this.post({ t: 'visible', on: document.visibilityState === 'visible' });

  constructor(user: string) {
    this.worker = new Worker(new URL('../../worker/server.worker.ts', import.meta.url), { type: 'module' });
    this.worker.onmessage = (e: MessageEvent<FromServer>) => this.onMessage(e.data);
    // Обновили образ при открытой вкладке: старого файла worker'а больше нет (B-208).
    this.worker.onerror = (e) => {
      e.preventDefault?.();
      this.fail('Не удалось запустить часть приложения — обновите страницу');
    };
    this.post({ t: 'init', user });
    document.addEventListener('visibilitychange', this.onVisibility);
    this.onVisibility();
  }

  private post(m: ToServer, transfer: Transferable[] = []): void {
    this.worker.postMessage(m, transfer);
  }

  private fail(message: string): void {
    this.broken = message;
    for (const p of this.pending.values()) p.reject(new Error(message));
    this.pending.clear();
  }

  private onMessage(m: FromServer): void {
    if (m.t === 'changed') {
      for (const l of this.listeners) l();
      return;
    }
    if (m.t === 'access') {
      for (const w of this.watchers.get(m.file) ?? []) w(m.state);
      return;
    }
    const p = this.pending.get(m.id);
    if (!p) return;
    this.pending.delete(m.id);
    if (m.ok) p.resolve(m.value);
    else p.reject(Object.assign(new Error(m.message), { code: m.code }));
  }

  private call<K extends ServerOp['op']>(op: Extract<ServerOp, { op: K }>, transfer: Transferable[] = []): Promise<ServerResults[K]> {
    if (this.broken) return Promise.reject(new Error(this.broken));
    const id = ++this.seq;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
      this.post({ t: 'req', id, ...op } as ToServer, transfer);
    });
  }

  list(section: Section, query: string, start: number, count: number): Promise<IoResults['list']> {
    return this.call({ op: 'list', section, query, start, count });
  }
  get(file: string) {
    return this.call({ op: 'get', file });
  }
  create(name: string, bytes?: Blob, project?: string) {
    return this.call({ op: 'create', name, bytes: bytes ?? null, project: project ?? null });
  }
  /** Перенос файла из браузера: байты и журнал передаются, основной поток их не копирует. */
  importLocal(name: string, bytes: ArrayBuffer | null, journal: ArrayBuffer[]) {
    return this.call({ op: 'create', name, bytes, project: null, journal }, [...(bytes ? [bytes] : []), ...journal]);
  }
  createFrom(name: string, bytes: ArrayBuffer) {
    return this.importLocal(name.slice(0, MAX_FILE_NAME_CHARS), bytes, []);
  }
  read(file: string) {
    return this.call({ op: 'read', file });
  }
  write(
    file: string,
    bytes: ArrayBuffer,
    thumb: ArrayBuffer | null | 'keep',
    base: number,
    journal: { session: number; upTo: number } | null,
  ) {
    return this.call(
      { op: 'write', file, bytes, thumb, base, journal },
      thumb instanceof ArrayBuffer ? [bytes, thumb] : [bytes],
    ) as Promise<FileMeta & { warning?: string }>;
  }
  async appendJournal(file: string, mark: JournalMark, bytes: ArrayBuffer) {
    await this.call({ op: 'appendJournal', file, session: mark.session, batch: mark.batch, bytes }, [bytes]);
  }
  blob(file: string) {
    return this.call({ op: 'blob', file });
  }
  rename(file: string, name: string) {
    return this.call({ op: 'rename', file, name });
  }
  touch(file: string) {
    return this.call({ op: 'touch', file });
  }
  async trash(file: string) {
    await this.call({ op: 'trash', file });
  }
  async restore(file: string) {
    await this.call({ op: 'restore', file });
  }
  async remove(file: string) {
    await this.call({ op: 'delete', file });
  }
  async emptyTrash() {
    await this.call({ op: 'emptyTrash' });
  }
  thumbnail(file: string) {
    return this.call({ op: 'thumb', file });
  }
  /** Список изменился мимо worker'а (перемещение файла с основного потока). */
  notifyChanged() {
    for (const l of this.listeners) l();
  }
  onChange(listener: () => void) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  /** Доступ к открытому файлу: понижение роли, снятие доступа, конец сессии. Этап 3 заменит опрос
   * сообщениями `role` и `access-revoked`, у получателя ничего не изменится. */
  watch(file: string, listener: (state: AccessState) => void) {
    const set = this.watchers.get(file) ?? new Set();
    this.watchers.set(file, set);
    set.add(listener);
    if (set.size === 1) this.post({ t: 'watch', file, on: true });
    return () => {
      set.delete(listener);
      if (set.size === 0) {
        this.watchers.delete(file);
        this.post({ t: 'watch', file, on: false });
      }
    };
  }
  /** Просьба о доступе и вход по ссылке — запросы к серверу без байтов. */
  redeemLink?: (file: string, token: string) => Promise<boolean>;
  requestAccess?: (file: string) => Promise<boolean>;
  fontBytes(blob: Blob) {
    return this.local.fontBytes(blob);
  }
  settings() {
    return this.local.settings();
  }
  setSettings(patch: Parameters<IdbFileStore['setSettings']>[0]) {
    return this.local.setSettings(patch);
  }
  onSettings(listener: (patch: Record<string, unknown>) => void) {
    return this.local.onSettings(listener);
  }
  dispose() {
    document.removeEventListener('visibilitychange', this.onVisibility);
    this.fail('хранилище закрыто');
    this.worker.terminate();
    this.local.dispose();
  }
}
