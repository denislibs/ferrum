// Заглушка `setup.tsx` для автономной сборки (`vite.config.ts`): иначе сборщик увидел бы в `server-store.ts`
// `new Worker(...)` и выпустил файл worker'а с кодом сервера, даже если до него нельзя дойти.

const unreachable = (): never => {
  throw new Error('Файлов на сервере нет в автономной сборке');
};

export const createServerStore = unreachable;
export const installServerUi = unreachable;
