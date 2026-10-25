// Связь ядра с системным буфером обмена и выбором файлов: события `copy`, `cut`,
// `paste` документа, события ядра `clipboard`, `clipboardPng`, `pasteRequested`,
// `requestImage`. Разбор форматов — в clipboard.ts, байты и текст с меткой
// разбирает worker: основной поток только переносит их.

import type { Editor } from './context';
import {
  encodeText,
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

/** Событие из поля ввода или при открытом модальном диалоге — это обычный текст
 * браузера, не документ на холсте. */
function textTarget(t: EventTarget | null): boolean {
  return modalOpen() || editableTarget(t);
}

export function installClipboard(ed: Editor): () => void {
  // Запасной путь записи: байты ждут события `copy`, которое вызывает execCommand.
  let pending: Uint8Array | null = null;
  const toast = (m: string) => ed.store.toast(m, 'error');

  const writeFallback = (bytes: Uint8Array) => {
    pending = bytes;
    let ok = false;
    try {
      ok = document.execCommand('copy');
    } catch {
      ok = false;
    }
    if (!ok || pending) {
      pending = null;
      toast('Не удалось записать в буфер обмена. Браузер не дал доступа');
    }
  };

  const write = (bytes: Uint8Array) => {
    if (!supportsCustomFormat()) {
      writeFallback(bytes);
      return;
    }
    navigator.clipboard
      .write([new ClipboardItem({ [FERRUM_MIME]: new Blob([bytes as BlobPart], { type: FERRUM_MIME }) })])
      .catch(() => writeFallback(bytes));
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
        toast('Слишком большой объём данных в буфере обмена: предел — 64 МБ');
        return;
      case 'image':
        break;
    }
    // Размер — до чтения: байты не копируются в память ради отказа.
    if (data.blob.size > MAX_CLIPBOARD_BYTES) {
      toast('Изображение слишком большое для вставки (больше 64 МБ)');
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
      e.clipboardData?.setData('text/plain', encodeText(pending));
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
    if (e.type === 'clipboard') write(e.bytes);
    else if (e.type === 'clipboardPng') writePng(e.bytes);
    else if (e.type === 'pasteRequested') void readAsync(false).then(deliver);
    else if (e.type === 'requestImage') {
      pickFile(IMAGE_FILE)
        .then(async (f) => {
          if (!f) return;
          if (f.size > MAX_CLIPBOARD_BYTES) {
            toast('Изображение слишком большое (больше 64 МБ)');
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
