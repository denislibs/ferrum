// Клиент Valkey: один на сервер, встроенный `Bun.RedisClient`.

/** Подключиться и вернуть клиент, который переживёт любой простой Valkey. Умолчания Bun
 * (20 повторов, очередь команд на время обрыва) дают два дефекта: после простоя дольше
 * ~25 с клиент не возвращается никогда, а запросы на время простоя повисают. Поэтому
 * повторов практически без предела, а очередь выключена: команда во время обрыва сразу
 * получает ошибку. Без очереди первую команду нельзя слать до готовности соединения, поэтому
 * функция ждёт подключения; недоступный Valkey при старте — отказ запуска, контейнер
 * перезапустится. */
export async function connectValkey(url: string): Promise<Bun.RedisClient> {
  const client = new Bun.RedisClient(url, { maxRetries: 2 ** 31 - 1, enableOfflineQueue: false });
  await client.connect();
  return client;
}

/** Valkey отвечает на `PING`. */
export async function valkeyAlive(client: Bun.RedisClient): Promise<boolean> {
  return (await client.send('PING', [])) === 'PONG';
}
