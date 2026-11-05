// Сообщения между основным потоком и worker'ом файлов сервера.

import type { FileMeta, IoResults } from './file-list';
import type { AccessState } from './server-files';

export type ServerOp =
  | { op: 'list'; section: string; query: string; start: number; count: number }
  | { op: 'get'; file: string }
  /** `bytes`: `File` с диска уходит ссылкой, читает worker; `journal` — правки переносимого файла. */
  | { op: 'create'; name: string; bytes: Blob | ArrayBuffer | null; project: string | null; journal?: ArrayBuffer[] }
  | { op: 'read'; file: string }
  | {
      op: 'write';
      file: string;
      bytes: ArrayBuffer;
      thumb: ArrayBuffer | null | 'keep';
      base: number;
      journal: { session: number; upTo: number } | null;
    }
  | { op: 'appendJournal'; file: string; session: number; batch: number; bytes: ArrayBuffer }
  | { op: 'blob'; file: string }
  | { op: 'rename'; file: string; name: string }
  | { op: 'touch'; file: string }
  | { op: 'trash'; file: string }
  | { op: 'restore'; file: string }
  | { op: 'delete'; file: string }
  | { op: 'emptyTrash' }
  | { op: 'thumb'; file: string };

export type ServerRequest = { id: number } & ServerOp;

export interface ServerResults {
  list: IoResults['list'];
  get: FileMeta | null;
  create: FileMeta;
  read: IoResults['read'];
  write: FileMeta;
  appendJournal: null;
  blob: Blob | null;
  rename: FileMeta;
  touch: FileMeta;
  trash: null;
  restore: null;
  delete: null;
  emptyTrash: null;
  thumb: Blob | null;
}

export type ToServer =
  { t: 'init'; user: string } | { t: 'visible'; on: boolean } | { t: 'watch'; file: string; on: boolean } | ({ t: 'req' } & ServerRequest);

export type FromServer =
  | { t: 'res'; id: number; ok: true; value: unknown }
  | { t: 'res'; id: number; ok: false; message: string; code?: string }
  | { t: 'changed' }
  | { t: 'access'; file: string; state: AccessState };
