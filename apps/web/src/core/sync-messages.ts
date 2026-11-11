// Сообщения `sync.worker`: с основным потоком только состояние и роль, трафик идёт прямым портом с worker'ом ядра.

import type { CoreCommand, CoreSyncEvent, Participant, SyncStatus } from './sync-client';
import type { AccessState } from './server-files';
import type { CommentRequest, CommentsToMain } from './comments-store';

export type ToSync =
  | { t: 'start'; file: string; user: string; schema: number; port: MessagePort }
  | { t: 'stop' }
  /** Запрос комментариев (спека подсистемы 4, раздел 10): ответ — `{ t: 'comments', m: { t: 'result', id } }`. */
  | { t: 'comments'; id: number; req: CommentRequest };

export type FromSync =
  | { t: 'status'; status: SyncStatus; detail?: string }
  | { t: 'access'; state: AccessState }
  | { t: 'reload' }
  /** Участники файла для шапки (спека подсистемы 4, раздел 9): не чаще четырёх раз в секунду. */
  | { t: 'participants'; list: Participant[] }
  /** Комментарии: изменившиеся обсуждения, открытое обсуждение, ответы на запросы. */
  | { t: 'comments'; m: CommentsToMain };

/** Порт между worker'ом ядра и `sync.worker`. `applied` и `reset` — ядро выполнило `applyRemote` и `syncReset`. */
export type CoreToSync = { t: 'event'; event: CoreSyncEvent } | { t: 'applied' } | { t: 'reset' };
export type SyncToCore = { t: 'cmd'; cmd: CoreCommand } | { t: 'flush'; on: boolean };
