// Файлы в браузере: выбрать файл («Открыть…», «Импорт…», изображение) —
// `showOpenFilePicker`, где он есть, иначе `<input type=file>`; скачать байты
// («Сохранить копию на диск…», экспорт PNG). Байты сразу уходят в worker. Подписи
// экрана файлов: «Изменён 5 минут назад», размер «1,2 МБ». Хранилище файлов —
// `file-store.ts`.

/** Скачивает байты файлом `name` (ссылка с `download`, адрес освобождается). */
export function downloadBytes(name: string, bytes: Uint8Array, type: string): void {
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

/** Наибольшая длина имени без расширения (в символах). */
const MAX_NAME = 120;

/** Имя скачиваемого файла из имени документа: без символов, запрещённых в именах
 * файлов (и управляющих), без точек и пробелов по краям; пустое — «Без названия». */
export function fileName(name: string, ext: string): string {
  // eslint-disable-next-line no-control-regex
  const clean = [...name.replace(/[\\/:*?"<>|\u0000-\u001f\u007f]/g, '_')]
    .slice(0, MAX_NAME)
    .join('')
    .replace(/^[\s.]+|[\s.]+$/g, '');
  return `${clean || 'Без названия'}${ext}`;
}

export interface FileKind {
  description: string;
  /** MIME → расширения (`{'image/png': ['.png']}`). */
  accept: Record<string, string[]>;
}

export const FERRUM_FILE: FileKind = { description: 'Файл Ferrum', accept: { 'application/x-ferrum': ['.ferrum'] } };
export const IMAGE_FILE: FileKind = {
  description: 'Изображение',
  accept: { 'image/png': ['.png'], 'image/jpeg': ['.jpg', '.jpeg'], 'image/webp': ['.webp'] },
};

type OpenPicker = (o: { types: FileKind[]; multiple?: boolean }) => Promise<{ getFile(): Promise<File> }[]>;

/** Выбор одного файла; `null` — пользователь отказался. */
export async function pickFile(kind: FileKind): Promise<File | null> {
  const picker = (window as unknown as { showOpenFilePicker?: OpenPicker }).showOpenFilePicker;
  if (picker) {
    try {
      const [handle] = await picker({ types: [kind] });
      return handle ? await handle.getFile() : null;
    } catch (e) {
      if (e instanceof DOMException && e.name === 'AbortError') return null;
      throw e;
    }
  }
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = Object.entries(kind.accept)
      .flatMap(([mime, exts]) => [mime, ...exts])
      .join(',');
    input.addEventListener('change', () => resolve(input.files?.[0] ?? null), { once: true });
    input.addEventListener('cancel', () => resolve(null), { once: true });
    input.click();
  });
}

/** Байты файла (передаются worker'у без копии). */
export async function fileBytes(file: Blob): Promise<Uint8Array> {
  return new Uint8Array(await file.arrayBuffer());
}

const RELATIVE = new Intl.RelativeTimeFormat('ru', { numeric: 'auto' });
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** Время относительно `now`: «только что» до минуты, затем минуты, часы, дни,
 * месяцы и годы назад («5 минут назад», «вчера»). Будущее (часы другой вкладки
 * спешат) — тоже «только что». */
export function relativeTime(at: number, now: number): string {
  const d = now - at;
  if (!Number.isFinite(d) || d < MINUTE) return 'только что';
  if (d < HOUR) return RELATIVE.format(-Math.floor(d / MINUTE), 'minute');
  if (d < DAY) return RELATIVE.format(-Math.floor(d / HOUR), 'hour');
  if (d < 30 * DAY) return RELATIVE.format(-Math.floor(d / DAY), 'day');
  if (d < 365 * DAY) return RELATIVE.format(-Math.floor(d / (30 * DAY)), 'month');
  return RELATIVE.format(-Math.floor(d / (365 * DAY)), 'year');
}

const SIZE = new Intl.NumberFormat('ru', { maximumFractionDigits: 1 });

/** Размер файла: «512 Б», «12 КБ», «1,2 МБ»; новый (0 байт) — «—». */
export function formatSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '—';
  const units = ['Б', 'КБ', 'МБ', 'ГБ'];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${SIZE.format(i === 0 ? v : Math.round(v * 10) / 10)} ${units[i]}`;
}
