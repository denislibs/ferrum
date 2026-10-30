// Системные шрифты (спека подсистемы 3, раздел 4). Chrome и Edge —
// `queryLocalFonts()`: список (метаданные `FontData` без байтов) строится здесь,
// в основном потоке, по явному действию пользователя или при запуске, если
// разрешение уже выдано; байты файла (`FontData.blob()`) читает worker
// ввода-вывода и отдаёт передачей. Десктоп — сканирование каталогов шрифтов в
// Rust, байты сырым ответом IPC. Firefox и Safari — только вшитые.

import type { FontFamilyDto } from '../wasm/ferrum_wasm';
import { desktopFamilies, localFamilies, type LocalFont } from '../core/fonts';
import { MAX_FONT_BYTES, tooLarge } from '../core/limits';
import type { Editor } from './context';
import { isTauri, systemFontBytes, systemFontList } from './tauri';

/** `FontData` Chrome (тип ещё не во всех lib.dom). */
interface FontData extends LocalFont {
  blob(): Promise<Blob>;
}

type QueryLocalFonts = () => Promise<FontData[]>;

export interface SystemFonts {
  /** Есть ли системные шрифты вообще (иначе пункт «Показать системные шрифты» скрыт). */
  readonly supported: boolean;
  /** Разрешение уже есть: список строится без вопроса. */
  granted(): Promise<boolean>;
  /** Список семейств (в Chrome — по действию пользователя: просит разрешение). */
  list(): Promise<FontFamilyDto[]>;
  /** Байты файла начертания (postscript-имя в Chrome, номер файла в десктопе). */
  read(file: string): Promise<ArrayBuffer>;
}

/** Chrome и Edge: `queryLocalFonts`; байты — через worker ввода-вывода. */
export function chromeFonts(readBlob: (blob: Blob) => Promise<ArrayBuffer>): SystemFonts {
  const query = (window as unknown as { queryLocalFonts?: QueryLocalFonts }).queryLocalFonts;
  const byName = new Map<string, FontData>();
  return {
    supported: typeof query === 'function',
    async granted() {
      try {
        const p = await navigator.permissions.query({ name: 'local-fonts' as PermissionName });
        return p.state === 'granted';
      } catch {
        return false;
      }
    },
    async list() {
      if (!query) return [];
      const fonts = await query.call(window);
      byName.clear();
      for (const f of fonts) byName.set(f.postscriptName, f);
      return localFamilies(fonts);
    },
    async read(file) {
      const f = byName.get(file);
      if (!f) throw new Error('шрифта нет в списке');
      const blob = await f.blob();
      if (blob.size > MAX_FONT_BYTES) throw new Error(tooLarge(MAX_FONT_BYTES, blob.size));
      return readBlob(blob);
    },
  };
}

/** Десктоп: каталоги шрифтов сканирует Rust; разрешения не нужно. */
export function tauriFonts(): SystemFonts {
  return {
    supported: true,
    granted: async () => true,
    async list() {
      const { families, dropped } = desktopFamilies(await systemFontList());
      if (dropped) console.warn(`ferrum: системных шрифтов пропущено (неверные или сверх предела): ${dropped}`);
      return families;
    },
    read: (file) => systemFontBytes(file),
  };
}

export const NO_SYSTEM_FONTS: SystemFonts = {
  supported: false,
  granted: async () => false,
  list: async () => [],
  read: async () => {
    throw new Error('системных шрифтов нет');
  },
};

/** Источник системных шрифтов этой платформы. */
export function systemFonts(readBlob: (blob: Blob) => Promise<ArrayBuffer>): SystemFonts {
  if (isTauri()) return tauriFonts();
  const chrome = chromeFonts(readBlob);
  return chrome.supported ? chrome : NO_SYSTEM_FONTS;
}

/** Подключает системные шрифты к ядру: список при запуске (если разрешение уже
 * есть), ответы на `fontRequest` системных семейств. Возвращает «показать
 * системные шрифты» (по щелчку пользователя) и отписку. */
export function installSystemFonts(
  ed: Pick<Editor, 'core' | 'store' | 'send'>,
  src: SystemFonts,
): { show: () => Promise<void>; dispose: () => void } {
  let shown = false;
  const show = async () => {
    try {
      const families = await src.list();
      shown = true;
      ed.store.setState('systemFonts', 'shown', true);
      ed.send({ type: 'fontsAvailable', source: 'system', families });
    } catch (e) {
      ed.store.toast(`Системные шрифты недоступны: ${e instanceof Error ? e.message : String(e)}`, 'error');
    }
  };
  ed.store.setState('systemFonts', { supported: src.supported, shown: false });
  if (src.supported) {
    void src.granted().then(async (ok) => {
      if (ok && !shown) await show();
    });
  }
  const off = ed.core.on((e) => {
    if (e.type !== 'fontRequest' || e.source !== 'system') return;
    for (const file of e.files) {
      src
        .read(file)
        .then((buf) => ed.send({ type: 'loadFont', family: e.family, file, bytes: new Uint8Array(buf) }))
        .catch((err: unknown) => {
          console.warn(`ferrum: системный шрифт ${e.family} не прочитан`, err);
          ed.send({ type: 'loadFont', family: e.family, file });
        });
    }
  });
  return { show, dispose: off };
}
