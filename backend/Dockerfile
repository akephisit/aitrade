# ── Backend Dockerfile (Multi-stage) ──────────────────────────────────────────
# Stage 1: Build
FROM rust:1.82-slim AS builder

WORKDIR /app

# Install dependencies for OpenSSL/TLS
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main(){}' > src/main.rs
RUN cargo build --release && rm -rf src

# Copy actual source and build
COPY . .
RUN touch src/main.rs && cargo build --release

# Stage 2: Runtime (minimal image)
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/antigravity ./antigravity
COPY --from=builder /app/migrations ./migrations

EXPOSE 3000

ENV RUST_LOG=antigravity=info,tower_http=warn
ENV BIND_ADDR=0.0.0.0:3000

CMD ["./antigravity"]
