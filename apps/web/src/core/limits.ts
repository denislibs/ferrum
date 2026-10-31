// Пределы данных интерфейса — единственная запись на стороне TypeScript. Те же
// числа держит ядро на Rust (`MAX_PASTE_BYTES`, `clipboard::MAX_JSON`,
// `MAX_IMAGE_SIDE`, `MAX_KEYMAP_JSON`, `MAX_NAME_CHARS`); `public/theme-init.js`
// не может импортировать модуль и повторяет предел настроек числом. Все копии
// сверяет `unit/limits.test.ts`. Тексты для пользователя строятся отсюда же.

const MIB = 1024 * 1024;

/** Байты буфера обмена и картинки (из буфера и из файла) — как у ядра. */
export const MAX_DATA_BYTES = 64 * MIB;
/** Открываемый файл `.ferrum`: память wasm ограничена 4 ГБ, а копия байтов в ней — вторая. */
export const MAX_OPEN_BYTES = 512 * MIB;
/** Наибольшая сторона картинки в пикселях (по заголовку и после декодирования). */
export const MAX_IMAGE_SIDE = 8192;
/** Файл раскладки клавиш (JSON отличий). */
export const MAX_KEYMAP_BYTES = 64 * 1024;
/** Запись настроек в хранилище: раскладка и немного полей. */
export const MAX_SETTINGS_BYTES = 80 * 1024;
/** Наибольшая длина имени слоя, страницы и файла в символах. */
export const MAX_NAME_CHARS = 256;
/** Пачек журнала изменений при открытии файла — как `MAX_JOURNAL_BATCHES` ядра. */
export const MAX_JOURNAL_ENTRIES = 100_000;
/** Файл шрифта — как `ferrum_fontmeta::MAX_FONT_FILE` ядра (спека подсистемы 3, раздел 4). */
export const MAX_FONT_BYTES = 64 * MIB;
/** Имя семейства и начертания в байтах UTF-8 — как `MAX_FAMILY_NAME` ядра. */
export const MAX_FAMILY_NAME = 256;
/** Семейств в списке шрифтов и начертаний у семейства — как `MAX_FONT_FAMILIES`, `MAX_FONT_FACES`. */
export const MAX_FONT_FAMILIES = 4096;
export const MAX_FONT_FACES = 1000;

/** Правка текста: размер шрифта 1…10 000, интерлиньяж и трекинг в пикселях, интервал и отступ
 * абзаца — как `MAX_FONT_SIZE` ядра. Ядро значения вне пределов отклоняет ошибкой, поэтому
 * секция «Текст» зажимает всё, что шлёт, этими же числами. Проценты ядро держит литералами
 * в `core/text.rs` (интерлиньяж 0…10 000, трекинг ±1000). */
export const MIN_FONT_SIZE = 1;
export const MAX_FONT_SIZE = 10_000;
export const MAX_LINE_HEIGHT_PERCENT = 10_000;
export const MAX_LETTER_SPACING_PERCENT = 1000;

/** Предел острого угла обводки — как `MITER_LIMIT_RANGE` ядра (`ferrum-doc`): значение вне
 * пределов ядро отклоняет ошибкой, поэтому секция «Обводка» зажимает поле этими числами. */
export const MIN_MITER_LIMIT = 1;
export const MAX_MITER_LIMIT = 100;

/** Мегабайты с округлением вверх: «предел 64 МБ, получено 64 МБ» сбивало бы с толку. */
export const mb = (bytes: number): number => Math.ceil(bytes / MIB);
/** Килобайты с округлением вверх. */
export const kb = (bytes: number): number => Math.ceil(bytes / 1024);

/** Один текст для всех отказов по размеру: сколько пришло (если известно) и предел. */
export function tooLarge(limit: number, actual?: number): string {
  return actual === undefined
    ? `Слишком большой объём данных: предел — ${mb(limit)} МБ`
    : `Слишком большой объём данных: ${mb(actual)} МБ, предел — ${mb(limit)} МБ`;
}
