// Перезапуск редактора после сбоя: не больше двух за минуту, тексты уведомлений.

import { expect, test } from '@playwright/test';
import { allowRestart, restartText, RESTART_LIMIT, RESTART_WINDOW_MS } from '../src/ui/restart';

test('не больше двух перезапусков за минуту', () => {
  expect(RESTART_LIMIT).toBe(2);
  const h: number[] = [];
  expect(allowRestart(h, 1000)).toBe(true);
  expect(allowRestart(h, 2000)).toBe(true);
  expect(allowRestart(h, 3000)).toBe(false);
  expect(h).toEqual([1000, 2000]);
  // Через минуту после первого — снова можно.
  expect(allowRestart(h, 1000 + RESTART_WINDOW_MS)).toBe(true);
  expect(h).toEqual([2000, 1000 + RESTART_WINDOW_MS]);
});

test('тексты: сохранено, потеряны секунды, запасной рендер', () => {
  expect(restartText({ reason: 'crash', lost: false, message: '' })).toBe('Редактор перезапущен, изменения сохранены');
  expect(restartText({ reason: 'crash', lost: true, message: '' })).toBe(
    'Редактор перезапущен. Правки последних секунд не сохранились',
  );
  expect(restartText({ reason: 'gpu', lost: false, backend: 'webgl', message: '' })).toBe(
    'Видеокарта недоступна — редактор перезапущен с рендером WebGL2',
  );
  expect(restartText({ reason: 'gpu', lost: false, backend: 'cpu', message: '' })).toBe(
    'Видеокарта недоступна — редактор перезапущен с рендером на процессоре',
  );
  expect(restartText({ reason: 'gpu', lost: true, backend: 'cpu', message: '' })).toBe(
    'Видеокарта недоступна — редактор перезапущен с рендером на процессоре. Правки последних секунд не сохранились',
  );
});
