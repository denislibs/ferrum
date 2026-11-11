// Стенд E2E: команды compose, письма через API Mailpit, Valkey напрямую.

import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { expect, type Page } from '@playwright/test';

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
  Subject: string;
  To: { Address: string }[];
}

const LOGIN_SUBJECT = 'Вход в Ferrum';
const isLogin = (m: MailSummary, email: string) => m.Subject === LOGIN_SUBJECT && m.To.some((t) => t.Address === email);

/** Ссылки из писем на этот адрес, новые первыми; ждёт, пока их станет `count`. */
export async function linksFor(email: string, count = 1): Promise<string[]> {
  let found: MailSummary[] = [];
  await expect
    .poll(
      async () => {
        const list = (await (await fetch(`${MAILPIT}/messages`)).json()) as { messages: MailSummary[] };
        found = list.messages.filter((m) => isLogin(m, email));
        return found.length;
      },
      // Письмо уходит после ответа сервера: под нагрузкой не сразу.
      { message: `письма на ${email}`, timeout: 15_000 },
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
  return list.messages.filter((m) => isLogin(m, email)).length;
}

/** Адреса тестов не повторяются: пользователи живут в базе стенда весь прогон. */
let seq = 0;
export function newEmail(project: string): string {
  return `user-${Date.now().toString(36)}-${++seq}-${project}@example.com`;
}

export async function requestLink(page: Page, email: string): Promise<void> {
  await page.getByLabel('Электронная почта').fill(email);
  await page.getByRole('button', { name: 'Получить ссылку' }).click();
  await expect(page.getByRole('heading', { name: 'Проверьте почту' })).toBeVisible();
  await expect(page.getByText(email)).toBeVisible();
}

/** Вход целиком: от экрана входа до приложения. */
export async function signIn(page: Page, email: string, name = 'Иван Петров'): Promise<void> {
  // Новое письмо, а не одно из прежних: оно уходит после ответа сервера и приходит не сразу.
  const before = await mailCount(email);
  await requestLink(page, email);
  const [link] = await linksFor(email, before + 1);
  await page.goto(link);
  await page.getByRole('button', { name: 'Войти', exact: true }).click();
  if (name) {
    await expect(page.getByRole('heading', { name: 'Как вас зовут?' })).toBeVisible();
    await page.getByLabel('Имя').fill(name);
    await page.getByRole('button', { name: 'Продолжить' }).click();
  }
}

/** Письма на адрес с такой темой, новые первыми: текст целиком. Письмо уходит после ответа сервера. */
export async function mailsFor(email: string, subject: string, count = 1): Promise<string[]> {
  let found: MailSummary[] = [];
  await expect
    .poll(
      async () => {
        const list = (await (await fetch(`${MAILPIT}/messages`)).json()) as { messages: MailSummary[] };
        found = list.messages.filter((m) => m.Subject === subject && m.To.some((t) => t.Address === email));
        return found.length;
      },
      { message: `письма «${subject}» на ${email}`, timeout: 15_000 },
    )
    .toBeGreaterThanOrEqual(count);
  return Promise.all(found.map(async (m) => ((await (await fetch(`${MAILPIT}/message/${m.ID}`)).json()) as { Text: string }).Text));
}

/** Письма на адрес, тема которых подходит: текст целиком, новые первыми; ждёт, пока их станет `count`. */
export async function mailsMatching(email: string, subject: RegExp, count = 1): Promise<{ subject: string; text: string }[]> {
  let found: MailSummary[] = [];
  await expect
    .poll(
      async () => {
        const list = (await (await fetch(`${MAILPIT}/messages`)).json()) as { messages: MailSummary[] };
        found = list.messages.filter((m) => subject.test(m.Subject) && m.To.some((t) => t.Address === email));
        return found.length;
      },
      { message: `письма ${subject} на ${email}`, timeout: 20_000 },
    )
    .toBeGreaterThanOrEqual(count);
  return Promise.all(
    found.map(async (m) => ({
      subject: m.Subject,
      text: ((await (await fetch(`${MAILPIT}/message/${m.ID}`)).json()) as { Text: string }).Text,
    })),
  );
}
