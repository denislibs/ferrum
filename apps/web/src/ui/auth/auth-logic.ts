// Чистая логика входа: что показать, сколько ждать до повторной отправки, тексты ошибок,
// кружок с инициалами. Без DOM и сети: это проверяют модульные тесты.

import { parseConfirmHash, parseNext, RESEND_SECONDS, type AuthErrorCode, type SessionUser } from '@ferrum/protocol';

export type AuthScreen =
  | { kind: 'confirm'; token: string | null; next: string | null }
  | { kind: 'login'; next: string | null }
  | { kind: 'name'; user: SessionUser }
  | { kind: 'app'; user: SessionUser };

/** Адрес страницы подтверждения показывается всегда, остальное решает сессия. */
export function screenFor(hash: string, user: SessionUser | null): AuthScreen {
  const confirm = parseConfirmHash(hash);
  if (confirm) return { kind: 'confirm', ...confirm };
  if (!user) return { kind: 'login', next: parseNext(hash) };
  return user.name ? { kind: 'app', user } : { kind: 'name', user };
}

/** Сколько секунд осталось до повторной отправки; `0` — можно. */
export function resendLeft(sentAt: number, now: number): number {
  return Math.max(0, Math.ceil((sentAt + RESEND_SECONDS * 1000 - now) / 1000));
}

export type AuthFailure = AuthErrorCode | 'NETWORK';

export function failureText(f: AuthFailure): string {
  switch (f) {
    case 'BAD_REQUEST':
      return 'Проверьте, правильно ли всё заполнено';
    case 'INVALID_TOKEN':
      return 'Ссылка недействительна или устарела. Запросите новую';
    case 'TOO_MANY_REQUESTS':
      return 'Слишком много запросов. Попробуйте позже';
    case 'NETWORK':
      return 'Сервер недоступен. Проверьте соединение и попробуйте ещё раз';
    case 'UNAVAILABLE':
      return 'Сервер недоступен. Попробуйте ещё раз через минуту';
    case 'BAD_ORIGIN':
    case 'UNAUTHORIZED':
    case 'NOT_FOUND':
      return 'Не получилось. Обновите страницу и попробуйте ещё раз';
  }
}

/** Инициалы для кружка: первые буквы двух слов имени, без имени — буква адреса. */
export function initials(user: Pick<SessionUser, 'name' | 'email'>): string {
  const words = user.name.trim().split(/\s+/).filter(Boolean);
  const letters = words.length > 0 ? words.slice(0, 2).map((w) => Array.from(w)[0]) : [Array.from(user.email)[0] ?? '?'];
  return letters.join('').toUpperCase();
}

/** Оттенок кружка из идентификатора: у человека он один и тот же на всех устройствах. */
export function avatarHue(id: string): number {
  let h = 0;
  for (const c of id) h = (h * 31 + (c.codePointAt(0) ?? 0)) % 360;
  return h;
}
