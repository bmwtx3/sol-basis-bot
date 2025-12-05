FROM rust:1.75-bookworm as builder

WORKDIR /app

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Build actual application
COPY . .
RUN touch src/main.rs && cargo build --release

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 solbot

WORKDIR /app

# Copy binary
COPY --from=builder /app/target/release/sol-basis-bot /app/sol-basis-bot

# Copy default config
COPY config.yaml /app/config.yaml

# Create directories
RUN mkdir -p /app/logs && chown -R solbot:solbot /app

USER solbot

# Expose metrics port
EXPOSE 9090

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:9090/metrics || exit 1

ENTRYPOINT ["/app/sol-basis-bot"]
CMD ["--config", "/app/config.yaml"]
