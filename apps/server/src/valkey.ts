// Клиент Valkey: один на сервер, встроенный `Bun.RedisClient`.

export function connectValkey(url: string): Bun.RedisClient {
  return new Bun.RedisClient(url);
}

/** Valkey отвечает на `PING`. */
export async function valkeyAlive(client: Bun.RedisClient): Promise<boolean> {
  return (await client.send('PING', [])) === 'PONG';
}
