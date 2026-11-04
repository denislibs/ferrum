import { expect, test } from 'bun:test';
import { clientIp, normalizeAddress, proxyMatcher, proxyProblem } from '../src/client-ip';

const req = (xff?: string) => new Request('http://x/', { headers: xff === undefined ? {} : { 'x-forwarded-for': xff } });

test('адреса: форма `::ffff:a.b.c.d` приводится к IPv4, IPv6 — к одному виду, мусор — null', () => {
  expect(normalizeAddress('203.0.113.9')).toBe('203.0.113.9');
  expect(normalizeAddress('::ffff:203.0.113.9')).toBe('203.0.113.9');
  expect(normalizeAddress('::FFFF:CB00:7109')).toBe('203.0.113.9');
  expect(normalizeAddress('2001:DB8:0:0:0:0:0:1')).toBe('2001:db8::1');
  for (const bad of ['', 'мусор', '1.2.3', '1.2.3.4:80', '1.2.3.4/8', 'fe80::1%eth0', '[::1]']) expect(normalizeAddress(bad)).toBeNull();
});

test('список прокси: точные адреса и IPv4 CIDR', () => {
  const trusted = proxyMatcher(['10.0.0.0/8', '192.0.2.7', '::ffff:198.51.100.1', '2001:db8::1', '203.0.113.128/25', '198.51.100.9/32']);
  for (const yes of ['10.1.2.3', '::ffff:10.1.2.3', '192.0.2.7', '198.51.100.1', '2001:DB8::1', '203.0.113.200', '198.51.100.9'])
    expect([yes, trusted(yes)]).toEqual([yes, true]);
  for (const no of ['11.0.0.1', '192.0.2.8', '2001:db8::2', '203.0.113.127', '198.51.100.8', 'мусор', 'unknown'])
    expect([no, trusted(no)]).toEqual([no, false]);
  expect(proxyMatcher([])('127.0.0.1')).toBe(false);
});

test('список прокси: неверная запись называется, доверие всем не принимается', () => {
  for (const ok of ['10.0.0.1', '10.0.0.0/8', '::1', '::ffff:10.0.0.1']) expect(proxyProblem(ok)).toBeNull();
  for (const bad of ['прокси', '10.0.0.0/33', '10.0.0.0/x', '10.0.0.0/', '2001:db8::/32', '0.0.0.0/0', '10.0.0.1/8/8'])
    expect([bad, typeof proxyProblem(bad)]).toEqual([bad, 'string']);
  expect(() => proxyMatcher(['прокси'])).toThrow('прокси');
});

test('адрес клиента: заголовок учитывается, только если соединение пришло от прокси из списка', () => {
  const none = proxyMatcher([]);
  expect(clientIp(req('1.1.1.1'), '9.9.9.9', none)).toBe('9.9.9.9');
  const trusted = proxyMatcher(['10.0.0.0/8', '192.0.2.7']);
  // Соединение не от прокси: заголовок прислал сам клиент.
  expect(clientIp(req('1.1.1.1'), '9.9.9.9', trusted)).toBe('9.9.9.9');
  expect(clientIp(req('1.1.1.1'), undefined, trusted)).toBe('unknown');
  expect(clientIp(req('1.1.1.1'), '::ffff:10.0.0.1', trusted)).toBe('1.1.1.1');
  expect(clientIp(req(), '10.0.0.1', trusted)).toBe('10.0.0.1');
  expect(clientIp(req('::ffff:1.1.1.1'), '10.0.0.1', trusted)).toBe('1.1.1.1');
});

test('адрес клиента: идём по заголовку справа, подмена слева не помогает', () => {
  const trusted = proxyMatcher(['10.0.0.0/8', '192.0.2.7']);
  // Клиент подставил 6.6.6.6, настоящий адрес дописал первый прокси, а второй соединился с нами.
  expect(clientIp(req('6.6.6.6, 1.1.1.1, 192.0.2.7'), '10.0.0.1', trusted)).toBe('1.1.1.1');
  expect(clientIp(req('6.6.6.6,1.1.1.1'), '10.0.0.1', trusted)).toBe('1.1.1.1');
  // Все из списка: клиент неизвестен, считаем по адресу соединения.
  expect(clientIp(req('192.0.2.7, 10.0.0.5'), '10.0.0.1', trusted)).toBe('10.0.0.1');
  // Мусор: общий счётчик прокси, а не новый счётчик на каждое значение.
  expect(clientIp(req('garbage, 192.0.2.7'), '10.0.0.1', trusted)).toBe('10.0.0.1');
});
