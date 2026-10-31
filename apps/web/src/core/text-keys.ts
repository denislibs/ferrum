// Клавиши в поле правки текста (спека подсистемы 3, раздел 7): что отдать ядру
// командой `TextKey` (ход курсора, удаление, отмена, форматирование), что
// оставить браузеру (набор и IME идут событиями `beforeinput` и составления,
// Ctrl/Cmd+C, X, V — событиями буфера обмена) и что погасить: остальные команды
// раскладки в правке текста не действуют.

// Модуль без DOM: его берёт и worker ядра (предел команды ввода в `messages.ts`).

/** Платформа (как `Platform` в `core.ts`, без зависимости от DOM). */
type Platform = 'mac' | 'windows' | 'linux';

export type TextKeyAction = 'core' | 'browser' | 'block';

/** Ход курсора и удаление — ядро. */
const NAV = new Set([
  'ArrowLeft',
  'ArrowRight',
  'ArrowUp',
  'ArrowDown',
  'Home',
  'End',
  'PageUp',
  'PageDown',
  'Backspace',
  'Delete',
  'Escape',
  'Tab',
]);

/** Сочетания с главной клавишей, которые обрабатывает ядро в правке. */
const CORE_MOD = new Set(['KeyZ', 'KeyA', 'KeyB', 'KeyI', 'KeyU']);
/** С Alt и главной клавишей — выравнивание. */
const ALIGN = new Set(['KeyL', 'KeyT', 'KeyR', 'KeyJ']);
/** Буфер обмена — событиями браузера. */
const CLIPBOARD = new Set(['KeyC', 'KeyX', 'KeyV']);

/** Поля `KeyboardEvent`, нужные решению. */
export interface KeyLike {
  code: string;
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  isComposing: boolean;
  /** Устаревший код: 229 у клавиши, которую забрал IME (Safari шлёт её после
   * `compositionend` с `isComposing: false`). */
  keyCode?: number;
  /** AltGr (Windows сообщает его как Ctrl+Alt). */
  altGraph?: boolean;
}

/** Сочетания, которые оставляем браузеру и в правке (перезагрузка, инструменты). */
function browserKey(e: KeyLike): boolean {
  if (/^F\d+$/.test(e.key)) return true;
  const mod = e.metaKey || e.ctrlKey;
  return mod && (e.code === 'KeyR' || e.code === 'KeyW' || e.code === 'KeyT' || (e.shiftKey && e.code === 'KeyI'));
}

export function textKeyAction(e: KeyLike, platform: Platform): TextKeyAction {
  // Составление IME: клавиши — его (Enter подтверждает, Esc отменяет).
  if (e.isComposing || e.keyCode === 229 || e.key === 'Process' || e.key === 'Dead') return 'browser';
  // AltGr — это набор символа (ł, Ł, €), а не сочетание.
  if (e.altGraph) return 'browser';
  const primary = platform === 'mac' ? e.metaKey : e.ctrlKey;
  if (NAV.has(e.code)) return 'core';
  if (primary && e.altKey && !e.shiftKey && ALIGN.has(e.code)) return 'core';
  if (browserKey(e)) return 'browser';
  // Ctrl+Y — повтор на Windows и Linux.
  if (platform !== 'mac' && e.ctrlKey && !e.altKey && !e.shiftKey && e.code === 'KeyY') return 'core';
  if (primary && !e.altKey) {
    if (CORE_MOD.has(e.code)) return 'core';
    if (e.code === 'KeyX' && e.shiftKey) return 'core';
    if (CLIPBOARD.has(e.code) && !e.shiftKey) return 'browser';
    // Ctrl/Cmd+Shift+V — «вставить как обычный текст»: событие вставки, стиль у курсора.
    if (e.code === 'KeyV' && e.shiftKey) return 'browser';
    // Сохранить — обработчик редактора; остальные команды в правке не действуют.
    if (e.code === 'KeyS') return 'browser';
    return 'block';
  }
  return 'browser';
}

/** Вид ввода `beforeinput` → вид команды `TextInput`; `null` — не наш (или его
 * ведут события составления и буфера обмена). */
export function inputKind(
  inputType: string,
):
  | 'insert'
  | 'paragraph'
  | 'deleteBackward'
  | 'deleteForward'
  | 'deleteWordBackward'
  | 'deleteWordForward'
  | 'deleteLineBackward'
  | 'deleteLineForward'
  | null {
  switch (inputType) {
    case 'insertText':
    case 'insertReplacementText':
      return 'insert';
    case 'insertLineBreak':
    case 'insertParagraph':
      return 'paragraph';
    case 'deleteContentBackward':
      return 'deleteBackward';
    case 'deleteContentForward':
      return 'deleteForward';
    case 'deleteWordBackward':
      return 'deleteWordBackward';
    case 'deleteWordForward':
      return 'deleteWordForward';
    case 'deleteSoftLineBackward':
    case 'deleteHardLineBackward':
      return 'deleteLineBackward';
    case 'deleteSoftLineForward':
    case 'deleteHardLineForward':
      return 'deleteLineForward';
    default:
      return null;
  }
}

/** Предел текста одной команды ввода (спека 8): 64 КБ в UTF-8. */
export const MAX_TEXT_INPUT_BYTES = 64 * 1024;

/** Длина строки в UTF-8 без кодирования (суррогатная пара — 4 байта). */
export function utf8Length(s: string): number {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    if (c < 0x80) n += 1;
    else if (c < 0x800) n += 2;
    else if (c >= 0xd800 && c <= 0xdbff && i + 1 < s.length) {
      n += 4;
      i++;
    } else n += 3;
  }
  return n;
}
