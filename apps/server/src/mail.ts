// Письма: `nodemailer` по SMTP_URL. В разработке их перехватывает Mailpit.

import nodemailer from 'nodemailer';

export interface Mail {
  to: string;
  subject: string;
  text: string;
  html: string;
}

export interface Mailer {
  send(mail: Mail): Promise<void>;
}

export function smtpMailer(url: string, from: string): Mailer {
  const transport = nodemailer.createTransport(url);
  return {
    send: async (mail) => {
      await transport.sendMail({ from, ...mail });
    },
  };
}

/** Причина отказа для журнала: только коды. Текст ошибки nodemailer содержит ответ SMTP, а в нём
 * часто адрес получателя. */
export function mailFailure(e: unknown): string {
  const { code, responseCode } = (typeof e === 'object' && e !== null ? e : {}) as { code?: unknown; responseCode?: unknown };
  return `код ${typeof code === 'string' ? code : 'неизвестен'}, ответ ${typeof responseCode === 'number' ? responseCode : 'неизвестен'}`;
}

const ESCAPES: Record<string, string> = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
const escapeHtml = (s: string) => s.replace(/[&<>"']/g, (c) => ESCAPES[c]);

export function magicLinkMail(to: string, link: string): Mail {
  const a = escapeHtml(link);
  return {
    to,
    subject: 'Вход в Ferrum',
    text: `Чтобы войти в Ferrum, откройте ссылку:\n\n${link}\n\nОна действует 15 минут и только один раз. Если вы не просили вход, просто удалите это письмо.\n`,
    html: `<p>Чтобы войти в Ferrum, откройте ссылку:</p><p><a href="${a}">Войти в Ferrum</a></p><p>Она действует 15 минут и только один раз. Если вы не просили вход, просто удалите это письмо.</p>`,
  };
}
