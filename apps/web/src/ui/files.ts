// Файлы в браузере (этап 5 — минимум): выбрать файл («Открыть…», изображение) —
// `showOpenFilePicker`, где он есть, иначе `<input type=file>`; скачать байты
// («Сохранить как…», экспорт PNG). Байты сразу уходят в worker. Недавние файлы,
// IndexedDB, автосохранение и экран файлов — этап 6.

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
