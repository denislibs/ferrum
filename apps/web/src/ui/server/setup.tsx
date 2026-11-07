// Всё серверное в интерфейсе: ставит `main.tsx` в ветке рабочей сборки. Общий код об этом не знает,
// поэтому в автономную сборку ни запросы к серверу, ни его разделы не попадают.

import { isUuid } from '@ferrum/protocol';
import { createSignal } from 'solid-js';
import { currentUser } from '../auth/session';
import { extensions } from '../extensions';
import { LocalFilesEntry } from './local-files';
import { redeemLink, requestFileAccess } from './server-api';
import { ServerFileStore } from './server-store';
import { CommentsUi } from './comments';
import { PresenceBar } from './presence-bar';
import { ShareButton } from './share-dialog';
import { fileMenuItems, MoveDialog, refreshTeams, setOnMoved, teamNavEntries, TeamsEntry, teamSectionTitle } from './teams';

const [store, setStore] = createSignal<ServerFileStore | null>(null);

export function createServerStore(): ServerFileStore {
  // `Root` монтируется только при живой сессии (`AuthGate`).
  const s = new ServerFileStore(currentUser()!.id);
  s.redeemLink = async (id, token) => (await redeemLink(id, token)).ok;
  s.requestAccess = async (id) => (await requestFileAccess(id)).ok;
  setStore(s);
  // Команды и проекты могли измениться у других: вместе со списком файлов.
  s.onChange(() => void refreshTeams());
  // Перемещение идёт мимо worker'а: список перечитывается как после любого изменения.
  setOnMoved(() => {
    s.notifyChanged();
  });
  return s;
}

export function installServerUi(): void {
  extensions.parseSection = (path) => {
    if (path === 'shared') return 'shared';
    const m = /^project\/([^/]+)$/.exec(path);
    return m && isUuid(m[1]) ? `project:${m[1]}` : null;
  };
  extensions.sectionTitle = teamSectionTitle;
  extensions.filesNav = teamNavEntries;
  extensions.filesNavExtra = () => (
    <>
      <TeamsEntry />
      <LocalFilesEntry store={store} user={currentUser()?.id ?? ''} />
      <MoveDialog />
    </>
  );
  extensions.fileMenu = fileMenuItems;
  extensions.newFileRoute = (section) => (section.startsWith('project:') ? { kind: 'new', project: section.slice(8) } : { kind: 'new' });
  extensions.shareButton = ShareButton;
  extensions.presenceBar = PresenceBar;
  extensions.comments = CommentsUi;
}
