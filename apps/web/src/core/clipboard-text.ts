// Байты Ferrum текстом в буфере обмена (запасной путь, когда асинхронный Clipboard
// API не умеет свои форматы): base64 за меткой. Кодирует основной поток при
// копировании, декодирует только worker — текст из буфера не доверенный, а
// декодирование десятков мегабайт в основном потоке было бы длинной задачей.

/** Метка байтов Ferrum в `text/plain` (запасной путь). */
export const FERRUM_TEXT_MARKER = 'ferrum-clipboard:v1:';
/** Предел байтов из буфера — как у команды `paste` (64 МБ). */
export const MAX_CLIPBOARD_BYTES = 64 * 1024 * 1024;
/** Предел текста с меткой: base64 длиннее байтов на треть (и немного на пробелы по краям). */
export const MAX_CLIPBOARD_TEXT = FERRUM_TEXT_MARKER.length + Math.ceil((MAX_CLIPBOARD_BYTES * 4) / 3) + 64;

/** base64 без переполнения стека на больших массивах. */
export function toBase64(bytes: Uint8Array): string {
  let s = '';
  const CHUNK = 0x8000;
  for (let i = 0; i < bytes.length; i += CHUNK) {
    s += String.fromCharCode(...bytes.subarray(i, i + CHUNK));
  }
  return btoa(s);
}

export function fromBase64(text: string): Uint8Array | null {
  try {
    const bin = atob(text);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out;
  } catch {
    return null;
  }
}

/** Байты Ferrum → текст для запасного пути. */
export function encodeText(bytes: Uint8Array): string {
  return FERRUM_TEXT_MARKER + toBase64(bytes);
}

/** Текст с меткой (не декодируя). */
export function isMarkedText(text: string): boolean {
  return text.startsWith(FERRUM_TEXT_MARKER);
}

/** Текст из буфера → байты Ferrum; `null` — не наш текст или он испорчен. */
export function decodeText(text: string): Uint8Array | null {
  if (!isMarkedText(text)) return null;
  // Предел — до декодирования: дальше него не декодируем.
  if (text.length > MAX_CLIPBOARD_TEXT) return null;
  const body = text.slice(FERRUM_TEXT_MARKER.length).trim();
  if (body.length > Math.ceil((MAX_CLIPBOARD_BYTES * 4) / 3) + 4) return null;
  return fromBase64(body);
}
