// Общий протокол клиента и сервера: только типы и разбор сообщений, без зависимостей,
// потому что его импортируют и `apps/web`, и `apps/server` (спека подсистемы 4, раздел 3).

export * from './auth';
export * from './files';
export * from './container';
export * from './sync';
export * from './presence';
export * from './comments';
