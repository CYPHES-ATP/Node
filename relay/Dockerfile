FROM rust:1.88-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN useradd --create-home --uid 10001 cyphes
COPY --from=builder /build/target/release/cyphes-relay /usr/local/bin/cyphes-relay
RUN mkdir -p /var/lib/cyphes-relay && chown -R cyphes:cyphes /var/lib/cyphes-relay
USER cyphes
ENV CYPHES_RELAY_DATA_DIR=/var/lib/cyphes-relay
ENV CYPHES_RELAY_PORT=4001
EXPOSE 4001/tcp
EXPOSE 4001/udp
VOLUME ["/var/lib/cyphes-relay"]
ENTRYPOINT ["cyphes-relay"]
