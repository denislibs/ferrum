// Перезапуск редактора (этап 6, спека 11): после сбоя ядра (паника в Rust, исключение
// в worker'е) и после потери GPU, которую не удалось пережить на месте, тот же файл
// открывается в новом редакторе с новым холстом (OffscreenCanvas второй раз не
// передать): документ — из файла и журнала. Не чаще `RESTART_LIMIT` раз за
// `RESTART_WINDOW_MS` — иначе повторяющийся сбой крутился бы вечно; дальше остаётся
// плашка остановки ядра.

import type { Backend } from '../wasm/ferrum_wasm';

export interface Restart {
  reason: 'crash' | 'gpu';
  /** Были правки ни в журнале, ни в файле — они потеряны. */
  lost: boolean;
  /** Бэкенд нового ядра (после потери GPU — запасной). */
  backend?: Backend;
  /** Причина — для консоли. */
  message: string;
}

export const RESTART_LIMIT = 2;
export const RESTART_WINDOW_MS = 60_000;

/** Можно ли перезапустить ещё раз; можно — момент записывается в `history`. */
export function allowRestart(history: number[], now: number): boolean {
  while (history.length && now - history[0] >= RESTART_WINDOW_MS) history.shift();
  if (history.length >= RESTART_LIMIT) return false;
  history.push(now);
  return true;
}

const LOST = 'Правки последних секунд не сохранились';

/** Уведомление в новом редакторе. */
export function restartText(r: Restart): string {
  if (r.reason === 'gpu') {
    const name = r.backend === 'webgl' ? 'WebGL2' : r.backend === 'webgpu' ? 'WebGPU' : 'на процессоре';
    return `Видеокарта недоступна — редактор перезапущен с рендером ${name}${r.lost ? `. ${LOST}` : ''}`;
  }
  return r.lost ? `Редактор перезапущен. ${LOST}` : 'Редактор перезапущен, изменения сохранены';
}
