// Сообщения `sync.worker`: с основным потоком только состояние и роль, трафик идёт прямым портом с worker'ом ядра.

import type { CoreCommand, CoreSyncEvent, SyncStatus } from './sync-client';
import type { AccessState } from './server-files';

export type ToSync = { t: 'start'; file: string; user: string; schema: number; port: MessagePort } | { t: 'stop' };

export type FromSync = { t: 'status'; status: SyncStatus; detail?: string } | { t: 'access'; state: AccessState } | { t: 'reload' };

/** Порт между worker'ом ядра и `sync.worker`. `applied` и `reset` — ядро выполнило `applyRemote` и `syncReset`. */
export type CoreToSync = { t: 'event'; event: CoreSyncEvent } | { t: 'applied' } | { t: 'reset' };
export type SyncToCore = { t: 'cmd'; cmd: CoreCommand } | { t: 'flush'; on: boolean };
