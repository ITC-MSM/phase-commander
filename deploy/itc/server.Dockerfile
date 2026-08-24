FROM rust:slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends musl-tools ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY . .
RUN rustup target add x86_64-unknown-linux-musl \
    && cargo build -p phase-server --profile server-release --bin phase-server \
       --target x86_64-unknown-linux-musl

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl gosu \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system phase \
    && useradd --system --gid phase --home-dir /var/lib/phase-server --shell /usr/sbin/nologin phase
COPY --from=build /app/target/x86_64-unknown-linux-musl/server-release/phase-server /usr/local/bin/phase-server
COPY client/public/card-data.json client/public/draft-pools.json /opt/phase-seed/
COPY deploy/itc/server-entrypoint.sh /usr/local/bin/phase-server-entrypoint
RUN mkdir -p /var/lib/phase-server \
    && chown -R phase:phase /var/lib/phase-server \
    && chmod +x /usr/local/bin/phase-server /usr/local/bin/phase-server-entrypoint
ENV PORT=9374 PHASE_DATA_DIR=/var/lib/phase-server RUST_LOG=info
EXPOSE 9374
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
    CMD curl -fsS http://127.0.0.1:9374/health >/dev/null || exit 1
ENTRYPOINT ["phase-server-entrypoint"]

