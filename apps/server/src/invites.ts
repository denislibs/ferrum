// Приглашения по адресу: письма и переход в доступ при первом входе. Счётчики — на адрес получателя,
// отдельно от писем входа: чужие приглашения не должны закрывать человеку вход.

import type { Pool } from 'pg';
import type { ApiDeps } from './api';
import { emailAllowed } from './auth';
import { inTransaction } from './db';
import { escapeHtml, mailFailure, type Mail } from './mail';
import { valkeyStorage } from './secondary-storage';

/** Приглашений на один адрес в час: иначе чужую почту можно забить письмами от своего имени. */
export const INVITES_PER_ADDRESS = 5;
const HOUR = 60 * 60;

/** Письмо уходит после ответа: ответ приглашающему не зависит от адреса (`ALLOWED_EMAILS`, предел).
 * Адрес вне списка и сверх предела письма не получает, но запись остаётся: при входе он её получит. */
export function sendInvite(deps: ApiDeps, to: string, mail: Mail): void {
  setTimeout(() => {
    void (async () => {
      const sent = await valkeyStorage(deps.valkey).increment!(`invite-to:${to}`, HOUR);
      if (sent > INVITES_PER_ADDRESS || !emailAllowed(to, deps.allowedEmails)) return;
      await deps.mailer.send(mail);
    })().catch((e: unknown) => console.error('Не удалось отправить приглашение:', mailFailure(e)));
  }, 0);
}

function letter(to: string, subject: string, lines: string[], link: string, action: string): Mail {
  return {
    to,
    subject,
    text: `${lines.join('\n')}\n\n${link}\n`,
    html: `${lines.map((l) => `<p>${escapeHtml(l)}</p>`).join('')}<p><a href="${escapeHtml(link)}">${escapeHtml(action)}</a></p>`,
  };
}

export const fileInviteMail = (to: string, from: string, file: string, role: 'editor' | 'viewer', link: string): Mail =>
  letter(
    to,
    'Вам открыли файл в Ferrum',
    [`${from} открывает вам файл «${file}»: ${role === 'editor' ? 'можно редактировать' : 'можно смотреть'}.`],
    link,
    'Открыть файл',
  );

export const teamInviteMail = (to: string, from: string, team: string, link: string): Mail =>
  letter(to, 'Вас пригласили в команду в Ferrum', [`${from} приглашает вас в команду «${team}».`], link, 'Открыть Ferrum');

export const accessRequestMail = (to: string, from: string, file: string, link: string): Mail =>
  letter(to, 'Запрос доступа к файлу в Ferrum', [`${from} просит доступ к файлу «${file}».`], link, 'Открыть файл');

/** Каждый вход: приглашения на этот адрес становятся доступом. Сбой не мешает входу, а приглашение,
 * вставленное в гонке с прошлым входом, не теряется: запись остаётся и перейдёт при следующем входе. */
export function claimInvites(pool: Pool, user: string): Promise<void> {
  return inTransaction(pool, async (client) => {
    const { rows } = await client.query<{ email: string }>('SELECT email FROM "user" WHERE id = $1', [user]);
    const email = rows[0]?.email ?? '';
    await client.query(
      `INSERT INTO file_access (file_id, user_id, role) SELECT file_id, $1, role FROM file_invite WHERE email = $2
       ON CONFLICT (file_id, user_id) DO NOTHING`,
      [user, email],
    );
    await client.query('DELETE FROM file_invite WHERE email = $1', [email]);
    await client.query(
      `INSERT INTO team_member (team_id, user_id, role) SELECT team_id, $1, 'member' FROM team_invite WHERE email = $2
       ON CONFLICT (team_id, user_id) DO NOTHING`,
      [user, email],
    );
    await client.query('DELETE FROM team_invite WHERE email = $1', [email]);
  });
}
