// Имя скачиваемого файла: имя документа без символов, запрещённых в именах файлов.

import { expect, test } from '@playwright/test';
import { fileName } from '../src/ui/files';

test('имя файла очищается', () => {
  expect(fileName('Макет', '.ferrum')).toBe('Макет.ferrum');
  expect(fileName('', '.ferrum')).toBe('Без названия.ferrum');
  expect(fileName('  ', '.ferrum')).toBe('Без названия.ferrum');
  expect(fileName('a/b\\c:d*e?f"g<h>i|j', '.ferrum')).toBe('a_b_c_d_e_f_g_h_i_j.ferrum');
  expect(fileName('..', '.ferrum')).toBe('Без названия.ferrum');
  expect(fileName(' .скрытый. ', '.ferrum')).toBe('скрытый.ferrum');
  expect(fileName('a\u0000\u001fb', '.ferrum')).toBe('a__b.ferrum');
  expect(fileName('я'.repeat(500), '.ferrum')).toBe(`${'я'.repeat(120)}.ferrum`);
});
