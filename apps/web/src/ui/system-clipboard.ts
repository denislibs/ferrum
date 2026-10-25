// Связь ядра с системным буфером обмена и выбором файлов: события `copy`, `cut`,
// `paste` документа, события ядра `clipboard`, `clipboardPng`, `pasteRequested`,
// `requestImage`. Разбор форматов — в clipboard.ts, байты и текст с меткой
// разбирает worker: основной поток только переносит их.

import { tooLarge } from '../core/limits';
import type { ClipboardExtra } from '../core/messages';
import type { Editor } from './context';
import {
  eventIsEmpty,
  FERRUM_MIME,
  fromClipboardItems,
  fromDataTransfer,
  MAX_CLIPBOARD_BYTES,
  supportsCustomFormat,
  type PasteData,
} from './clipboard';
import { fileBytes, IMAGE_FILE, pickFile } from './files';
import { editableTarget, modalOpen } from './keys';

/** Событие из поля ввода, из диалога (и немодального — палитры), из меню или при
 * открытом модальном диалоге — это обычный текст браузера, не документ на холсте. */
function textTarget(t: EventTarget | null): boolean {
  if (modalOpen() || editableTarget(t)) return true;
  return t instanceof Element && !!t.closest('[role="dialog"], [role="alertdialog"], [role="menu"]');
}

/** Отказ записи; вырезанное уже удалено из документа — подсказка, как вернуть. */
function writeFailed(cut: boolean): string {
  return cut
    ? 'Не удалось записать в буфер обмена: браузер не дал доступа. Отмена вернёт вырезанные объекты'
    : 'Не удалось записать в буфер обмена: браузер не дал доступа. Повторите копирование';
}

export function installClipboard(ed: Editor): () => void {
  // Запасной путь записи: текст с меткой (его кодирует worker) ждёт события `copy`,
  // которое вызывает execCommand.
  let pending: string | null = null;
  // Подсказка о доступе на чтение — одна за сеанс, а не на каждую вставку.
  let hinted = false;
  const toast = (m: string) => ed.store.toast(m, 'error');

  const writeText = (text: string, cut: boolean) => {
    pending = text;
    let ok = false;
    try {
      // Без недавнего действия пользователя (например, ответ worker'а пришёл поздно) — false.
      ok = document.execCommand('copy');
    } catch {
      ok = false;
    }
    if (!ok || pending) {
      pending = null;
      toast(writeFailed(cut));
    }
  };

  /** Браузер отказал асинхронному API: текст с меткой — от worker'а, затем execCommand. */
  const writeViaWorker = (bytes: Uint8Array, cut: boolean) => {
    void ed.core.encodeClipboard(bytes).then((text) => {
      // `null` — отказ по размеру (ошибка уже пришла событием) или ядро остановлено.
      if (text !== null) writeText(text, cut);
    });
  };

  const write = (e: { bytes: Uint8Array } & ClipboardExtra) => {
    const cut = e.cut === true;
    if (!supportsCustomFormat()) {
      if (typeof e.text === 'string') writeText(e.text, cut);
      else writeViaWorker(e.bytes, cut);
      return;
    }
    navigator.clipboard
      .write([new ClipboardItem({ [FERRUM_MIME]: new Blob([e.bytes as BlobPart], { type: FERRUM_MIME }) })])
      .catch(() => writeViaWorker(e.bytes, cut));
  };

  const writePng = (bytes: Uint8Array) => {
    navigator.clipboard
      .write([new ClipboardItem({ 'image/png': new Blob([bytes as BlobPart], { type: 'image/png' }) })])
      .then(() => ed.store.toast('Скопировано как PNG'))
      .catch(() => toast('Не удалось скопировать картинку: браузер не дал доступа к буферу обмена'));
  };

  /** Асинхронное чтение (Chromium может спросить разрешение). `quiet` — вставка
   * сочетанием: данные события уже просмотрены, отказ в доступе — без уведомления. */
  const readAsync = async (quiet: boolean): Promise<PasteData> => {
    try {
      return await fromClipboardItems(await navigator.clipboard.read());
    } catch {
      if (!quiet) toast('Нет доступа к буферу обмена. Вставьте сочетанием клавиш');
      else if (!hinted) {
        // Свой формат читается только асинхронно: без доступа объекты Ferrum не вставить.
        hinted = true;
        ed.store.toast('Разрешите доступ к буферу обмена, чтобы вставлять объекты Ferrum');
      }
      return null;
    }
  };

  const deliver = async (data: PasteData) => {
    if (!data) return;
    switch (data.kind) {
      case 'ferrum':
        ed.send({ type: 'paste', bytes: data.bytes });
        return;
      case 'text':
        ed.core.pasteText(data.text);
        return;
      case 'tooBig':
        toast(tooLarge(MAX_CLIPBOARD_BYTES));
        return;
      case 'image':
        break;
    }
    // Размер — до чтения: байты не копируются в память ради отказа.
    if (data.blob.size > MAX_CLIPBOARD_BYTES) {
      toast(tooLarge(MAX_CLIPBOARD_BYTES, data.blob.size));
      return;
    }
    try {
      ed.send({ type: 'pasteImage', bytes: await fileBytes(data.blob) });
    } catch {
      toast('Не удалось прочитать изображение из буфера обмена');
    }
  };

  const onCopy = (command: 'edit.copy' | 'edit.cut') => (e: ClipboardEvent) => {
    if (pending && command === 'edit.copy') {
      e.clipboardData?.setData('text/plain', pending);
      pending = null;
      e.preventDefault();
      return;
    }
    if (textTarget(e.target)) return;
    e.preventDefault();
    ed.run(command);
  };
  const copy = onCopy('edit.copy');
  const cut = onCopy('edit.cut');
  const paste = (e: ClipboardEvent) => {
    if (textTarget(e.target)) return;
    e.preventDefault();
    const sync = fromDataTransfer(e.clipboardData);
    if (sync) void deliver(sync);
    // Свой формат асинхронного API в событии не виден: событие пустое — читаем его.
    else if (eventIsEmpty(e.clipboardData)) void readAsync(true).then(deliver);
  };
  document.addEventListener('copy', copy);
  document.addEventListener('cut', cut);
  document.addEventListener('paste', paste);
  const off = ed.core.on((e) => {
    if (e.type === 'clipboard') write(e);
    else if (e.type === 'clipboardPng') writePng(e.bytes);
    else if (e.type === 'pasteRequested') void readAsync(false).then(deliver);
    else if (e.type === 'requestImage') {
      pickFile(IMAGE_FILE)
        .then(async (f) => {
          if (!f) return;
          if (f.size > MAX_CLIPBOARD_BYTES) {
            toast(tooLarge(MAX_CLIPBOARD_BYTES, f.size));
            return;
          }
          ed.send({ type: 'placeImage', bytes: await fileBytes(f) });
        })
        .catch((err: unknown) =>
          toast(`Не удалось выбрать изображение: ${err instanceof Error ? err.message : String(err)}`),
        );
    }
  });
  return () => {
    document.removeEventListener('copy', copy);
    document.removeEventListener('cut', cut);
    document.removeEventListener('paste', paste);
    off();
  };
}
