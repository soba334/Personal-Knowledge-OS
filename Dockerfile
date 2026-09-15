FROM rust:1.98.1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release --locked

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN useradd --create-home --uid 10001 pkos
COPY --from=builder /app/target/release/personal-knowledge-os /usr/local/bin/personal-knowledge-os
USER 10001
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/personal-knowledge-os"]
