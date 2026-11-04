# Как запустить Ferrum

## Сервер, одной командой

Нужен Docker с Compose.

```bash
cp .env.example .env
# впишите в .env секрет: BETTER_AUTH_SECRET=$(openssl rand -base64 32)
docker compose up --build
```

Откройте http://localhost:8090. Письма со ссылками входа перехватывает Mailpit: http://localhost:8025 (профиль `dev` из `.env.example`; порт открыт только на `127.0.0.1`: в письмах ссылки входа, а у веб-интерфейса нет пароля).

Первая сборка образа идёт несколько минут: ядро Rust в wasm, затем интерфейс, затем сервер. Данные живут в томах `ferrum-postgres`, `ferrum-valkey` и `ferrum-data`; `docker compose down` их не трогает.

### На рабочем сервере

- **До первого запуска** смените `POSTGRES_PASSWORD`: том создаётся с этим паролем, и смена в `.env` после запуска уже не действует. Пароль попадает в `DATABASE_URL`, поэтому без спецсимволов URL (`@ : / ? #`).
- Уберите `COMPOSE_PROFILES=dev`, задайте настоящий `SMTP_URL` и `MAIL_FROM`, `PUBLIC_URL` с `https://`. `PUBLIC_URL` — только адрес, без пути и запроса: браузер присылает его в `Origin`, и при смене `FERRUM_PORT` его тоже нужно поменять. С `https://` сервер отдаёт `Strict-Transport-Security` на год.
- Для почты берите `smtps://`. С `smtp://` письмо со ссылкой входа можно прочитать по дороге, если кто-то на пути снимет STARTTLS.
- `ALLOWED_EMAILS` ограничивает, кто может войти. Письмо уходит без ожидания ответа SMTP: сбой отправки виден только в журнале сервера, там его код и ответ сервера, без адреса.
- Смена `BETTER_AUTH_SECRET` выводит всех: действующие cookie перестают проходить проверку подписи.

### Обратный прокси и порт

По умолчанию порт открыт только на `127.0.0.1` (`FERRUM_BIND`). Снаружи сервер должен быть доступен через обратный прокси:

- `TRUSTED_PROXIES` за прокси обязателен: иначе все клиенты считаются одним адресом соединения и делят предел 20 писем в час. Это список через запятую, точные адреса и IPv4 CIDR. Прокси на той же машине приходит с адреса шлюза сети Docker, например `TRUSTED_PROXIES=172.16.0.0/12`.
- Прокси обязан перезаписывать `X-Forwarded-For` или дописывать в него адрес клиента, а не пропускать чужой как есть. В nginx: `proxy_set_header X-Forwarded-For $remote_addr;`. Сервер идёт по заголовку справа и берёт первый адрес не из списка, поэтому цепочку прокси перечисляйте целиком.
- От адреса вне списка заголовок не учитывается. Пустой список — заголовку не верим.
- Прямой доступ без прокси: `FERRUM_BIND=0.0.0.0`. Опубликованный Docker порт обходит ufw. За пробросом порта Docker Desktop и OrbStack адрес клиента — адрес шлюза, предел на IP становится общим на всех.

### Снять сессии человека

Адрес, убранный из `ALLOWED_EMAILS`, больше не может запросить ссылку, но открытые сессии живут и продлеваются. После смены списка и перезапуска сервера:

```bash
USER_ID=$(docker compose exec -T postgres psql -U ferrum -d ferrum -Atc "SELECT id FROM \"user\" WHERE email = 'ivan@example.com'")
docker compose exec -T valkey valkey-cli EVAL "local l = redis.call('GET', KEYS[1]); if l then for _, s in ipairs(cjson.decode(l)) do redis.call('DEL', s.token) end; redis.call('DEL', KEYS[1]) end; return 'ok'" 1 "active-sessions-$USER_ID"
```

Ссылка, выданная до исключения, действует ещё 15 минут: повторите команду по их истечении.

## Редактор без сервера

Автономная сборка (`VITE_FERRUM_LOCAL=1`, режим `standalone`): без входа, файлы в IndexedDB.

```bash
cd apps/web
npm ci
npm run wasm    # ядро; нужен Rust и wasm-bindgen-cli 0.2.129
npm run dev     # или npm run build && npm run preview
```

Десктоп собирает её же: `beforeBuildCommand` в `tauri.conf.json`. Рабочая сборка, та что в образе, — `npm run build:server`: `dist-server`, без стенда ядра и без автономного режима, это проверяет `scripts/check-dist.mjs`.

Интерфейс в разработке против сервера: `npm run dev:server` (API проксируется на `FERRUM_API`, по умолчанию http://localhost:8090), а `PUBLIC_URL` сервера — адрес vite.

## Проверки

Из корня репозитория, зависимости — `npm ci`, `apps/server`: `bun install`.

```bash
npm run typecheck           # apps/web (три tsconfig), apps/server, packages/protocol
npm run lint                # oxlint
npm run format:check        # prettier
npm run test:server         # bun test против Postgres, Valkey и Mailpit, стенд поднимается сам
npm run test:stand:down     # снять стенд тестов
```

В `apps/web`:

```bash
npm run unit                # модульные
npm run build && npm run e2e:ci   # E2E автономной сборки
npm run e2e:server          # проект `server`: вход на настоящем стенде, Chromium и Firefox
```

Стенд тестов — свой проект compose `ferrum-test` с портами 18090 (сервер), 18025 и 11025 (Mailpit), 15432 (Postgres), 16379 (Valkey). `FERRUM_KEEP_STAND=1` оставляет его после E2E.
