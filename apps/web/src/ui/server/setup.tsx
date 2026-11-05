// Всё серверное в интерфейсе: ставит `main.tsx` в ветке рабочей сборки. Общий код об этом не знает,
// поэтому в автономную сборку ни запросы к серверу, ни его разделы не попадают.

import { isUuid } from '@ferrum/protocol';
import { currentUser } from '../auth/session';
import { extensions } from '../extensions';
import { redeemLink, requestFileAccess } from './server-api';
import { ServerFileStore } from './server-store';

export function createServerStore(): ServerFileStore {
  // `Root` монтируется только при живой сессии (`AuthGate`).
  const s = new ServerFileStore(currentUser()!.id);
  s.redeemLink = async (id, token) => (await redeemLink(id, token)).ok;
  s.requestAccess = async (id) => (await requestFileAccess(id)).ok;
  return s;
}

export function installServerUi(): void {
  extensions.parseSection = (path) => {
    if (path === 'shared') return 'shared';
    const m = /^project\/([^/]+)$/.exec(path);
    return m && isUuid(m[1]) ? `project:${m[1]}` : null;
  };
  extensions.sectionTitle = (section) => (section === 'shared' ? 'Доступные мне' : null);
  extensions.filesNav = () => [{ kind: 'section', id: 'shared', title: 'Доступные мне', icon: 'file' }];
  extensions.newFileRoute = (section) => (section.startsWith('project:') ? { kind: 'new', project: section.slice(8) } : { kind: 'new' });
}
