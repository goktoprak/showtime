# ---- Build stage ----
FROM rust:1-bookworm AS builder

WORKDIR /app

# Cache dependencies separately from source changes: copy manifests first,
# build a dummy main so `cargo build` compiles all deps, then copy real
# source and rebuild (only recompiles our own code on source changes).
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

COPY src ./src
COPY migrations ./migrations
# Touch main.rs so cargo knows to actually rebuild it (mtime from the dummy
# build above can otherwise be newer than the real file after COPY).
RUN touch src/main.rs
RUN cargo build --release

# ---- Runtime stage ----
FROM debian:bookworm-slim

# ca-certificates is required for reqwest to make HTTPS calls to TMDB.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/showtime ./showtime
COPY static ./static

# Persistent data (SQLite DB) lives here - mount a volume at this path.
RUN mkdir -p /data
ENV SHOWTIME_DB=/data/showtime.db
ENV SHOWTIME_BIND=0.0.0.0:3000

EXPOSE 3000

CMD ["./showtime"]
