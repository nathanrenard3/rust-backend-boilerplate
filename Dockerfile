FROM rust:1.98.1-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migration ./migration
RUN cargo build --workspace --release --locked

FROM debian:bookworm-slim AS runtime

COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
COPY --from=builder /app/target/release/rust-backend-boilerplate /usr/local/bin/api

COPY --from=builder /app/target/release/migration /usr/local/bin/migrate

USER 10001:10001
EXPOSE 8000
CMD ["api"]
