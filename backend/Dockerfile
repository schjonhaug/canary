# Build stage for Rust
FROM rust:1.97.1-slim-bookworm AS rust-builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src
COPY migrations ./migrations
COPY locales ./locales

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Copy the Rust binary from builder stage
COPY --from=rust-builder /app/target/release/canary /app/canary

# Copy migrations with correct ownership for non-root user
COPY --from=rust-builder --chown=1000:1000 /app/migrations /app/migrations

# Create data directory with correct ownership for non-root user
RUN mkdir -p /app/data && chown 1000:1000 /app/data

# Expose port
EXPOSE 3000

# Run the application
CMD ["./canary"]
