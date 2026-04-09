FROM rust:alpine AS builder
RUN apk add --no-cache musl-dev
WORKDIR /app

RUN --mount=type=bind,source=src,target=src \
    --mount=type=bind,source=.cargo,target=.cargo \
    --mount=type=bind,source=Cargo.toml,target=Cargo.toml \
    --mount=type=bind,source=Cargo.lock,target=Cargo.lock \
    --mount=type=cache,target=/app/target/ \
    --mount=type=cache,target=/usr/local/cargo/git/db \
    --mount=type=cache,target=/usr/local/cargo/registry/ \
    cargo build --locked --release && \
    cp ./target/release/honeypot-lite /bin/honeypot-lite

FROM alpine AS final
COPY --from=builder /bin/honeypot-lite .
CMD ["./honeypot-lite"]
