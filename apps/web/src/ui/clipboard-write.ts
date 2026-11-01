// Запись готовых байтов в системный буфер обмена: «Копировать как PNG» и
// «Копировать как SVG». Асинхронного API может не быть (старый движок, небезопасный
// контекст: нет `ClipboardItem` или `navigator.clipboard`), и он может отказать —
// тогда SVG уходит обычным текстом через `execCommand`, а пользователь всегда видит
// итог: «Скопировано…» или ошибку по-русски. Исключений наружу нет.

/** Куда сообщать итог и как записать обычный текст запасным путём. */
export interface ClipboardSink {
  toast(message: string, kind?: 'error'): void;
  /** Обычный текст через `execCommand('copy')`; `false` — браузер отказал. */
  writeText(text: string): boolean;
}

export const PNG_COPIED = 'Скопировано как PNG';
export const SVG_COPIED = 'Скопировано как SVG';
export const PNG_FAILED = 'Не удалось скопировать картинку: браузер не дал доступа к буферу обмена';
export const SVG_FAILED = 'Не удалось скопировать SVG: браузер не дал доступа к буферу обмена';

type Item = new (items: Record<string, Blob>) => ClipboardItem;

function clipboardItem(): (Item & { supports?: (type: string) => boolean }) | null {
  const C = (globalThis as { ClipboardItem?: unknown }).ClipboardItem;
  return typeof C === 'function' ? (C as Item) : null;
}

/** Асинхронная запись набора форматов, если она есть; иначе `null`. */
export function asyncWriter(): ((items: Record<string, Blob>) => Promise<void>) | null {
  try {
    const C = clipboardItem();
    const clip = (globalThis as { navigator?: { clipboard?: Clipboard } }).navigator?.clipboard;
    if (!C || typeof clip?.write !== 'function') return null;
    return (items) => clip.write([new C(items)]);
  } catch {
    return null;
  }
}

/** Умеет ли асинхронный буфер записывать `image/svg+xml` (Chromium 124+). */
export function svgSupported(): boolean {
  try {
    const C = clipboardItem();
    return typeof C?.supports === 'function' && C.supports('image/svg+xml');
  } catch {
    return false;
  }
}

/** «Копировать как PNG»: только асинхронным API (обычного текста у картинки нет). */
export async function copyPng(bytes: Uint8Array, sink: ClipboardSink): Promise<void> {
  const write = asyncWriter();
  if (!write) {
    sink.toast(PNG_FAILED, 'error');
    return;
  }
  try {
    await write({ 'image/png': new Blob([bytes as BlobPart], { type: 'image/png' }) });
    sink.toast(PNG_COPIED);
  } catch {
    sink.toast(PNG_FAILED, 'error');
  }
}

/** «Копировать как SVG»: байты UTF-8 от worker'а идут в `text/plain` и, где браузер
 * умеет, в `image/svg+xml` без перекодирования; в строку они раскодируются один
 * раз — только для запасного пути `execCommand`. */
export async function copySvg(bytes: Uint8Array, sink: ClipboardSink): Promise<void> {
  const fallback = () => {
    let ok = false;
    try {
      ok = sink.writeText(new TextDecoder().decode(bytes));
    } catch {
      ok = false;
    }
    if (ok) sink.toast(SVG_COPIED);
    else sink.toast(SVG_FAILED, 'error');
  };
  const write = asyncWriter();
  if (!write) {
    fallback();
    return;
  }
  try {
    const items: Record<string, Blob> = { 'text/plain': new Blob([bytes as BlobPart], { type: 'text/plain' }) };
    if (svgSupported()) items['image/svg+xml'] = new Blob([bytes as BlobPart], { type: 'image/svg+xml' });
    await write(items);
    sink.toast(SVG_COPIED);
  } catch {
    fallback();
  }
}
