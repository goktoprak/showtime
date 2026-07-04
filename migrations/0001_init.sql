-- settings: single row table holding app config (TMDB API key)
CREATE TABLE IF NOT EXISTS settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    tmdb_api_key TEXT
);
INSERT OR IGNORE INTO settings (id, tmdb_api_key) VALUES (1, NULL);

-- shows: one row per tracked TV show
CREATE TABLE IF NOT EXISTS shows (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    tmdb_id INTEGER NOT NULL UNIQUE,
    name TEXT NOT NULL,
    overview TEXT,
    poster_path TEXT,
    backdrop_path TEXT,
    tmdb_status TEXT,           -- raw TMDB status: "Returning Series", "Ended", "Canceled", etc.
    status TEXT NOT NULL DEFAULT 'watchlist',  -- 'watchlist' | 'watching' | 'ongoing' | 'finished'
    added_at TEXT NOT NULL DEFAULT (datetime('now')),
    last_refreshed_at TEXT
);

-- seasons: one row per season of a show
CREATE TABLE IF NOT EXISTS seasons (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    show_id INTEGER NOT NULL REFERENCES shows(id) ON DELETE CASCADE,
    tmdb_season_number INTEGER NOT NULL,
    name TEXT,
    episode_count INTEGER NOT NULL DEFAULT 0,
    UNIQUE(show_id, tmdb_season_number)
);

-- episodes: one row per episode of a season
CREATE TABLE IF NOT EXISTS episodes (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    season_id INTEGER NOT NULL REFERENCES seasons(id) ON DELETE CASCADE,
    tmdb_episode_number INTEGER NOT NULL,
    name TEXT,
    air_date TEXT,
    watched INTEGER NOT NULL DEFAULT 0,
    watched_at TEXT,
    UNIQUE(season_id, tmdb_episode_number)
);

CREATE INDEX IF NOT EXISTS idx_seasons_show_id ON seasons(show_id);
CREATE INDEX IF NOT EXISTS idx_episodes_season_id ON episodes(season_id);
