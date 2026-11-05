// Bun Worker сжатия (спека подсистемы 4, раздел 7.4): по одному файлу за раз. Своё соединение с базой и свой wasm: основной поток
// от паники и памяти Loro отделён, а `Compactor` пересоздаёт worker каждые 50 задач.

import { FsBlobStore } from '../blob-store';
import { createPool } from '../db';
import { compactFile, type CompactResult } from './compact';

export interface CompactJob {
  id: number;
  file: string;
}

export type CompactReply = { id: number; ok: true; result: CompactResult } | { id: number; ok: false; message: string };

export interface CompactInit {
  databaseUrl: string;
  dataDir: string;
  maxContent?: number;
}

const scope = globalThis as unknown as Worker;
let deps: { pool: ReturnType<typeof createPool>; blobs: FsBlobStore; maxContent?: number } | null = null;

function ready(c: CompactInit) {
  deps ??= {
    pool: createPool(c.databaseUrl, { max: 2, statement_timeout: 120_000, query_timeout: 125_000 }),
    blobs: new FsBlobStore(c.dataDir),
    maxContent: c.maxContent,
  };
  return deps;
}

scope.onmessage = (e: MessageEvent<CompactJob & { init: CompactInit }>) => {
  const { id, file, init: given } = e.data;
  const d = ready(given);
  compactFile(d.pool, d.blobs, file, d.maxContent).then(
    (result) => scope.postMessage({ id, ok: true, result } satisfies CompactReply),
    (err: unknown) =>
      scope.postMessage({ id, ok: false, message: err instanceof Error ? err.message : String(err) } satisfies CompactReply),
  );
};
