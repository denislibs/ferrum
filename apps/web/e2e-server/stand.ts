// Стенд E2E: команды compose, письма через API Mailpit, Valkey напрямую.

import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { expect } from '@playwright/test';

const ROOT = fileURLToPath(new URL('../../../', import.meta.url));
const MAILPIT = 'http://localhost:18025/api/v1';

export function compose(...args: string[]): string {
  return execFileSync(
    'docker',
    [
      'compose',
      '-p',
      'ferrum-test',
      '--env-file',
      'docker-compose.test.env',
      '-f',
      'docker-compose.yml',
      '-f',
      'docker-compose.test.yml',
      ...args,
    ],
    { cwd: ROOT, encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'] },
  );
}

export function valkey(...args: string[]): string {
  return compose('exec', '-T', 'valkey', 'valkey-cli', ...args);
}

/** Чистый стенд между тестами: счётчики частоты и письма. */
export async function resetStand(): Promise<void> {
  valkey('FLUSHALL');
  await fetch(`${MAILPIT}/messages`, { method: 'DELETE' });
}

interface MailSummary {
  ID: string;
  To: { Address: string }[];
}

/** Ссылки из писем на этот адрес, новые первыми; ждёт, пока их станет `count`. */
export async function linksFor(email: string, count = 1): Promise<string[]> {
  let found: MailSummary[] = [];
  await expect
    .poll(
      async () => {
        const list = (await (await fetch(`${MAILPIT}/messages`)).json()) as { messages: MailSummary[] };
        found = list.messages.filter((m) => m.To.some((t) => t.Address === email));
        return found.length;
      },
      { message: `письма на ${email}` },
    )
    .toBeGreaterThanOrEqual(count);
  return Promise.all(
    found.map(async (m) => {
      const full = (await (await fetch(`${MAILPIT}/message/${m.ID}`)).json()) as { Text: string };
      return /http\S+#\/auth\/confirm\S+/.exec(full.Text)![0];
    }),
  );
}

export async function mailCount(email: string): Promise<number> {
  const list = (await (await fetch(`${MAILPIT}/messages`)).json()) as { messages: MailSummary[] };
  return list.messages.filter((m) => m.To.some((t) => t.Address === email)).length;
}

/** Адреса тестов не повторяются: пользователи живут в базе стенда весь прогон. */
let seq = 0;
export function newEmail(project: string): string {
  return `user-${Date.now().toString(36)}-${++seq}-${project}@example.com`;
}
