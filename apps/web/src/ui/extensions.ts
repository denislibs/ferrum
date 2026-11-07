// Части интерфейса, которые есть только в рабочей сборке. Их ставит `main.tsx` в ветке, которую
// автономная сборка выбрасывает, поэтому код входа в неё не попадает, а экраны о нём не знают.

import type { Component, JSX } from 'solid-js';
import type { FileMeta, Section } from '../core/file-list';
import type { Participant } from '../core/sync-client';
import type { IconName } from './icons';
import type { MenuItem } from './menu';
import type { Route } from './router';

/** Полоса аватаров: участники, свой человек, за кем следуем, текущая страница; `follow(null)` — перестать. */
export interface PresenceBarProps {
  participants: () => Participant[];
  self: string;
  following: () => number | null;
  currentPage: () => string;
  follow: (participant: number | null) => void;
}

/** Строка панели разделов: раздел или подпись группы. */
export type NavEntry = { kind: 'section'; id: Section; title: string; icon: IconName } | { kind: 'label'; title: string };

export const extensions: {
  /** Вкладка «Аккаунт» настроек. */
  accountTab?: Component;
  /** Строка с человеком внизу панели экрана файлов. */
  accountRow?: Component;
  /** Адрес ведёт не приложение (ссылка из письма): `Root` его пропускает. В автономной сборке
   * таких адресов нет, любой адрес — его. */
  ownsHash?: (hash: string) => boolean;
  /** Адрес `?debug` не включает тестовый режим: страница рабочей сборки не раскрывает ядро по ссылке. */
  noDebugParam?: boolean;
  /** Раздел по части адреса, если у этой сборки есть другие разделы. */
  parseSection?: (path: string) => Section | null;
  /** Заголовок такого раздела. */
  sectionTitle?: (section: Section) => string | null;
  /** Строки панели разделов между «Черновиками» и «Корзиной». */
  filesNav?: () => NavEntry[];
  /** Под панелью разделов: команды, файлы прежней версии. */
  filesNavExtra?: Component<{ navigate: (r: Route) => void }>;
  /** Дополнительные пункты меню файла. */
  fileMenu?: (file: FileMeta) => MenuItem[];
  /** Новый файл из этого раздела попадает в проект. */
  newFileRoute?: (section: Section) => Route;
  /** Кнопка «Поделиться» открытого файла. */
  shareButton?: Component<{ file: FileMeta; role: () => FileMeta['role']; canShare: () => boolean; children?: JSX.Element }>;
  /** Аватары участников открытого файла (спека подсистемы 4, раздел 9). */
  presenceBar?: Component<PresenceBarProps>;
} = {};

/** Связь корня приложения с кодом входа: ставит `Root` (в обеих сборках, это просто два поля),
 * читает только код входа. Выход и смена входа не должны терять правки открытого файла. */
export const editorHooks: {
  /** Протокол ухода редактора: дописать файл, а если не вышло — спросить человека.
   * `false` — человек остался, уходить нельзя. Без редактора — `true`. */
  leave?: () => Promise<boolean>;
  /** Записать открытый файл без вопросов (вкладка сейчас перезагрузится). `false` — не записался. */
  flush?: () => Promise<boolean>;
  /** Остались правки, которых нет ни в файле, ни в журнале (новый файл без записи): перезагрузка их потеряет. */
  unsaved?: () => Promise<boolean>;
  /** Сессии больше нет, а файл не записался: редактор остаётся и предлагает копию на диск. */
  sessionLost?: () => void;
} = {};
