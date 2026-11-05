import { expect, test } from 'bun:test';
import { isFileAccess } from '../src/files';

test('проверка доступа: `canShare` необязателен, но неверного вида не проходит', () => {
  expect(isFileAccess({ role: 'editor', version: 1 })).toBe(true);
  expect(isFileAccess({ role: 'editor', version: 1, canShare: false })).toBe(true);
  expect(isFileAccess({ role: 'editor', version: 1, canShare: 'нет' })).toBe(false);
});
