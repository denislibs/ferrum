// Снимок одного документа из контейнера файла: сервер не держит файл в памяти, читает только нужные куски.

import { ContainerError, scanContainer } from '@ferrum/protocol';
import type { BlobStore } from '../blob-store';

/** Секции контейнера, которых нет в `packages/protocol`: корень, страница, кэш сцены (`crates/ferrum-doc/src/container.rs`). */
export const SECTION_ROOT = 1;
export const SECTION_PAGE = 2;
export const SECTION_SCENE = 5;

/** Данные секции документа; файла нет, контейнер чужой или секции нет — `null`. */
export async function readDocSnapshot(blobs: BlobStore, contentKey: string | null, doc: string): Promise<Uint8Array | null> {
  if (!contentKey) return null;
  const blob = await blobs.open(contentKey);
  if (!blob) return null;
  const read = async (s: number, e: number) => new Uint8Array(await blob.slice(s, e).arrayBuffer());
  try {
    const spans = await scanContainer(blob.size, read);
    const span = spans.find((x) => (doc === 'root' ? x.kind === SECTION_ROOT : x.kind === SECTION_PAGE && x.id === doc));
    return span ? await read(span.data, span.data + span.size) : null;
  } catch (e) {
    if (e instanceof ContainerError) return null;
    throw e;
  }
}
