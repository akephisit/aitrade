# ── OpenClaw Dockerfile (Multi-stage) ─────────────────────────────────────────
FROM rust:1.82-slim AS builder

WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main(){}' > src/main.rs
RUN cargo build --release && rm -rf src

COPY . .
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim AS runtime
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/openclaw ./openclaw

ENV RUST_LOG=openclaw=info
ENV AITRADE_URL=http://backend:3000
ENV BRAIN_INTERVAL_SECS=300

CMD ["./openclaw"]
