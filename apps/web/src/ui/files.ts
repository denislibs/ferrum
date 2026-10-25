// Файлы в браузере (этап 5 — минимум): скачать байты как файл. Открытие,
// «Сохранить как…» и хранение — в files.ts дополняются дальше.

/** Скачивает байты файлом `name` (ссылка с `download`, адрес освобождается). */
export function downloadBytes(name: string, bytes: Uint8Array, type: string): void {
  const url = URL.createObjectURL(new Blob([bytes as BlobPart], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}
