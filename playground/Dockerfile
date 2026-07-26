# CuNi hosted playground — emit + cuni check + Notelog/Critic Book
FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock* ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates python3 golang-go nodejs \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=build /src/target/release/cuni /usr/local/bin/cuni
COPY playground ./playground
COPY examples ./examples

ENV CUNI_BIN=/usr/local/bin/cuni \
    CUNI_PLAYGROUND_HOST=0.0.0.0 \
    CUNI_PLAYGROUND_PORT=8787 \
    CUNI_PLAYGROUND_DATA=/data \
    CUNI_PLAYGROUND_TIMEOUT=45 \
    CUNI_PLAYGROUND_MAX_CONCURRENT=2 \
    PATH="/usr/local/go/bin:/usr/bin:$PATH"

VOLUME ["/data"]
EXPOSE 8787
CMD ["python3", "playground/server.py"]
