// Снимок одного документа из контейнера файла: сервер не держит файл в памяти, читает только нужные куски.

import { ContainerError, scanContainer } from '@ferrum/protocol';
import type { BlobStore } from '../blob-store';

/** Секции контейнера, которых нет в `packages/protocol`: корень, страница, кэш сцены (`crates/ferrum-doc/src/container.rs`). */
export const SECTION_ROOT = 1;
export const SECTION_PAGE = 2;
export const SECTION_SCENE = 5;

/** Где в файле данные секции документа: их можно отдать потоком или прочитать, зная размер заранее. */
export interface DocSection {
  blob: Blob;
  start: number;
  size: number;
}

/** Секция документа без чтения её данных; файла нет, контейнер чужой или секции нет — `null`. */
export async function findDocSection(blobs: BlobStore, contentKey: string | null, doc: string): Promise<DocSection | null> {
  if (!contentKey) return null;
  const blob = await blobs.open(contentKey);
  if (!blob) return null;
  const read = async (s: number, e: number) => new Uint8Array(await blob.slice(s, e).arrayBuffer());
  try {
    const spans = await scanContainer(blob.size, read);
    const span = spans.find((x) => (doc === 'root' ? x.kind === SECTION_ROOT : x.kind === SECTION_PAGE && x.id === doc));
    return span ? { blob, start: span.data, size: span.size } : null;
  } catch (e) {
    if (e instanceof ContainerError) return null;
    throw e;
  }
}

/** Данные секции целиком. */
export async function readSection(section: DocSection): Promise<Uint8Array> {
  return new Uint8Array(await section.blob.slice(section.start, section.start + section.size).arrayBuffer());
}

/** Данные секции документа; файла нет, контейнер чужой или секции нет — `null`. */
export async function readDocSnapshot(blobs: BlobStore, contentKey: string | null, doc: string): Promise<Uint8Array | null> {
  const section = await findDocSection(blobs, contentKey, doc);
  return section ? readSection(section) : null;
}
