//! Types shared between the server and the web frontend.
//!
//! Every type here is both `Serialize` and `Deserialize`: the server writes
//! them onto the wire and the frontend reads them back off it, so both
//! directions are needed on both sides. The `sqlx` derives are the
//! exception - they're gated behind the `ssr` feature, since sqlx doesn't
//! build for `wasm32-unknown-unknown`.

use serde::{Deserialize, Serialize};

/// Which shelf a Show currently sits on. Always derived from watched
/// progress and the show's production status, never chosen by the user -
/// see `docs/adr/0001`.
///
/// Serialises and stores as the same lowercase strings this was before it
/// was a type (`"watchlist"`, `"watching"`, ...), so neither the wire format
/// nor the CSS class names changed when it stopped being a `String`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ssr", derive(sqlx::Type), sqlx(rename_all = "lowercase"))]
#[serde(rename_all = "lowercase")]
pub enum ShowCategory {
    Watchlist,
    Watching,
    Ongoing,
    Finished,
}

impl ShowCategory {
    /// Every category, in the order the dashboard shows its tabs.
    pub const ALL: [ShowCategory; 4] = [
        ShowCategory::Watching,
        ShowCategory::Ongoing,
        ShowCategory::Watchlist,
        ShowCategory::Finished,
    ];

    /// The stored and serialised form, which doubles as the CSS modifier
    /// on `.status-badge` and `.tab-btn`.
    pub fn as_str(self) -> &'static str {
        match self {
            ShowCategory::Watchlist => "watchlist",
            ShowCategory::Watching => "watching",
            ShowCategory::Ongoing => "ongoing",
            ShowCategory::Finished => "finished",
        }
    }

    /// How the category is written in the UI.
    pub fn label(self) -> &'static str {
        match self {
            ShowCategory::Watchlist => "Watch List",
            ShowCategory::Watching => "Watching",
            ShowCategory::Ongoing => "Ongoing",
            ShowCategory::Finished => "Finished",
        }
    }

    /// Parses the stored form. Used for the remembered dashboard tab, which
    /// comes back out of local storage as an arbitrary string.
    pub fn parse(s: &str) -> Option<ShowCategory> {
        ShowCategory::ALL.into_iter().find(|c| c.as_str() == s)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct Show {
    pub id: i64,
    pub tmdb_id: i64,
    pub name: String,
    pub overview: Option<String>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    /// TMDB's own production status: "Returning Series", "Ended", and so on.
    /// Mirrored verbatim; only interpreted to split Ongoing from Finished.
    pub tmdb_status: Option<String>,
    pub category: ShowCategory,
    pub added_at: String,
    pub last_refreshed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct Season {
    pub id: i64,
    pub show_id: i64,
    pub tmdb_season_number: i64,
    pub name: Option<String>,
    pub episode_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ssr", derive(sqlx::FromRow))]
pub struct Episode {
    pub id: i64,
    pub season_id: i64,
    pub tmdb_episode_number: i64,
    pub name: Option<String>,
    pub air_date: Option<String>,
    pub watched: bool,
    pub watched_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeasonWithEpisodes {
    pub season: Season,
    pub episodes: Vec<Episode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShowDetail {
    #[serde(flatten)]
    pub show: Show,
    pub seasons: Vec<SeasonWithEpisodes>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddShowRequest {
    pub tmdb_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetApiKeyRequest {
    pub api_key: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingsResponse {
    pub has_api_key: bool,
    /// A masked preview of the stored key (e.g. "************af92"),
    /// safe to show in the UI. The real key is never sent back to the
    /// frontend after it's saved.
    pub masked_key: Option<String>,
}

/// Result of refreshing every tracked show in one go.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RefreshAllResponse {
    pub refreshed: i64,
    pub failed: i64,
}

/// Builds a masked display version of an API key: all but the last 4
/// characters replaced with asterisks. For very short keys (<= 4 chars,
/// which shouldn't happen with real TMDB keys but just in case), masks
/// everything.
pub fn mask_api_key(key: &str) -> String {
    let len = key.chars().count();
    if len <= 4 {
        "*".repeat(len)
    } else {
        let tail: String = key.chars().skip(len - 4).collect();
        format!("{}{}", "*".repeat(len - 4), tail)
    }
}

/// Maps a raw TMDB show status string into whether the show is still
/// actively producing new content ("airing") or is done.
pub fn tmdb_status_is_airing(tmdb_status: &str) -> bool {
    matches!(
        tmdb_status,
        "Returning Series" | "In Production" | "Planned" | "Pilot"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masking_keeps_only_the_last_four_characters() {
        assert_eq!(mask_api_key("abcdef1234567890"), "************7890");
    }

    #[test]
    fn masking_hides_everything_for_short_keys() {
        assert_eq!(mask_api_key(""), "");
        assert_eq!(mask_api_key("a"), "*");
        assert_eq!(mask_api_key("abcd"), "****");
    }

    #[test]
    fn masking_reveals_the_tail_from_five_characters_up() {
        assert_eq!(mask_api_key("abcde"), "*bcde");
    }

    /// The masking arithmetic uses `chars().count()`, not byte length. A
    /// multi-byte key must not panic or slice mid-character.
    #[test]
    fn masking_counts_characters_not_bytes() {
        assert_eq!(mask_api_key("áéíóúab"), "***óúab");
        assert_eq!(mask_api_key("🔑🔑🔑🔑🔑"), "*🔑🔑🔑🔑");
        // Exactly 4 characters but 16 bytes - must mask all of it.
        assert_eq!(mask_api_key("🔑🔑🔑🔑"), "****");
    }

    #[test]
    fn airing_statuses_are_recognised() {
        for s in ["Returning Series", "In Production", "Planned", "Pilot"] {
            assert!(tmdb_status_is_airing(s), "{s} should count as airing");
        }
    }

    #[test]
    fn finished_and_unknown_statuses_are_not_airing() {
        for s in ["Ended", "Canceled", "Cancelled", "", "returning series"] {
            assert!(!tmdb_status_is_airing(s), "{s} should not count as airing");
        }
    }

    /// The stored form is load-bearing in three places: the database column,
    /// the JSON the frontend reads, and the CSS class on the badge.
    #[test]
    fn categories_keep_their_stored_spelling() {
        assert_eq!(ShowCategory::Watchlist.as_str(), "watchlist");
        assert_eq!(ShowCategory::Watching.as_str(), "watching");
        assert_eq!(ShowCategory::Ongoing.as_str(), "ongoing");
        assert_eq!(ShowCategory::Finished.as_str(), "finished");
    }

    #[test]
    fn every_category_round_trips_through_its_stored_form() {
        for c in ShowCategory::ALL {
            assert_eq!(ShowCategory::parse(c.as_str()), Some(c));
        }
    }

    #[test]
    fn an_unrecognised_category_does_not_parse() {
        assert_eq!(ShowCategory::parse(""), None);
        assert_eq!(ShowCategory::parse("Watching"), None);
        assert_eq!(ShowCategory::parse("abandoned"), None);
    }

    #[test]
    fn categories_serialise_as_the_bare_lowercase_string() {
        let json = serde_json::to_string(&ShowCategory::Ongoing).unwrap();
        assert_eq!(json, "\"ongoing\"");
        let back: ShowCategory = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ShowCategory::Ongoing);
    }

    #[test]
    fn the_tab_order_is_the_one_the_dashboard_shows() {
        let order: Vec<_> = ShowCategory::ALL.iter().map(|c| c.as_str()).collect();
        assert_eq!(order, ["watching", "ongoing", "watchlist", "finished"]);
    }
}
