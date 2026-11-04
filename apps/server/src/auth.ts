// Better Auth: magic link, пользователи в Postgres, сессии и токены — только в Valkey
// (спека подсистемы 4, раздел 5).

import { confirmHash, MAGIC_LINK_TTL_SECONDS } from '@ferrum/protocol';
import { APIError, betterAuth, type BetterAuthOptions } from 'better-auth';
import { magicLink } from 'better-auth/plugins';
import type { Pool } from 'pg';
import { magicLinkMail, mailFailure, type Mailer } from './mail';
import { valkeyStorage } from './secondary-storage';

/** Заголовок с адресом клиента: сервер ставит его сам (`clientIp`), чужой затирает. */
export const CLIENT_IP_HEADER = 'x-ferrum-client-ip';
const SESSION_SECONDS = 30 * 24 * 60 * 60;
const HOUR = 60 * 60;
/** Писем на адрес и на IP в час, спека, раздел 5. */
export const MAILS_PER_ADDRESS = 5;
export const MAILS_PER_IP = 20;

export interface AuthDeps {
  publicUrl: string;
  secret: string;
  pool: Pool;
  valkey: Bun.RedisClient;
  mailer: Mailer;
  allowedEmails: string[];
  /** Тесты сокращают сроки; по умолчанию — из спеки. */
  magicLinkSeconds?: number;
  sessionUpdateAgeSeconds?: number;
}

/** Пустой список — войти может любой; иначе адрес целиком или его домен. */
export function emailAllowed(email: string, allowed: string[]): boolean {
  if (allowed.length === 0) return true;
  return allowed.includes(email) || allowed.includes(email.slice(email.lastIndexOf('@') + 1));
}

export function authOptions(deps: AuthDeps) {
  const storage = valkeyStorage(deps.valkey);
  return {
    baseURL: deps.publicUrl,
    basePath: '/api/auth',
    secret: deps.secret,
    trustedOrigins: [deps.publicUrl],
    database: deps.pool,
    secondaryStorage: storage,
    verification: { storeInDatabase: false },
    telemetry: { enabled: false },
    session: { expiresIn: SESSION_SECONDS, updateAge: deps.sessionUpdateAgeSeconds ?? 24 * HOUR },
    rateLimit: {
      enabled: true,
      storage: 'secondary-storage',
      window: 60,
      max: 100,
      customRules: {
        // Проверка сессии только читает и нужна при каждом открытии: за общим IP (офис, шлюз) предел
        // закрыл бы приложение всем, кто уже вошёл.
        '/get-session': false,
        '/sign-in/magic-link': { window: HOUR, max: MAILS_PER_IP },
        '/magic-link/verify': { window: 60, max: 30 },
      },
    },
    advanced: {
      cookiePrefix: 'ferrum',
      useSecureCookies: deps.publicUrl.startsWith('https://'),
      ipAddress: { ipAddressHeaders: [CLIENT_IP_HEADER] },
    },
    plugins: [
      magicLink({
        expiresIn: deps.magicLinkSeconds ?? MAGIC_LINK_TTL_SECONDS,
        storeToken: 'hashed',
        sendMagicLink: async ({ email, token, metadata }) => {
          // Встроенный `rateLimit` считает только по IP и пути; счётчик на адрес — рядом,
          // в том же хранилище и после проверки IP. Считаем до проверки списка: иначе по шестому
          // ответу (429 или 200) видно, входит ли адрес в `ALLOWED_EMAILS`.
          const sent = await storage.increment(`mail-to:${email}`, HOUR);
          if (sent > MAILS_PER_ADDRESS) throw new APIError('TOO_MANY_REQUESTS');
          // Адрес не из списка: ответ тот же, письма нет, ничего не раскрываем.
          if (!emailAllowed(email, deps.allowedEmails)) return;
          const next = typeof metadata?.next === 'string' ? metadata.next : undefined;
          // Письмо собирается и уходит после ответа: ни его сборка, ни время SMTP не должны отличать
          // адрес из списка по времени ответа. В журнал — только коды сбоя, без адреса, токена и ссылки.
          setTimeout(() => {
            deps.mailer.send(magicLinkMail(email, `${deps.publicUrl}/${confirmHash(token, next)}`)).catch((e: unknown) => {
              console.error('Не удалось отправить письмо входа:', mailFailure(e));
            });
          }, 0);
        },
      }),
    ],
  } satisfies BetterAuthOptions;
}

export function createAuth(deps: AuthDeps) {
  return betterAuth(authOptions(deps));
}

export type Auth = ReturnType<typeof createAuth>;
