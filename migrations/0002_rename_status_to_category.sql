-- `status` meant two different things on the shows table: this column (the
-- shelf we derive) and `tmdb_status` (TMDB's production status). Renaming the
-- derived one to `category` leaves each word meaning one thing.

-- Normalise first: the column is decoded into a four-variant type from here
-- on, so any value outside the set would fail to read. Nothing this app has
-- ever written falls outside it; this only catches hand-edited databases.
UPDATE shows
SET status = 'watchlist'
WHERE status NOT IN ('watchlist', 'watching', 'ongoing', 'finished');

ALTER TABLE shows RENAME COLUMN status TO category;
