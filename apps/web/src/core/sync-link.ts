// Порт синхронизации в worker'е ядра (спека подсистемы 4, разделы 7.1 и 8): команды `sync.worker` идут ядру, события ядра — обратно,
// пачки своих правок ядро собирает по таймеру не реже 30 раз в секунду, но только пока `sync.worker` разрешает и ядру есть что
// собрать. У порта есть конец: уход с файла, новый порт и сбой ядра снимают таймер и закрывают порт.

import type { CoreCommand } from './sync-client';
import type { CoreToSync, SyncToCore } from './sync-messages';

/** Пачки правок — не реже 30 раз в секунду, пока есть куда и что (спека подсистемы 4, раздел 8). */
export const SYNC_FLUSH_MS = 33;

export interface PortLike {
  onmessage: ((e: MessageEvent<SyncToCore>) => void) | null;
  postMessage(m: CoreToSync, transfer?: Transferable[]): void;
  close(): void;
}

export interface SyncLinkEnv {
  /** Выполнить команду ядра и разослать его события. */
  run(cmd: CoreCommand | { type: 'syncFlush' }): void;
  setInterval(fn: () => void, ms: number): unknown;
  clearInterval(t: unknown): void;
}

export class SyncLink {
  private port: PortLike | null = null;
  private timer: unknown = null;
  /** С прошлой сборки ядро что-то делало (правка, ответ сервера, загрузка страницы): могут быть новые правки и подписки. В простое
   * событий нет, и ядро не будится. */
  private wanted = true;

  constructor(private readonly env: SyncLinkEnv) {}

  get connected(): boolean {
    return this.port !== null;
  }

  /** Новый порт: прежний закрывается вместе со своим таймером, его запоздалые кадры уже не доходят. */
  connect(port: PortLike): void {
    this.stop();
    this.port = port;
    port.onmessage = (e) => {
      if (this.port === port) this.onMessage(e.data);
    };
  }

  /** Уход с файла или сбой ядра. */
  stop(): void {
    this.setFlush(false);
    const port = this.port;
    this.port = null;
    if (port) {
      port.onmessage = null;
      port.close();
    }
  }

  /** Ядро выдало события: после них могут быть несобранные правки. */
  touch(): void {
    this.wanted = true;
  }

  post(m: CoreToSync, transfer: Transferable[] = []): void {
    this.port?.postMessage(m, transfer);
  }

  private onMessage(m: SyncToCore): void {
    if (m.t === 'flush') {
      this.setFlush(m.on);
      return;
    }
    // Присутствие правок не рождает: сборка пачек на него не просыпается (оно приходит 30 раз в секунду).
    if (m.cmd.type !== 'presence' && m.cmd.type !== 'presenceReset') this.wanted = true;
    this.env.run(m.cmd);
    // Порядок порта: всё, что ядро пришлёт после этих ответов, — после команды.
    if (m.cmd.type === 'applyRemote') this.post({ t: 'applied' });
    else if (m.cmd.type === 'syncReset') this.post({ t: 'reset' });
  }

  private setFlush(on: boolean): void {
    if (this.timer !== null) this.env.clearInterval(this.timer);
    this.timer = null;
    if (!on || !this.port) return;
    this.wanted = true;
    this.flush();
    this.timer = this.env.setInterval(() => this.flush(), SYNC_FLUSH_MS);
  }

  private flush(): void {
    if (!this.wanted) return;
    this.wanted = false;
    this.env.run({ type: 'syncFlush' });
  }
}
