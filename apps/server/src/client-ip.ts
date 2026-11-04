// Адрес клиента для счётчиков частоты. `X-Forwarded-For` может прислать кто угодно, поэтому
// заголовку верим, только если соединение пришло от прокси из `TRUSTED_PROXIES`.

import { isIPv4, isIPv6 } from 'node:net';

/** Один вид адреса для сравнения: `::ffff:a.b.c.d` — это IPv4, IPv6 — в каноническом виде. Не адрес — `null`. */
export function normalizeAddress(raw: string): string | null {
  if (isIPv4(raw)) return raw;
  if (!isIPv6(raw) || raw.includes('%')) return null;
  // `URL` сводит любую запись IPv6 к одной, а встроенный IPv4 — к двум шестнадцатеричным группам.
  const canonical = new URL(`http://[${raw}]/`).hostname.slice(1, -1);
  const mapped = /^::ffff:([0-9a-f]{1,4}):([0-9a-f]{1,4})$/.exec(canonical);
  if (!mapped) return canonical;
  const [high, low] = [parseInt(mapped[1], 16), parseInt(mapped[2], 16)];
  return `${high >> 8}.${high & 255}.${low >> 8}.${low & 255}`;
}

const ipv4Number = (a: string) => a.split('.').reduce((n, part) => n * 256 + Number(part), 0);

/** Запись списка как правило: разбор один и для проверки при запуске, и для сравнения. */
function parseEntry(entry: string): ((address: string) => boolean) | string {
  const slash = entry.indexOf('/');
  if (slash < 0) {
    const exact = normalizeAddress(entry);
    return exact ? (a) => a === exact : `«${entry}» — не адрес`;
  }
  const base = entry.slice(0, slash);
  const bits = entry.slice(slash + 1);
  if (!isIPv4(base) || !/^\d{1,2}$/.test(bits) || Number(bits) > 32) return `«${entry}» — нужен IPv4 CIDR, например 10.0.0.0/8`;
  // Всему интернету верить нельзя: это та же подмена адреса, что и без списка.
  if (Number(bits) === 0) return `«${entry}» — доверие всем адресам`;
  const size = 2 ** (32 - Number(bits));
  const start = Math.floor(ipv4Number(base) / size);
  return (a) => isIPv4(a) && Math.floor(ipv4Number(a) / size) === start;
}

/** Что не так с записью `TRUSTED_PROXIES`; всё хорошо — `null`. */
export function proxyProblem(entry: string): string | null {
  const parsed = parseEntry(entry);
  return typeof parsed === 'string' ? parsed : null;
}

export type ProxyMatcher = (address: string) => boolean;

/** Адрес из списка прокси? Адреса сравниваются в нормальном виде, пустой список не верит никому. */
export function proxyMatcher(entries: readonly string[]): ProxyMatcher {
  const rules = entries.map((entry) => {
    const parsed = parseEntry(entry);
    if (typeof parsed === 'string') throw new Error(`TRUSTED_PROXIES: ${parsed}`);
    return parsed;
  });
  return (address) => {
    const a = normalizeAddress(address);
    return a !== null && rules.some((rule) => rule(a));
  };
}

/** Соединение от прокси из списка — идём по заголовку справа и берём первый адрес не из списка:
 * левее него записи прислал сам клиент. Иначе, и если все записи из списка, — адрес соединения.
 * Мусор в заголовке — тоже адрес соединения: общий счётчик, а не новый на каждое значение. */
export function clientIp(req: Request, peer: string | undefined, trusted: ProxyMatcher): string {
  if (!peer) return 'unknown';
  const from = normalizeAddress(peer) ?? peer;
  if (!trusted(from)) return from;
  const chain = (req.headers.get('x-forwarded-for') ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
  for (const entry of chain.reverse()) {
    const address = normalizeAddress(entry);
    if (!address) return from;
    if (!trusted(address)) return address;
  }
  return from;
}
