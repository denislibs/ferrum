// Хранилище Better Auth в Valkey на встроенном `Bun.RedisClient`: сессии, токены входа,
// счётчики частоты. `getAndDelete` (`GETDEL`) делает ссылку из письма одноразовой, а
// `increment` одним скриптом не даёт счётчику остаться без срока.

import type { SecondaryStorage } from 'better-auth';

const INCREMENT = "local v = redis.call('INCR', KEYS[1]); if v == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end; return v";

export function valkeyStorage(client: Bun.RedisClient): SecondaryStorage {
  return {
    get: (key) => client.get(key),
    getAndDelete: (key) => client.getdel(key),
    increment: async (key, ttl) => {
      if (!Number.isInteger(ttl) || ttl <= 0) throw new TypeError('Срок счётчика — целое число секунд больше нуля');
      return Number(await client.send('EVAL', [INCREMENT, '1', key, String(ttl)]));
    },
    set: async (key, value, ttl) => {
      if (ttl) await client.send('SET', [key, value, 'EX', String(ttl)]);
      else await client.set(key, value);
    },
    delete: async (key) => {
      await client.del(key);
    },
  };
}
