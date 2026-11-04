// Настройки сервера из окружения (`.env`, образец — `.env.example`). Неверное — отказ при
// запуске со всем списком сразу, а не ошибка на первом запросе.

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
  /** За обратным прокси: адрес клиента из `X-Forwarded-For`, иначе — адрес соединения. */
  trustProxy: boolean;
}

const MIN_SECRET_CHARS = 32;

export function loadConfig(env: Record<string, string | undefined> = process.env): Config {
  const problems: string[] = [];
  const need = (name: string): string => {
    const v = env[name]?.trim();
    if (!v) problems.push(`${name} не задан`);
    return v ?? '';
  };
  const authSecret = need('BETTER_AUTH_SECRET');
  if (authSecret && authSecret.length < MIN_SECRET_CHARS) problems.push(`BETTER_AUTH_SECRET короче ${MIN_SECRET_CHARS} символов`);
  const publicUrl = need('PUBLIC_URL').replace(/\/+$/, '');
  if (publicUrl && !/^https?:\/\/[^/]+$/.test(publicUrl)) problems.push('PUBLIC_URL должен быть вида https://адрес, без пути');
  const port = Number(env.FERRUM_PORT ?? 8090);
  if (!Number.isInteger(port) || port < 0 || port > 65535) problems.push('FERRUM_PORT — неверный порт');
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
    trustProxy: env.TRUST_PROXY === '1',
  };
  if (problems.length > 0) throw new Error(`Настройки сервера: ${problems.join('; ')}`);
  return config;
}
