# ============================================================
# Stage 1: Build
# ============================================================
FROM rust:1.75-bookworm AS builder

RUN apt-get update && \
    apt-get install -y --no-install-recommends cmake g++ && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache dependency builds
COPY Cargo.toml Cargo.lock* rust-toolchain.toml ./
COPY rememhq-core/Cargo.toml rememhq-core/Cargo.toml
COPY rememhq-mcp/Cargo.toml rememhq-mcp/Cargo.toml
COPY rememhq-api/Cargo.toml rememhq-api/Cargo.toml
COPY rememhq-cli/Cargo.toml rememhq-cli/Cargo.toml
COPY sdk/rust/Cargo.toml sdk/rust/Cargo.toml

# Create dummy src files for dependency caching
RUN mkdir -p rememhq-core/src rememhq-mcp/src rememhq-api/src rememhq-cli/src sdk/rust/src && \
    echo "pub fn main() {}" > rememhq-core/src/lib.rs && \
    echo "fn main() {}" > rememhq-mcp/src/main.rs && \
    echo "fn main() {}" > rememhq-api/src/main.rs && \
    echo "fn main() {}" > rememhq-cli/src/main.rs && \
    echo "pub use rememhq_core;" > sdk/rust/src/lib.rs

RUN cargo build --release --workspace 2>/dev/null || true

# Copy real source and build
COPY libremem/ libremem/
COPY rememhq-core/ rememhq-core/
COPY rememhq-mcp/ rememhq-mcp/
COPY rememhq-api/ rememhq-api/
COPY rememhq-cli/ rememhq-cli/
COPY sdk/rust/ sdk/rust/

# Touch source files to invalidate cache
RUN touch rememhq-core/src/lib.rs rememhq-mcp/src/main.rs rememhq-api/src/main.rs rememhq-cli/src/main.rs

RUN cargo build --release --workspace

# ============================================================
# Stage 2: Runtime
# ============================================================
FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

RUN groupadd --gid 1000 remem && \
    useradd --uid 1000 --gid remem --shell /bin/bash --create-home remem

COPY --from=builder /build/target/release/remem /usr/local/bin/remem
COPY --from=builder /build/target/release/rememhq-api /usr/local/bin/rememhq-api
COPY --from=builder /build/target/release/rememhq-mcp /usr/local/bin/rememhq-mcp

USER remem
WORKDIR /home/remem

# Default data directory
ENV REMEM_DATA_DIR=/home/remem/.remem
VOLUME ["/home/remem/.remem"]

# REST API port
EXPOSE 7474

# Default: run the REST API server
ENTRYPOINT ["remem"]
CMD ["serve", "--port", "7474"]

# ============================================================
# Labels
# ============================================================
LABEL org.opencontainers.image.source="https://github.com/remem-io/remem"
LABEL org.opencontainers.image.description="Reasoning memory layer for AI agents"
LABEL org.opencontainers.image.licenses="Apache-2.0"
LABEL org.opencontainers.image.vendor="rememhq"
