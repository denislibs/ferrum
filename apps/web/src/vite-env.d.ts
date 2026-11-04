/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** `1` — автономная сборка: редактор без сервера и входа (спека подсистемы 4, раздел 5). */
  readonly VITE_FERRUM_LOCAL?: string;
}
