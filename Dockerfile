ARG RUST_VERSION=1.79.0

FROM rust:${RUST_VERSION}-slim-bookworm AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y libssl-dev pkg-config
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release && \
    cp ./target/release/epimetheus /epimetheus

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates
COPY --from=builder /epimetheus /usr/local/bin/epimetheus
ENTRYPOINT [ "epimetheus" ]
