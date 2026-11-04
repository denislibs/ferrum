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
  expect(c.trustedProxies).toEqual([]);
  expect(loadConfig({ ...ENV, TRUSTED_PROXIES: ' 10.0.0.0/8, ::ffff:172.18.0.1 ,,', FERRUM_PORT: '0' })).toMatchObject({
    trustedProxies: ['10.0.0.0/8', '::ffff:172.18.0.1'],
    port: 0,
  });
  // Прежний флаг больше ни на что не влияет: заголовку верим только по списку.
  expect(loadConfig({ ...ENV, TRUST_PROXY: '1' }).trustedProxies).toEqual([]);
  expect(loadConfig(ENV).allowedEmails).toEqual([]);
});

test('неверные настройки: отказ со всем списком сразу', () => {
  expect(() => loadConfig({})).toThrow(
    /BETTER_AUTH_SECRET не задан.*PUBLIC_URL не задан.*DATABASE_URL не задан.*VALKEY_URL не задан.*SMTP_URL не задан.*MAIL_FROM не задан/,
  );
  expect(() => loadConfig({ ...ENV, BETTER_AUTH_SECRET: 'short' })).toThrow('короче 32');
  expect(() => loadConfig({ ...ENV, PUBLIC_URL: 'http://x/path' })).toThrow('без пути');
  expect(() => loadConfig({ ...ENV, FERRUM_PORT: 'abc' })).toThrow('неверный порт');
  expect(() => loadConfig({ ...ENV, TRUSTED_PROXIES: 'прокси,0.0.0.0/0' })).toThrow(
    /TRUSTED_PROXIES: «прокси».*TRUSTED_PROXIES: «0\.0\.0\.0\/0»/,
  );
});

test('PUBLIC_URL сводится к origin: браузер пришлёт именно его', () => {
  for (const [given, origin] of [
    ['https://App.Example.com', 'https://app.example.com'],
    ['https://x.ru:443', 'https://x.ru'],
    ['http://x.ru:80/', 'http://x.ru'],
    ['http://localhost:8090/', 'http://localhost:8090'],
    ['  https://x.ru  ', 'https://x.ru'],
  ]) {
    expect([given, loadConfig({ ...ENV, PUBLIC_URL: given }).publicUrl]).toEqual([given, origin]);
  }
});

test('PUBLIC_URL с чем-то кроме адреса: отказ при запуске', () => {
  for (const bad of [
    'x.ru',
    'не адрес',
    'ftp://x.ru',
    'https://x.ru/app',
    'https://x.ru//',
    'https://x.ru/?a=1',
    'https://x.ru/#a',
    'https://user:pass@x.ru',
    'https://user@x.ru',
  ]) {
    expect(() => loadConfig({ ...ENV, PUBLIC_URL: bad })).toThrow('PUBLIC_URL');
  }
});

test('FERRUM_PORT пустой — 8090, а не случайный порт', () => {
  expect(loadConfig({ ...ENV, FERRUM_PORT: '' }).port).toBe(8090);
  expect(loadConfig({ ...ENV, FERRUM_PORT: '  ' }).port).toBe(8090);
  expect(loadConfig({ ...ENV, FERRUM_PORT: '9000' }).port).toBe(9000);
});
