# ShowTime

A local, personal-use TV show/episode tracker. Rust throughout: an Axum +
SQLite backend and a Leptos frontend compiled to WebAssembly, with TMDB for
metadata. Nothing is sent anywhere except to `api.themoviedb.org`.

## Running it with Docker (recommended for a home server)

This is the easiest way to deploy ShowTime on a home server (Synology,
Unraid, a Raspberry Pi, a spare Linux box). The image is built by GitHub
Actions and published to GHCR on every push to `main`, so your server never
needs Rust, Cargo, or a clone of this repo — just Docker.

The image lives at `ghcr.io/goktoprak/showtime`, built by the workflow in
`.github/workflows/docker-publish.yml`. It's tagged `latest` (always the most
recent push) and `sha-<commit>` (a pinned snapshot, useful for rolling back).

**One-time setup:** the package needs to be public, or you need to log in
with a GitHub token on the server, since new GHCR packages default to
private. After the first successful workflow run, go to
`github.com/goktoprak/showtime` → **Packages** → the `showtime` package →
**Package settings** → change visibility to Public. Skip this if you'd rather
authenticate on the server (see below).

### Using Docker Compose (simplest)

On the server you only need `docker-compose.yml`, not the whole repo:

```bash
mkdir showtime && cd showtime
curl -O https://raw.githubusercontent.com/goktoprak/showtime/main/docker-compose.yml
docker compose up -d
```

This pulls the pre-built image, exposes it on port 3000, and persists the
SQLite database in a named volume (`showtime_data`) so your shows and watched
progress survive restarts and updates.

Visit `http://<your-server-ip>:3000` from any device on your network.

Stop it with `docker compose down` — this does **not** delete the volume, so
your data is safe. Only `docker compose down -v` would remove it.

Update with:
```bash
docker compose pull
docker compose up -d
```

### Using plain Docker

```bash
docker run -d \
  --name showtime \
  -p 3000:3000 \
  -v showtime_data:/data \
  --restart unless-stopped \
  ghcr.io/goktoprak/showtime:latest
```

To update: `docker pull ghcr.io/goktoprak/showtime:latest`, then
`docker stop showtime && docker rm showtime`, then re-run the above.

### If the package is private

Log in on the server once with a token that has `read:packages` scope:
```bash
docker login ghcr.io -u goktoprak
```
Docker remembers this, so later `pull`/`compose up` commands just work.

### Changing the port

Change the left side of the port mapping — `"8080:3000"` in
`docker-compose.yml`, or `-p 8080:3000` with plain `docker run`. The app
always listens on 3000 *inside* the container.

## First run

Open the app, go to **Settings**, and paste in a free TMDB API key
(https://www.themoviedb.org/settings/api). It's stored in the `settings`
table in the database.

Then use **+ Add Show** and enter a TMDB TV show ID — the number in a show's
URL on themoviedb.org, e.g. `1399` for
`themoviedb.org/tv/1399-game-of-thrones`. That pulls in the show, all
seasons, and all episodes.

## How categories work

- **Watch List** — show added, nothing marked watched yet
- **Watching** — at least one episode watched, but not all of them
- **Ongoing** — every currently-known episode watched, and TMDB reports the
  show is still airing/in production/planned
- **Finished** — every currently-known episode watched, and TMDB reports the
  show has ended or been canceled

If a show is `Ongoing` or `Finished` and you hit **Refresh Metadata** and
TMDB has added episodes since you last checked, it drops back to `Watching`
automatically, since not everything is watched anymore.

**Specials** (TMDB season 0) are excluded from these rules entirely. They're
stored, shown, and individually checkable, but whether they're watched has no
bearing on the category — a show with every regular episode watched counts as
Ongoing or Finished even with unwatched specials.

Nothing refreshes automatically. Episode and season data is only re-pulled
when you click **Refresh Metadata** on a show, use **Refresh All Shows** in
Settings, or add a show for the first time.

## Backing up your data

The database — TMDB key, shows, and all watched progress — is a single SQLite
file. Two ways to get a copy:

- **From the app:** Settings → **Download Backup**. This streams a consistent
  snapshot taken with `VACUUM INTO`, so it's safe even with uncheckpointed
  WAL writes.
- **From the volume:**
  ```bash
  docker run --rm -v showtime_data:/data -v $(pwd):/backup debian \
    cp /data/showtime.db /backup/showtime-backup.db
  ```

## Resetting data

Delete `showtime.db` (and any `showtime.db-shm` / `showtime.db-wal` files
beside it) and restart. A fresh empty database is created on boot.

---

# Development

The frontend is Rust compiled to WebAssembly, so unlike earlier versions of
this project there is now a build step. **This changes nothing for people
running the Docker image** — it's entirely a contributor concern.

## Requirements

- Rust (stable) + Cargo — https://rustup.rs
- The wasm target: `rustup target add wasm32-unknown-unknown`
- [Trunk](https://trunkrs.dev), which bundles the frontend

Install Trunk from its **prebuilt binary**, not from source:

```bash
# macOS arm64; substitute your platform's asset name
curl -sSL https://github.com/trunk-rs/trunk/releases/download/v0.21.14/trunk-aarch64-apple-darwin.tar.gz \
  | tar -xzf - -C ~/.cargo/bin trunk
```

`cargo install trunk` currently fails on recent toolchains — its
`lightningcss` dependency doesn't compile. The Dockerfile downloads the
prebuilt binary for the same reason.

No Node required.

## Running it locally

Two processes. Trunk serves the UI with rebuild-on-save and proxies `/api` to
the backend, which keeps the browser same-origin so no CORS layer is needed:

```bash
# terminal 1 — API on :3000
cargo run -p showtime

# terminal 2 — UI on :8080, proxying /api to :3000
cd web && trunk serve
```

Then open http://localhost:8080.

To run the way production does — one process, backend serving the built
bundle:

```bash
cd web && trunk build --release && cd ..
cargo run -p showtime --release
```

Then open http://localhost:3000. Run this from the repository root: the
server resolves `dist/` relative to the working directory.

The database is created at `./showtime.db` on first run, unless
`SHOWTIME_DB` says otherwise. `SHOWTIME_BIND` overrides the listen address
(default `0.0.0.0:3000`).

## Tests

```bash
cargo test
```

This covers the category rules in `server/src/status.rs` against an
in-memory SQLite database, plus the pure helpers in `shared`.

Use plain `cargo test`, not `cargo test --workspace`. The `--workspace` flag
overrides `default-members` and compiles `web` for the host target too — it
succeeds, but produces a binary that can't run and roughly doubles the build
for no benefit.

## Building the crates directly

`web` is a workspace member but **not** a default member, so a bare
`cargo build` or `cargo test` at the root skips it. That's a deliberate
convenience rather than a hard constraint: it *can* be compiled for the host,
since web-sys and wasm-bindgen build natively, but the binary it produces has
no DOM to mount into. To build it for real:

```bash
cargo build -p web --target wasm32-unknown-unknown
```

## Project layout

```
showtime/
├── Cargo.toml                  -- workspace; resolver 2, default-members
├── Dockerfile                  -- cargo-chef; parallel native + wasm builders
├── docker-compose.yml
├── migrations/
│   └── 0001_init.sql           -- SQLite schema
├── shared/                     -- types on the wire, used by both sides
│   └── src/lib.rs
├── server/
│   └── src/
│       ├── main.rs             -- routes, static serving, cache headers
│       ├── db.rs               -- pool + migrations
│       ├── tmdb.rs             -- TMDB client
│       ├── status.rs           -- category rules (+ tests)
│       └── handlers.rs         -- HTTP handlers
└── web/
    ├── Trunk.toml              -- build config + dev proxy
    ├── index.html              -- Trunk entrypoint
    ├── styles/style.css
    └── src/
        ├── main.rs             -- router
        ├── api.rs              -- fetch wrapper
        ├── images.rs           -- TMDB image URLs
        ├── components/         -- Topbar, ErrorMsg
        └── pages/              -- dashboard, show, add, settings, 404
```

## Notes on the design

**Types are shared, not duplicated.** `shared` holds every type that crosses
the wire. Its `sqlx::FromRow` derives are gated behind an `ssr` feature that
only the server enables, since sqlx doesn't build for wasm. Renaming a field
is a compile error in the UI rather than a silent `undefined` in the browser.

**The client never computes a show's category.** Every mutation endpoint
returns the recomputed show row. The rules exclude specials and depend on the
raw TMDB status, so mirroring them in the frontend would mean two places to
get them wrong.

**Mutations are optimistic with rollback.** Ticking an episode updates local
state immediately and reverts the whole detail if the request fails.
Refreshing metadata is deliberately *not* optimistic — it can add entire
seasons, so that data has to come from the server.

**Caching is split.** Everything Trunk emits except `index.html` is
content-hashed, so it's served `immutable` with a one-year max-age.
`index.html` and the whole API are `no-store`.
