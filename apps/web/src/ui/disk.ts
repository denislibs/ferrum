// File System Access (Chrome, Edge; спека 10): записи хранилища, связанные с файлом на
// диске. Выбор файла и запрос доступа — здесь, в окне: диалогам и разрешениям нужно
// действие пользователя. Чтение и запись по дескриптору — в worker'е ввода-вывода.
// Где API нет (Firefox, Safari), `fsaSupported()` — `false`, и пунктов меню нет.

import type { FileMeta } from '../core/file-list';
import { FERRUM_FILE } from './files';

export type Access = 'granted' | 'prompt' | 'denied';

export interface DiskFiles {
  /** «Открыть с диска…»: выбрать файл и завести связанную с ним запись; `null` — отказ. */
  open(): Promise<FileMeta | null>;
  /** «Сохранить на диск…»: выбрать файл; дальше запись идёт в него. `null` — отказ. */
  saveAs(id: string, suggestedName: string): Promise<FileMeta | null>;
  /** Доступ к файлу записи (не на диске — `granted`). */
  access(id: string): Promise<Access>;
  /** Запросить доступ — только из обработчика действия пользователя. */
  requestAccess(id: string): Promise<boolean>;
  /** Копия файла в выбранное место родным диалогом (десктоп); нет — копия скачиванием. */
  saveCopy?(id: string, suggestedName: string): Promise<boolean>;
}

/** Запросы к worker'у ввода-вывода, которые нужны диску (их даёт `IdbFileStore`). */
export interface DiskIo {
  createFromDisk(handle: FileSystemFileHandle): Promise<FileMeta>;
  attachDisk(id: string, handle: FileSystemFileHandle): Promise<FileMeta>;
  handle(id: string): Promise<FileSystemFileHandle | null>;
}

type Permission = { mode: 'readwrite' };
type Handle = FileSystemFileHandle & {
  queryPermission?(o: Permission): Promise<PermissionState>;
  requestPermission?(o: Permission): Promise<PermissionState>;
};
type Pickers = {
  showOpenFilePicker?(o: { types: unknown[] }): Promise<FileSystemFileHandle[]>;
  showSaveFilePicker?(o: { suggestedName: string; types: unknown[] }): Promise<FileSystemFileHandle>;
};

export function fsaSupported(): boolean {
  const w = window as unknown as Pickers;
  return (
    typeof w.showOpenFilePicker === 'function' &&
    typeof w.showSaveFilePicker === 'function' &&
    typeof FileSystemFileHandle === 'function' &&
    'createWritable' in FileSystemFileHandle.prototype
  );
}

const aborted = (e: unknown) => e instanceof DOMException && e.name === 'AbortError';

export function createDiskFiles(io: DiskIo): DiskFiles {
  const w = window as unknown as Required<Pickers>;
  const mode: Permission = { mode: 'readwrite' };
  /** Дескрипторы, полученные при проверке доступа: запрос по щелчку идёт сразу, без
   * обращения к worker'у (иначе действие пользователя могло бы истечь). */
  const known = new Map<string, Handle>();
  const handleOf = async (id: string) => known.get(id) ?? ((await io.handle(id)) as Handle | null);
  return {
    async open() {
      let handle: FileSystemFileHandle | undefined;
      try {
        [handle] = await w.showOpenFilePicker({ types: [FERRUM_FILE] });
      } catch (e) {
        if (aborted(e)) return null;
        throw e;
      }
      return handle ? io.createFromDisk(handle) : null;
    },
    async saveAs(id, suggestedName) {
      let handle: FileSystemFileHandle;
      try {
        handle = await w.showSaveFilePicker({ suggestedName, types: [FERRUM_FILE] });
      } catch (e) {
        if (aborted(e)) return null;
        throw e;
      }
      return io.attachDisk(id, handle);
    },
    async access(id) {
      const h = (await io.handle(id)) as Handle | null;
      if (h) known.set(id, h);
      if (!h?.queryPermission) return 'granted';
      return h.queryPermission(mode);
    },
    async requestAccess(id) {
      const h = await handleOf(id);
      if (!h?.requestPermission) return true;
      return (await h.requestPermission(mode)) === 'granted';
    },
  };
}
