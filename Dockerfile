# Two independent builders - the native server binary and the wasm frontend -
# feeding one slim runtime image. Neither builder depends on the other, only
# on `planner`, so buildx runs them concurrently.

# ---- Chef base --------------------------------------------------------------
# cargo-chef caches the dependency build separately from the source. The old
# single-crate trick of building a dummy main.rs stopped working once this
# became a workspace, which would otherwise need a hand-maintained dummy
# entrypoint per member.
FROM rust:1-bookworm AS chef
WORKDIR /app
# --locked pins cargo-chef's own tested dependency versions. Without it cargo
# resolves the newest semver-compatible ones instead, which is exactly how
# `cargo install trunk` breaks on current toolchains.
RUN cargo install cargo-chef --locked

# ---- Dependency recipe ------------------------------------------------------
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# ---- Server (native target) -------------------------------------------------
FROM chef AS server-builder
COPY --from=planner /app/recipe.json recipe.json
# Scoped to the server package so this stage never tries to build the wasm
# crate's dependency tree.
RUN cargo chef cook --release --package showtime --recipe-path recipe.json
COPY . .
RUN cargo build --release --package showtime

# ---- Frontend (wasm target) -------------------------------------------------
FROM chef AS wasm-builder
ARG TRUNK_VERSION=0.21.14
RUN rustup target add wasm32-unknown-unknown
# The prebuilt binary is not merely faster than `cargo install trunk`: trunk
# does not currently build from source on recent toolchains at all, because
# its lightningcss dependency fails to compile.
RUN set -eux; \
    case "$(uname -m)" in \
      x86_64)  arch=x86_64-unknown-linux-gnu ;; \
      aarch64) arch=aarch64-unknown-linux-gnu ;; \
      *) echo "unsupported architecture: $(uname -m)" >&2; exit 1 ;; \
    esac; \
    curl -sSfL "https://github.com/trunk-rs/trunk/releases/download/v${TRUNK_VERSION}/trunk-${arch}.tar.gz" \
      | tar -xzf - -C /usr/local/bin trunk; \
    trunk --version
COPY --from=planner /app/recipe.json recipe.json
# Cooked separately from the server stage and scoped to `web`: the workspace's
# native dependencies (sqlx, tokio, reqwest) do not build for wasm32.
RUN cargo chef cook --release --target wasm32-unknown-unknown --package web --recipe-path recipe.json
COPY . .
# trunk fetches a wasm-bindgen CLI matching the wasm-bindgen crate version on
# first run, so this step needs network access even though the crates
# themselves are already present.
RUN cd web && trunk build --release

# ---- Runtime ----------------------------------------------------------------
FROM debian:bookworm-slim

# ca-certificates is required for reqwest to make HTTPS calls to TMDB.
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=server-builder /app/target/release/showtime ./showtime
# Trunk writes to the repo root rather than web/dist, so the server's ServeDir
# and this COPY both refer to a single top-level dist/.
COPY --from=wasm-builder /app/dist ./dist

# Persistent data (SQLite DB) lives here - mount a volume at this path.
RUN mkdir -p /data
ENV SHOWTIME_DB=/data/showtime.db
ENV SHOWTIME_BIND=0.0.0.0:3000

EXPOSE 3000

CMD ["./showtime"]
