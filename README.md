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
