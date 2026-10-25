// Системный буфер обмена (спека 7.7: работа интерфейса). Ctrl/Cmd+C, X, V идут
// событиями `copy`, `cut`, `paste` документа. Байты формата Ferrum готовит
// worker (событие `clipboard`), разбирает при вставке тоже worker — основной поток
// только переносит байты (и текст с меткой — не декодируя его).
//
// Запись: где асинхронный Clipboard API умеет свои форматы
// (`ClipboardItem.supports('web application/x-ferrum')`), байты кладутся под
// `web application/x-ferrum`. Иначе — запасной путь: `document.execCommand('copy')`
// вызывает ещё одно событие `copy`, и в его `clipboardData` байты уходят текстом
// base64 за меткой `FERRUM_TEXT_MARKER`.
//
// Вставка: сначала `clipboardData` события (метка в тексте, файлы картинок), затем
// асинхронное чтение своего формата. Картинки из других приложений (PNG, JPEG,
// WebP) ставятся узлом-изображением; не-PNG перекодирует worker.

import { FERRUM_TEXT_MARKER, isMarkedText, MAX_CLIPBOARD_BYTES, MAX_CLIPBOARD_TEXT } from '../core/clipboard-text';

export {
  decodeText,
  encodeText,
  FERRUM_TEXT_MARKER,
  fromBase64,
  MAX_CLIPBOARD_BYTES,
  MAX_CLIPBOARD_TEXT,
  toBase64,
} from '../core/clipboard-text';

/** Свой формат в асинхронном Clipboard API (префикс `web ` обязателен). */
export const FERRUM_MIME = 'web application/x-ferrum';
/** Картинки, которые вставляются узлом-изображением. */
export const IMAGE_TYPES = ['image/png', 'image/jpeg', 'image/webp'] as const;

/** Что вставить: байты Ferrum (свой формат), текст с меткой (декодирует worker),
 * картинка или отказ по размеру — до чтения и декодирования. */
export type PasteData =
  | { kind: 'ferrum'; bytes: Uint8Array }
  | { kind: 'text'; text: string }
  | { kind: 'image'; type: string; blob: Blob }
  | { kind: 'tooBig' }
  | null;

/** Выбор из `clipboardData` события вставки: свои байты (метка в тексте) важнее
 * картинки — Ferrum может положить и то и другое. */
export function fromDataTransfer(dt: Pick<DataTransfer, 'getData' | 'files'> | null): PasteData {
  if (!dt) return null;
  const text = dt.getData('text/plain') ?? '';
  if (isMarkedText(text)) return text.length > MAX_CLIPBOARD_TEXT ? { kind: 'tooBig' } : { kind: 'text', text };
  for (const f of Array.from(dt.files ?? [])) {
    if ((IMAGE_TYPES as readonly string[]).includes(f.type)) return { kind: 'image', type: f.type, blob: f };
  }
  return null;
}

/** В событии вставки ничего нет (свой формат асинхронного API в нём не виден):
 * тогда — асинхронное чтение. Чужой текст или файлы — не повод спрашивать доступ. */
export function eventIsEmpty(dt: Pick<DataTransfer, 'getData' | 'files'> | null): boolean {
  return !dt || ((dt.getData('text/plain') ?? '') === '' && (dt.files?.length ?? 0) === 0);
}

/** Выбор из элементов асинхронного Clipboard API. */
export async function fromClipboardItems(items: readonly Pick<ClipboardItem, 'types' | 'getType'>[]): Promise<PasteData> {
  for (const item of items) {
    if (item.types.includes(FERRUM_MIME)) {
      const blob = await item.getType(FERRUM_MIME);
      if (blob.size > MAX_CLIPBOARD_BYTES) return { kind: 'tooBig' };
      return { kind: 'ferrum', bytes: new Uint8Array(await blob.arrayBuffer()) };
    }
    if (item.types.includes('text/plain')) {
      const blob = await item.getType('text/plain');
      // Сначала только начало: чужой текст (сколь угодно большой) не читается целиком
      // и не заслоняет картинку того же элемента. Наш — предел до чтения.
      if (isMarkedText(await blob.slice(0, FERRUM_TEXT_MARKER.length).text())) {
        if (blob.size > MAX_CLIPBOARD_TEXT) return { kind: 'tooBig' };
        return { kind: 'text', text: await blob.text() };
      }
    }
    const image = item.types.find((t) => (IMAGE_TYPES as readonly string[]).includes(t));
    if (image) return { kind: 'image', type: image, blob: await item.getType(image) };
  }
  return null;
}

/** Умеет ли асинхронный API записывать свой формат. */
export function supportsCustomFormat(): boolean {
  const C = (globalThis as { ClipboardItem?: { supports?: (t: string) => boolean } }).ClipboardItem;
  try {
    return typeof C?.supports === 'function' && C.supports(FERRUM_MIME);
  } catch {
    return false;
  }
}
