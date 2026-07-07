# ShowTime

A local, personal-use TV show/episode tracker. Rust backend (Axum + SQLite),
plain HTML/CSS/JS frontend, TMDB for metadata.

## Requirements

- Rust (stable) + Cargo — install via https://rustup.rs if you don't have it
- A free TMDB API key — https://www.themoviedb.org/settings/api

No Node, no build step for the frontend — it's static files served directly
by the Rust binary.

## Running it

```bash
cd showtime
cargo run
```

First run will:
- Create `showtime.db` in the current directory (SQLite file)
- Run migrations automatically to set up the schema
- Start the server at http://127.0.0.1:3000

Open that URL in your browser. Go to **Settings** first and paste in your
TMDB API key — it's stored in the `settings` table in `showtime.db`. Nothing
is sent anywhere except to `api.themoviedb.org`.

Then use **+ Add Show**, enter a TMDB TV show ID (the number in a show's URL
on themoviedb.org, e.g. `1399` for Game of Thrones), and it'll pull in the
show, all seasons, and all episodes.

## Running it with Docker (recommended for a home server)

This is the easiest way to deploy ShowTime on a home server (Synology,
Unraid, a Raspberry Pi, a spare Linux box, etc.) — the image is built
automatically by GitHub Actions and published to GHCR (GitHub Container
Registry) on every push to `main`. Your server never needs Rust, Cargo, or
even a clone of this repo — just Docker, pulling a ready-made image.

The image lives at `ghcr.io/goktoprak/showtime`, built by the workflow in
`.github/workflows/docker-publish.yml`. It's tagged `latest` (always tracks
the most recent push) and also `sha-<commit>` (a pinned snapshot of a
specific commit, useful for rolling back).

**One-time setup:** the package needs to be public (or you need to log in
with a GitHub token on the server) the first time, since new GHCR packages
default to private. After the first successful workflow run, go to
`github.com/goktoprak/showtime` → **Packages** (right sidebar) → click the
`showtime` package → **Package settings** → change visibility to Public.
Skip this if you're fine authenticating `docker login ghcr.io` on the
server instead (see below).

### Using Docker Compose (simplest)

On the server, you only need the `docker-compose.yml` file — not the whole
repo:

```bash
mkdir showtime && cd showtime
curl -O https://raw.githubusercontent.com/goktoprak/showtime/main/docker-compose.yml
docker compose up -d
```

This pulls the pre-built image and starts the container in the background,
exposing it on port 3000 and persisting the SQLite database in a named
Docker volume (`showtime_data`) so your shows and watched progress survive
container restarts and updates.

Visit `http://<your-server-ip>:3000` from any device on your network.

To stop it:
```bash
docker compose down
```
(this does **not** delete the volume, so your data is safe — only
`docker compose down -v` would remove the volume too)

To update to the latest pushed image:
```bash
docker compose pull
docker compose up -d
```

### Using plain Docker (no Compose, no files needed at all)

```bash
docker run -d \
  --name showtime \
  -p 3000:3000 \
  -v showtime_data:/data \
  --restart unless-stopped \
  ghcr.io/goktoprak/showtime:latest
```

To update:
```bash
docker pull ghcr.io/goktoprak/showtime:latest
docker stop showtime && docker rm showtime
# then re-run the docker run command above
```

### If the package is private instead of public

Log in on the server once with a GitHub Personal Access Token that has
`read:packages` scope:
```bash
docker login ghcr.io -u goktoprak
# paste the token when prompted for a password
```
Docker remembers this login, so subsequent `pull`/`compose up` commands
work without repeating it.

### Changing the port

If 3000 is already used by something else on your server, change the left
side of the port mapping — e.g. in `docker-compose.yml` change
`"3000:3000"` to `"8080:3000"`, or with plain `docker run` change
`-p 3000:3000` to `-p 8080:3000`. The app always listens on 3000 *inside*
the container; only the host-side port changes.

### Backing up your data

The SQLite database (including your TMDB API key and all show/episode
progress) lives entirely inside the `showtime_data` Docker volume. To back
it up:
```bash
docker run --rm -v showtime_data:/data -v $(pwd):/backup debian \
  cp /data/showtime.db /backup/showtime-backup.db
```

## How categories work

- **Watch List** — show added, nothing marked watched yet
- **Watching** — at least one episode watched, but not all of them
- **Ongoing** — every currently-known episode watched, and TMDB reports
  the show is still airing/in production/planned
- **Finished** — every currently-known episode watched, and TMDB reports the
  show has ended or been canceled

If a show is `Ongoing` or `Finished` and you hit **Refresh Metadata** and
TMDB has added new episodes/seasons since you last checked, it automatically
drops back to `Watching` (since not everything is watched anymore).

Nothing refreshes automatically — episode/season data is only ever re-pulled
from TMDB when you click **Refresh Metadata** on a show's page, or when a
show is first added.

## Project layout

```
showtime/
├── Cargo.toml
├── Dockerfile
├── docker-compose.yml
├── .dockerignore
├── .github/
│   └── workflows/
│       └── docker-publish.yml  -- builds & pushes image to GHCR on push
├── migrations/
│   └── 0001_init.sql       -- SQLite schema
├── src/
│   ├── main.rs              -- server setup, routes
│   ├── models.rs             -- DB row structs / request-response types
│   ├── db.rs                  -- connection pool + migrations
│   ├── tmdb.rs                 -- TMDB API client
│   ├── status.rs                -- category recompute logic
│   └── handlers.rs               -- all HTTP handlers
└── static/
    ├── index.html            -- dashboard (4 category grids)
    ├── add.html                -- add-show-by-id form
    ├── show.html                 -- show detail: banner, seasons, episodes
    ├── settings.html               -- API key form
    ├── style.css
    └── app.js
```

## Notes / things to double check on first run

I wrote this without a local Rust toolchain to compile against, so while
I've reviewed it carefully, there's a nonzero chance of a small compile
error on first `cargo build`. If you hit one:

- Most likely spot: a `sqlx` query macro/type mismatch — all queries here
  use runtime-checked `sqlx::query`/`query_as`/`query_scalar` (not the
  compile-time `query!` macros), specifically so this doesn't require a
  live DB connection at compile time.
- `libsqlite3-sys` is pinned to `bundled` in Cargo.toml so it compiles its
  own recent SQLite rather than relying on your system's, since the schema
  uses `RETURNING` (needs SQLite 3.35+).
- If you get a port-in-use error, something else is already on 3000 —
  change the bind address in `src/main.rs` (`main()`, near the bottom).

Feel free to paste any compiler error back to me and I'll fix it directly.

## Resetting data

Just delete `showtime.db` (and any `showtime.db-shm` / `showtime.db-wal`
files next to it) and restart — a fresh empty DB will be created.
