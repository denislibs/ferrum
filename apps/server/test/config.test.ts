import { expect, test } from 'bun:test';
import { loadConfig } from '../src/config';

const ENV = {
  BETTER_AUTH_SECRET: 's'.repeat(32),
  PUBLIC_URL: 'http://localhost:8090/',
  DATABASE_URL: 'postgres://a/b',
  VALKEY_URL: 'redis://a',
  SMTP_URL: 'smtp://a',
  MAIL_FROM: 'Ferrum <a@b.ru>',
};

test('разбирает окружение: умолчания, список адресов, адрес без косой черты', () => {
  const c = loadConfig({ ...ENV, ALLOWED_EMAILS: ' Ivan@X.ru, y.ru ,,' });
  expect(c.port).toBe(8090);
  expect(c.publicUrl).toBe('http://localhost:8090');
  expect(c.allowedEmails).toEqual(['ivan@x.ru', 'y.ru']);
  expect(c.trustProxy).toBe(false);
  expect(loadConfig({ ...ENV, TRUST_PROXY: '1', FERRUM_PORT: '0' })).toMatchObject({ trustProxy: true, port: 0 });
  expect(loadConfig(ENV).allowedEmails).toEqual([]);
});

test('неверные настройки: отказ со всем списком сразу', () => {
  expect(() => loadConfig({})).toThrow(
    /BETTER_AUTH_SECRET не задан.*PUBLIC_URL не задан.*DATABASE_URL не задан.*VALKEY_URL не задан.*SMTP_URL не задан.*MAIL_FROM не задан/,
  );
  expect(() => loadConfig({ ...ENV, BETTER_AUTH_SECRET: 'short' })).toThrow('короче 32');
  expect(() => loadConfig({ ...ENV, PUBLIC_URL: 'http://x/path' })).toThrow('без пути');
  expect(() => loadConfig({ ...ENV, FERRUM_PORT: 'abc' })).toThrow('неверный порт');
});
