# Build stage for Rust
FROM rust:1.85-slim AS rust-builder

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

# Build the application
RUN cargo build --release

# Build stage for Node.js dependencies
FROM node:20-slim AS node-builder

WORKDIR /app/xpub-tools

# Copy Node.js project files
COPY xpub-tools/package.json xpub-tools/package-lock.json* ./
COPY xpub-tools/scripts ./scripts

# Install Node.js dependencies
RUN npm ci --only=production

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies including Node.js
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js 20 from NodeSource repository
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Copy the Rust binary from builder stage
COPY --from=rust-builder /app/target/release/canary /app/canary

# Copy migrations
COPY --from=rust-builder /app/migrations /app/migrations

# Copy Node.js tools and dependencies
COPY --from=node-builder /app/xpub-tools /app/xpub-tools

# Create data directory
RUN mkdir -p /app/data

# Expose port
EXPOSE 3000

# Run the application
CMD ["./canary"]