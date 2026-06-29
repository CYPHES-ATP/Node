FROM rust:1-bookworm AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates \
  && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/cyphes-source-gateway /usr/local/bin/cyphes-source-gateway
ENV CYPHES_SOURCE_GATEWAY_BIND=0.0.0.0:8080
EXPOSE 8080
CMD ["cyphes-source-gateway"]

