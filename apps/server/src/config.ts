// Настройки сервера из окружения (`.env`, образец — `.env.example`). Неверное — отказ при
// запуске со всем списком сразу, а не ошибка на первом запросе.

import { proxyProblem } from './client-ip';

export interface Config {
  port: number;
  publicUrl: string;
  authSecret: string;
  databaseUrl: string;
  valkeyUrl: string;
  smtpUrl: string;
  mailFrom: string;
  /** Адреса (`ivan@x.ru`) и домены (`x.ru`) в нижнем регистре; пусто — войти может любой. */
  allowedEmails: string[];
  /** Том байтов: снимки, журналы-архивы, картинки. */
  dataDir: string;
  /** Собранный `apps/web`. */
  webDist: string;
  /** Адреса обратных прокси (точные и IPv4 CIDR): только от них верим `X-Forwarded-For`. */
  trustedProxies: string[];
}

const MIN_SECRET_CHARS = 32;

/** Дальше — только origin: браузер пришлёт в `Origin` именно его (нижний регистр, без порта по
 * умолчанию), а сравнение строгое. Всё, чего в origin нет, — ошибка настройки, а не молчаливый отказ
 * каждого запроса. */
function parsePublicUrl(raw: string, problems: string[]): string {
  if (!raw) return '';
  const problem = 'PUBLIC_URL должен быть вида https://адрес, без пути, запроса и учётных данных';
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    problems.push(problem);
    return '';
  }
  const bare = url.pathname === '/' && !url.search && !url.hash && !url.username && !url.password;
  if (!bare || !['http:', 'https:'].includes(url.protocol)) {
    problems.push(problem);
    return '';
  }
  return url.origin;
}

export function loadConfig(env: Record<string, string | undefined> = process.env): Config {
  const problems: string[] = [];
  const need = (name: string): string => {
    const v = env[name]?.trim();
    if (!v) problems.push(`${name} не задан`);
    return v ?? '';
  };
  const authSecret = need('BETTER_AUTH_SECRET');
  if (authSecret && authSecret.length < MIN_SECRET_CHARS) problems.push(`BETTER_AUTH_SECRET короче ${MIN_SECRET_CHARS} символов`);
  const publicUrl = parsePublicUrl(need('PUBLIC_URL'), problems);
  const port = Number(env.FERRUM_PORT?.trim() || 8090);
  if (!Number.isInteger(port) || port < 0 || port > 65535) problems.push('FERRUM_PORT — неверный порт');
  const trustedProxies = (env.TRUSTED_PROXIES ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  for (const entry of trustedProxies) {
    const problem = proxyProblem(entry);
    if (problem) problems.push(`TRUSTED_PROXIES: ${problem}`);
  }
  const config: Config = {
    port,
    publicUrl,
    authSecret,
    databaseUrl: need('DATABASE_URL'),
    valkeyUrl: need('VALKEY_URL'),
    smtpUrl: need('SMTP_URL'),
    mailFrom: need('MAIL_FROM'),
    allowedEmails: (env.ALLOWED_EMAILS ?? '')
      .split(',')
      .map((s) => s.trim().toLowerCase())
      .filter(Boolean),
    dataDir: env.FERRUM_DATA_DIR ?? '/data',
    webDist: env.FERRUM_WEB_DIST ?? '/app/web',
    trustedProxies,
  };
  if (problems.length > 0) throw new Error(`Настройки сервера: ${problems.join('; ')}`);
  return config;
}
