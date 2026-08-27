//! The show detail page's state.
//!
//! Two pieces, deliberately: `DetailIndex` is a plain value with no signals
//! and no DOM, so the lookup and progress logic can be unit-tested natively;
//! `DetailStore` wraps it reactively and owns the optimistic mutations.
//!
//! Children are handed the store and ask it for exactly what they render.
//! They never learn the shape of the season/episode tree, and they never see
//! an `Option` - the page has already established there is a show by the time
//! a store exists.

use std::collections::HashMap;

use leptos::prelude::*;
use shared::{Episode, SeasonWithEpisodes, Show, ShowDetail};

use crate::api;
use crate::components::Message;

/// An O(1) view over a fetched show. Rebuilding the maps costs one pass over
/// the episodes; without them every episode row rescanned the whole show.
#[derive(Clone, PartialEq)]
pub struct DetailIndex {
    detail: ShowDetail,
    /// season id -> position in `detail.seasons`
    seasons: HashMap<i64, usize>,
    /// episode id -> (season position, episode position)
    episodes: HashMap<i64, (usize, usize)>,
}

impl DetailIndex {
    pub fn new(detail: ShowDetail) -> Self {
        let mut seasons = HashMap::new();
        let mut episodes = HashMap::new();
        for (si, season) in detail.seasons.iter().enumerate() {
            seasons.insert(season.season.id, si);
            for (ei, episode) in season.episodes.iter().enumerate() {
                episodes.insert(episode.id, (si, ei));
            }
        }
        Self {
            detail,
            seasons,
            episodes,
        }
    }

    pub fn show(&self) -> &Show {
        &self.detail.show
    }

    pub fn season_ids(&self) -> Vec<i64> {
        self.detail.seasons.iter().map(|s| s.season.id).collect()
    }

    pub fn season(&self, season_id: i64) -> Option<&SeasonWithEpisodes> {
        self.seasons
            .get(&season_id)
            .and_then(|&i| self.detail.seasons.get(i))
    }

    pub fn episode_ids(&self, season_id: i64) -> Vec<i64> {
        self.season(season_id)
            .map(|s| s.episodes.iter().map(|e| e.id).collect())
            .unwrap_or_default()
    }

    pub fn episode(&self, episode_id: i64) -> Option<&Episode> {
        let &(si, ei) = self.episodes.get(&episode_id)?;
        self.detail.seasons.get(si)?.episodes.get(ei)
    }

    /// (watched, total) for one season.
    pub fn season_progress(&self, season_id: i64) -> (usize, usize) {
        self.season(season_id)
            .map(|s| {
                (
                    s.episodes.iter().filter(|e| e.watched).count(),
                    s.episodes.len(),
                )
            })
            .unwrap_or((0, 0))
    }

    /// TMDB doesn't always name a season; fall back to its number.
    pub fn season_name(&self, season_id: i64) -> String {
        self.season(season_id)
            .map(|s| {
                s.season
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("Season {}", s.season.tmdb_season_number))
            })
            .unwrap_or_default()
    }

    fn toggle_episode(&mut self, episode_id: i64) {
        if let Some(&(si, ei)) = self.episodes.get(&episode_id) {
            if let Some(e) = self
                .detail
                .seasons
                .get_mut(si)
                .and_then(|s| s.episodes.get_mut(ei))
            {
                e.watched = !e.watched;
            }
        }
    }

    fn mark_season(&mut self, season_id: i64) {
        if let Some(&si) = self.seasons.get(&season_id) {
            if let Some(season) = self.detail.seasons.get_mut(si) {
                for episode in &mut season.episodes {
                    episode.watched = true;
                }
            }
        }
    }

    fn mark_all(&mut self) {
        for season in &mut self.detail.seasons {
            for episode in &mut season.episodes {
                episode.watched = true;
            }
        }
    }

    /// Takes the recomputed show row a mutation answered with. The category
    /// can only come from the server, which excludes specials from the counts
    /// and keys off the raw TMDB production status.
    fn set_show(&mut self, show: Show) {
        self.detail.show = show;
    }
}

/// The reactive wrapper. `Copy`, so it passes to children without ceremony.
#[derive(Clone, Copy)]
pub struct DetailStore {
    index: RwSignal<DetailIndex>,
    error: RwSignal<Option<String>>,
    refreshing: RwSignal<bool>,
}

impl DetailStore {
    pub fn new(detail: ShowDetail) -> Self {
        Self {
            index: RwSignal::new(DetailIndex::new(detail)),
            error: RwSignal::new(None),
            refreshing: RwSignal::new(false),
        }
    }

    /// Swaps in a freshly fetched show, keeping the store (and therefore the
    /// open/closed state of the season panels) alive across a refresh.
    pub fn replace(&self, detail: ShowDetail) {
        self.index.set(DetailIndex::new(detail));
        self.error.set(None);
    }

    /// Read some slice of the show. Every child accessor goes through here.
    pub fn with<T>(&self, f: impl FnOnce(&DetailIndex) -> T) -> T {
        self.index.with(f)
    }

    pub fn show_id(&self) -> i64 {
        self.index.with_untracked(|i| i.show().id)
    }

    /// Errors are always failures here, so the tone is fixed.
    pub fn error(&self) -> Signal<Option<Message>> {
        let error = self.error;
        Signal::derive(move || error.get().map(Message::error))
    }

    pub fn refreshing(&self) -> Signal<bool> {
        self.refreshing.into()
    }

    /// Applies a change locally, fires the request, and rolls the whole index
    /// back if it fails. The response carries the recomputed show row.
    fn mutate(&self, apply: impl FnOnce(&mut DetailIndex) + 'static, path: String) {
        let index = self.index;
        let error = self.error;
        let snapshot = index.get_untracked();

        index.update(apply);
        error.set(None);

        leptos::task::spawn_local(async move {
            match api::post::<Show>(&path).await {
                Ok(show) => index.update(|i| i.set_show(show)),
                Err(e) => {
                    index.set(snapshot);
                    error.set(Some(e.to_string()));
                }
            }
        });
    }

    pub fn toggle_episode(&self, episode_id: i64) {
        self.mutate(
            move |i| i.toggle_episode(episode_id),
            format!("/episodes/{episode_id}/toggle"),
        );
    }

    pub fn mark_season(&self, season_id: i64) {
        self.mutate(
            move |i| i.mark_season(season_id),
            format!("/seasons/{season_id}/mark-watched"),
        );
    }

    pub fn mark_all(&self) {
        self.mutate(
            |i| i.mark_all(),
            format!("/shows/{}/mark-watched", self.show_id()),
        );
    }

    /// Not optimistic: a refresh can add whole seasons, so the detail has to
    /// come back from the server.
    pub fn refresh(&self) {
        let store = *self;
        let id = self.show_id();
        self.refreshing.set(true);
        self.error.set(None);
        leptos::task::spawn_local(async move {
            match api::post::<Show>(&format!("/shows/{id}/refresh")).await {
                Ok(_) => match api::get::<ShowDetail>(&format!("/shows/{id}")).await {
                    Ok(detail) => store.replace(detail),
                    Err(e) => store.error.set(Some(format!("Failed to reload: {e}"))),
                },
                Err(e) => store.error.set(Some(format!("Refresh failed: {e}"))),
            }
            store.refreshing.set(false);
        });
    }

    pub fn delete(&self, on_deleted: impl Fn() + 'static) {
        let error = self.error;
        let id = self.show_id();
        leptos::task::spawn_local(async move {
            match api::delete(&format!("/shows/{id}")).await {
                Ok(()) => on_deleted(),
                Err(e) => error.set(Some(format!("Failed to delete: {e}"))),
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::{Season, ShowCategory};

    fn episode(id: i64, number: i64, watched: bool) -> Episode {
        Episode {
            id,
            season_id: number,
            tmdb_episode_number: number,
            name: Some(format!("E{number}")),
            air_date: None,
            watched,
            watched_at: None,
        }
    }

    fn detail(seasons: Vec<(i64, Option<&str>, Vec<Episode>)>) -> ShowDetail {
        ShowDetail {
            show: Show {
                id: 1,
                tmdb_id: 1,
                name: "Test".into(),
                overview: None,
                poster_path: None,
                backdrop_path: None,
                tmdb_status: None,
                category: ShowCategory::Watching,
                added_at: "now".into(),
                last_refreshed_at: None,
            },
            seasons: seasons
                .into_iter()
                .enumerate()
                .map(|(i, (id, name, episodes))| SeasonWithEpisodes {
                    season: Season {
                        id,
                        show_id: 1,
                        tmdb_season_number: i as i64 + 1,
                        name: name.map(str::to_string),
                        episode_count: episodes.len() as i64,
                    },
                    episodes,
                })
                .collect(),
        }
    }

    fn sample() -> DetailIndex {
        DetailIndex::new(detail(vec![
            (
                10,
                Some("One"),
                vec![episode(100, 1, true), episode(101, 2, false)],
            ),
            (20, None, vec![episode(200, 1, false)]),
        ]))
    }

    #[test]
    fn an_episode_is_found_by_id_wherever_it_sits() {
        let index = sample();
        assert_eq!(index.episode(100).unwrap().tmdb_episode_number, 1);
        // Second season: the lookup must not be scoped to the first.
        assert_eq!(index.episode(200).unwrap().tmdb_episode_number, 1);
        assert!(index.episode(999).is_none());
    }

    #[test]
    fn season_progress_counts_only_that_season() {
        let index = sample();
        assert_eq!(index.season_progress(10), (1, 2));
        assert_eq!(index.season_progress(20), (0, 1));
        assert_eq!(index.season_progress(999), (0, 0));
    }

    #[test]
    fn an_unnamed_season_falls_back_to_its_number() {
        let index = sample();
        assert_eq!(index.season_name(10), "One");
        assert_eq!(index.season_name(20), "Season 2");
    }

    #[test]
    fn ids_come_back_in_order() {
        let index = sample();
        assert_eq!(index.season_ids(), vec![10, 20]);
        assert_eq!(index.episode_ids(10), vec![100, 101]);
        assert_eq!(index.episode_ids(999), Vec::<i64>::new());
    }

    #[test]
    fn toggling_flips_one_episode_and_leaves_the_rest() {
        let mut index = sample();
        index.toggle_episode(101);
        assert!(index.episode(101).unwrap().watched);
        assert!(index.episode(100).unwrap().watched);
        assert!(!index.episode(200).unwrap().watched);

        index.toggle_episode(101);
        assert!(!index.episode(101).unwrap().watched);
    }

    #[test]
    fn marking_a_season_leaves_other_seasons_alone() {
        let mut index = sample();
        index.mark_season(10);
        assert_eq!(index.season_progress(10), (2, 2));
        assert_eq!(index.season_progress(20), (0, 1));
    }

    #[test]
    fn marking_everything_covers_every_season() {
        let mut index = sample();
        index.mark_all();
        assert_eq!(index.season_progress(10), (2, 2));
        assert_eq!(index.season_progress(20), (1, 1));
    }

    #[test]
    fn a_show_with_no_seasons_is_navigable_not_a_panic() {
        let index = DetailIndex::new(detail(vec![]));
        assert!(index.season_ids().is_empty());
        assert!(index.episode(1).is_none());
        assert_eq!(index.season_progress(1), (0, 0));
        assert_eq!(index.season_name(1), "");
    }
}
