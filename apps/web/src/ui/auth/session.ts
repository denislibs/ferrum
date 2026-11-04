// Текущий пользователь для настроек и экрана файлов, смена входа в других вкладках.

import { createSignal } from 'solid-js';
import type { SessionUser } from '@ferrum/protocol';
import { editorHooks } from '../extensions';
import { signOut, signOutEverywhere } from './auth-api';
import { failureText } from './auth-logic';

const [user, setUser] = createSignal<SessionUser | null>(null);
export const currentUser = user;
export const setCurrentUser = setUser;

/** Другие вкладки узнают о входе и выходе (поведение, как в Figma). Канал один на страницу:
 * своих сообщений он не получает. */
let channel: BroadcastChannel | undefined;
const authChannel = () => (channel ??= new BroadcastChannel('ferrum-auth'));

export function announceAuthChange(): void {
  authChannel().postMessage('changed');
}

export function onAuthChange(listener: () => void): () => void {
  authChannel().onmessage = listener;
  return () => (authChannel().onmessage = null);
}

/** Выход: сначала редактор по своему протоколу ухода дописывает файл, а если запись не удалась,
 * спрашивает человека; «Остаться» отменяет выход (без текста ошибки). Затем сессия снимается и
 * страница загружается заново, она покажет экран входа. Ошибка запроса — текст. */
export async function leave(everywhere: boolean): Promise<string | null> {
  if (!(await (editorHooks.leave?.() ?? true))) return null;
  const r = await (everywhere ? signOutEverywhere() : signOut());
  if (!r.ok) return failureText(r.failure);
  announceAuthChange();
  location.reload();
  return null;
}
