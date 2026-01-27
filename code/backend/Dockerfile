# Build stage
FROM rust:latest AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy main to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copy source code
COPY src ./src

# Build the actual application
RUN touch src/main.rs && cargo build --release

# Runtime stage - use same base as builder for glibc compatibility
FROM debian:trixie-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3t64 \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary
COPY --from=builder /app/target/release/kubarr /app/kubarr

# Create data directory
RUN mkdir -p /data

ENV KUBARR_DATABASE_URL=sqlite:///data/kubarr.db
ENV KUBARR_HOST=0.0.0.0
ENV KUBARR_PORT=8080
ENV KUBARR_LOG_LEVEL=info
ENV KUBARR_OAUTH2_ISSUER_URL=http://localhost:8080

EXPOSE 8080

CMD ["/app/kubarr"]
