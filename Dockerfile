# Образ сервера, три стадии: ядро (Rust и wasm), интерфейс (apps/web), сервер (Bun).

FROM rust:1.96-slim AS wasm
RUN apt-get update && apt-get install -y --no-install-recommends nodejs && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY rust-toolchain.toml Cargo.toml Cargo.lock ./
COPY .cargo .cargo
COPY scripts scripts
COPY crates crates
COPY apps/web/scripts/build-wasm.mjs apps/web/scripts/build-wasm.mjs
RUN cargo install wasm-bindgen-cli --version 0.2.129 --locked
RUN node apps/web/scripts/build-wasm.mjs

FROM node:24-slim AS web
WORKDIR /src
COPY packages/protocol packages/protocol
COPY apps/web apps/web
COPY --from=wasm /src/apps/web/src/wasm apps/web/src/wasm
WORKDIR /src/apps/web
RUN npm ci && npm run build:server

FROM oven/bun:1.4.2-alpine
WORKDIR /app
COPY packages/protocol packages/protocol
COPY apps/server/package.json apps/server/bun.lock apps/server/bunfig.toml apps/server/
COPY apps/server/src apps/server/src
COPY apps/server/migrations apps/server/migrations
WORKDIR /app/apps/server
RUN bun install --frozen-lockfile --production
COPY --from=web /src/apps/web/dist-server /app/web
RUN mkdir -p /data && chown bun:bun /data
USER bun
ENV FERRUM_PORT=8090 FERRUM_DATA_DIR=/data FERRUM_WEB_DIST=/app/web
EXPOSE 8090
VOLUME /data
CMD ["bun", "run", "src/index.ts"]
