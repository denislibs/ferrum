// Сохранение открытого файла в хранилище (этап 6, часть 1 — без журнала): полный
// снимок ядра (`snapshot`: байты `.ferrum` и превью первой страницы) уходит в
// worker ввода-вывода передачей, через основной поток без копий. Когда:
// - по Ctrl/Cmd+S (команда раскладки `file.save`, событие `saveRequested`);
// - через `AUTOSAVE_MS` после последней правки;
// - при уходе на экран файлов или к другому файлу (`save` ждёт записи);
// - при `pagehide` — попытка без гарантии: вкладка может закрыться раньше записи.
// Признак «изменён» снимает ядро (`markSaved`) только после удачной записи и только
// если с момента снимка правок не было. Ошибка записи — одно уведомление, пока
// запись снова не удастся; документ остаётся открытым и изменённым.

import type { CoreEvent } from '../wasm/ferrum_wasm';
import type { Editor } from './context';
import type { FileMeta, FileStore } from './file-store';
import { ownBuffer } from './file-store';

/** Отложенное сохранение: через столько миллисекунд после последней правки. */
export const AUTOSAVE_MS = 2000;

export interface Saver {
  /** Сохранить, если документ изменён; `true` — записано (или нечего писать). */
  save(): Promise<boolean>;
  dispose(): void;
}

export function createSaver(ed: Editor, files: FileStore, id: string, onSaved?: (m: FileMeta) => void): Saver {
  /** «Изменён» по событиям ядра сразу при получении, без ожидания кадра планировщика:
   * правка за миг до ухода со страницы не теряется. */
  let modified = false;
  let timer = 0;
  let running: Promise<boolean> | null = null;
  let again: Promise<boolean> | null = null;
  /** Последняя показанная ошибка записи: повтор не показываем, пока запись не удастся. */
  let reported: string | null = null;
  let waiting: ((e: CoreEvent & { type: 'snapshot' }) => void) | null = null;
  let failWaiting: (() => void) | null = null;

  const schedule = () => {
    clearTimeout(timer);
    timer = window.setTimeout(() => void save(), AUTOSAVE_MS);
  };

  const off = ed.core.on((e) => {
    switch (e.type) {
      case 'modified':
        modified = e.modified;
        if (modified) schedule();
        break;
      case 'document':
      case 'documentName':
      case 'pages':
        // Правка, пока документ уже изменён: отложить сохранение ещё на 2 с.
        if (modified) schedule();
        break;
      case 'saveRequested':
        // После всей пачки событий: правка и Ctrl+S в одном кадре ядра присылают
        // `modified` позже `saveRequested`.
        queueMicrotask(() => void save());
        break;
      case 'snapshot':
        waiting?.(e);
        break;
      case 'error':
        // Снимок не удался (кодирование) или ядро остановлено: ответа не будет.
        if (e.message.startsWith('Не удалось сохранить') || e.message.startsWith('Ядро остановлено')) failWaiting?.();
        break;
    }
  });

  const fail = (message: string) => {
    if (reported === message) return;
    reported = message;
    ed.store.toast(message, 'error');
  };

  async function write(): Promise<boolean> {
    clearTimeout(timer);
    if (!modified) return true;
    if (ed.store.state.stopped) return false;
    const snap = await new Promise<(CoreEvent & { type: 'snapshot' }) | null>((resolve) => {
      waiting = resolve;
      failWaiting = () => resolve(null);
      ed.send({ type: 'snapshot', thumbnail: true });
    });
    waiting = null;
    failWaiting = null;
    if (!snap) return false;
    try {
      const meta = await files.write(
        id,
        snap.name,
        ownBuffer(snap.bytes),
        snap.thumbnail ? ownBuffer(snap.thumbnail) : null,
      );
      ed.send({ type: 'markSaved', revision: snap.revision });
      reported = null;
      onSaved?.(meta);
      return true;
    } catch (err) {
      fail(`Не удалось сохранить файл: ${err instanceof Error ? err.message : String(err)}`);
      return false;
    }
  }

  /** Одна запись за раз; просьбы во время записи сливаются в одну следующую. */
  function save(): Promise<boolean> {
    if (!running) {
      const p = write().finally(() => {
        if (running === p) running = null;
      });
      running = p;
      return p;
    }
    again ??= running.then(() => {
      again = null;
      return save();
    });
    return again;
  }

  const onHide = () => void save();
  window.addEventListener('pagehide', onHide);

  return {
    save,
    dispose() {
      clearTimeout(timer);
      off();
      window.removeEventListener('pagehide', onHide);
      failWaiting?.();
    },
  };
}
