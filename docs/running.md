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

На рабочем сервере: уберите `COMPOSE_PROFILES=dev`, задайте настоящий `SMTP_URL` и `MAIL_FROM`, `PUBLIC_URL` с `https://`. За обратным прокси поставьте `TRUST_PROXY=1`: иначе все клиенты считаются одним IP (предел — 20 писем в час), а при проброшенном порте Docker Desktop и OrbStack адрес клиента и так подменяют адресом шлюза. `ALLOWED_EMAILS` ограничивает, кто может войти. Письмо уходит без ожидания ответа SMTP: сбой отправки виден только в журнале сервера.

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
