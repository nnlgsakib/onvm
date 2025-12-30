FROM rust:1.82-bookworm AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --locked

FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/onvm /usr/local/bin/onvm
COPY docker/entrypoint.sh /usr/local/bin/onvm-entrypoint

RUN chmod +x /usr/local/bin/onvm-entrypoint

ENV ONVM_DATA_DIR=/data
VOLUME ["/data"]

EXPOSE 8080 37000

ENTRYPOINT ["/usr/local/bin/onvm-entrypoint"]
CMD ["run-node"]
