use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};
use shared::{Show, ShowDetail};

use crate::api;
use crate::components::{ErrorMsg, Topbar};
use crate::images::{backdrop_url, poster_url};

/// The whole page hangs off one `RwSignal<Option<ShowDetail>>`. Mutations
/// update it in place and the derived views recompute from there, rather
/// than the old approach of refetching the entire show and rebuilding the
/// DOM on every checkbox click.
type Detail = RwSignal<Option<ShowDetail>>;

#[component]
pub fn ShowPage() -> impl IntoView {
    let params = use_params_map();
    let show_id = Memo::new(move |_| params.read().get("id").and_then(|s| s.parse::<i64>().ok()));

    let detail: Detail = RwSignal::new(None);
    let error = RwSignal::new(None::<String>);
    let loaded = RwSignal::new(false);

    let reload = move || {
        let Some(id) = show_id.get_untracked() else {
            error.set(Some("Invalid show id.".to_string()));
            loaded.set(true);
            return;
        };
        leptos::task::spawn_local(async move {
            match api::get::<ShowDetail>(&format!("/shows/{id}")).await {
                Ok(d) => {
                    detail.set(Some(d));
                    error.set(None);
                }
                Err(e) => error.set(Some(format!("Failed to load show: {e}"))),
            }
            loaded.set(true);
        });
    };

    // Re-runs if the route id changes, e.g. navigating between two shows
    // without unmounting this component.
    Effect::new(move |_| {
        show_id.track();
        loaded.set(false);
        reload();
    });

    view! {
        <Topbar>
            <A href="/" attr:class="btn">"← Back"</A>
            <A href="/settings" attr:class="btn">"Settings"</A>
        </Topbar>
        <main>
            {move || {
                if !loaded.get() {
                    return view! { <div class="loading">"Loading…"</div> }.into_any();
                }
                match detail.get() {
                    None => {
                        let msg = error.get().unwrap_or_else(|| "Failed to load show.".to_string());
                        view! { <div class="empty-state">{msg}</div> }.into_any()
                    }
                    Some(_) => {
                        view! {
                            <Loaded
                                detail=detail
                                error=error
                                reload=Callback::new(move |()| reload())
                            />
                        }
                            .into_any()
                    }
                }
            }}
        </main>
    }
}

#[component]
fn Loaded(
    detail: Detail,
    error: RwSignal<Option<String>>,
    reload: Callback<()>,
) -> impl IntoView {
    let refreshing = RwSignal::new(false);
    let navigate = use_navigate();

    // Applies an optimistic change, fires the request, and rolls the whole
    // detail back if it fails. The show row in the response carries the
    // recomputed category, which the client can't derive itself: the server
    // excludes specials from the counts and keys off the raw TMDB status.
    let mutate = move |apply: Box<dyn Fn(&mut ShowDetail)>, path: String| {
        let snapshot = detail.get_untracked();
        detail.update(|d| {
            if let Some(d) = d.as_mut() {
                apply(d);
            }
        });
        error.set(None);
        leptos::task::spawn_local(async move {
            match api::post::<Show>(&path).await {
                Ok(show) => detail.update(|d| {
                    if let Some(d) = d.as_mut() {
                        d.show = show;
                    }
                }),
                Err(e) => {
                    detail.set(snapshot);
                    error.set(Some(e.to_string()));
                }
            }
        });
    };

    let toggle_episode = move |episode_id: i64| {
        mutate(
            Box::new(move |d| {
                for season in &mut d.seasons {
                    for ep in &mut season.episodes {
                        if ep.id == episode_id {
                            ep.watched = !ep.watched;
                        }
                    }
                }
            }),
            format!("/episodes/{episode_id}/toggle"),
        );
    };

    let mark_season = move |season_id: i64| {
        mutate(
            Box::new(move |d| {
                for season in &mut d.seasons {
                    if season.season.id == season_id {
                        for ep in &mut season.episodes {
                            ep.watched = true;
                        }
                    }
                }
            }),
            format!("/seasons/{season_id}/mark-watched"),
        );
    };

    let show_id = move || detail.with(|d| d.as_ref().map(|d| d.show.id)).unwrap_or_default();

    let mark_all = move || {
        mutate(
            Box::new(|d| {
                for season in &mut d.seasons {
                    for ep in &mut season.episodes {
                        ep.watched = true;
                    }
                }
            }),
            format!("/shows/{}/mark-watched", show_id()),
        );
    };

    // Not optimistic: a refresh can add whole seasons, so the detail has to
    // come back from the server.
    let refresh = move || {
        refreshing.set(true);
        error.set(None);
        let id = show_id();
        leptos::task::spawn_local(async move {
            match api::post::<Show>(&format!("/shows/{id}/refresh")).await {
                Ok(_) => reload.run(()),
                Err(e) => error.set(Some(format!("Refresh failed: {e}"))),
            }
            refreshing.set(false);
        });
    };

    let delete_show = move || {
        if !window()
            .confirm_with_message("Delete this show and all its watched progress?")
            .unwrap_or(false)
        {
            return;
        }
        let id = show_id();
        let navigate = navigate.clone();
        leptos::task::spawn_local(async move {
            match api::delete(&format!("/shows/{id}")).await {
                Ok(()) => navigate("/", Default::default()),
                Err(e) => error.set(Some(format!("Failed to delete: {e}"))),
            }
        });
    };

    let header_style = move || {
        detail.with(|d| {
            d.as_ref()
                .and_then(|d| backdrop_url(&d.show.backdrop_path))
                .map(|url| {
                    format!(
                        "background-image:linear-gradient(90deg, rgba(26,29,36,1) 30%, rgba(26,29,36,0.6)), url('{url}'); background-size:cover; background-position:center;"
                    )
                })
        })
    };

    let season_ids = Memo::new(move |_| {
        detail.with(|d| {
            d.as_ref()
                .map(|d| d.seasons.iter().map(|s| s.season.id).collect::<Vec<_>>())
                .unwrap_or_default()
        })
    });

    view! {
        <div class="show-header" style=header_style>
            {move || {
                match detail.with(|d| d.as_ref().and_then(|d| poster_url(&d.show.poster_path))) {
                    Some(url) => {
                        view! { <div class="poster" style=format!("background-image:url('{url}')")></div> }
                            .into_any()
                    }
                    None => view! { <div class="poster"></div> }.into_any(),
                }
            }}
            <div class="details">
                <div class=move || {
                    format!("status-badge {}", detail.with(|d| d.as_ref().map(|d| d.show.status.clone()).unwrap_or_default()))
                }>{move || detail.with(|d| d.as_ref().map(|d| d.show.status.clone()).unwrap_or_default())}</div>
                <h2>{move || detail.with(|d| d.as_ref().map(|d| d.show.name.clone()).unwrap_or_default())}</h2>
                <div class="overview">
                    {move || {
                        detail.with(|d| d.as_ref().and_then(|d| d.show.overview.clone()).unwrap_or_default())
                    }}
                </div>
                <div class="actions">
                    <button class="btn" disabled=move || refreshing.get() on:click=move |_| refresh()>
                        {move || if refreshing.get() { "Refreshing…" } else { "Refresh Metadata" }}
                    </button>
                    <button class="btn" on:click=move |_| mark_all()>"Mark All Watched"</button>
                    <button class="btn btn-danger" on:click=move |_| delete_show()>"Delete Show"</button>
                </div>
                <ErrorMsg message=error/>
            </div>
        </div>
        <div id="seasons">
            <For each=move || season_ids.get() key=|id| *id let:season_id>
                <SeasonPanel
                    detail=detail
                    season_id=season_id
                    on_mark_season=Callback::new(move |id| mark_season(id))
                    on_toggle_episode=Callback::new(move |id| toggle_episode(id))
                />
            </For>
        </div>
    }
}

#[component]
fn SeasonPanel(
    detail: Detail,
    season_id: i64,
    on_mark_season: Callback<i64>,
    on_toggle_episode: Callback<i64>,
) -> impl IntoView {
    let open = RwSignal::new(false);

    let with_season = move |f: &dyn Fn(&shared::SeasonWithEpisodes) -> String| {
        detail.with(|d| {
            d.as_ref()
                .and_then(|d| d.seasons.iter().find(|s| s.season.id == season_id))
                .map(f)
                .unwrap_or_default()
        })
    };

    // Recomputes only when this season's watched count actually changes, not
    // on every mutation anywhere in the show.
    let progress = Memo::new(move |_| {
        detail.with(|d| {
            d.as_ref()
                .and_then(|d| d.seasons.iter().find(|s| s.season.id == season_id))
                .map(|s| (s.episodes.iter().filter(|e| e.watched).count(), s.episodes.len()))
                .unwrap_or((0, 0))
        })
    });

    let episode_ids = Memo::new(move |_| {
        detail.with(|d| {
            d.as_ref()
                .and_then(|d| d.seasons.iter().find(|s| s.season.id == season_id))
                .map(|s| s.episodes.iter().map(|e| e.id).collect::<Vec<_>>())
                .unwrap_or_default()
        })
    });

    let name = move || {
        with_season(&|s| {
            s.season
                .name
                .clone()
                .unwrap_or_else(|| format!("Season {}", s.season.tmdb_season_number))
        })
    };

    view! {
        <div class="season">
            <div class="season-header" on:click=move |_| open.update(|o| *o = !*o)>
                <div>
                    <div class="name">{name}</div>
                    <div class="progress">
                        {move || {
                            let (watched, total) = progress.get();
                            format!("{watched} / {total} watched")
                        }}
                    </div>
                </div>
                <div class="season-actions">
                    <button
                        class="btn mark-season-btn"
                        on:click=move |e| {
                            e.stop_propagation();
                            on_mark_season.run(season_id);
                        }
                    >
                        "Mark season watched"
                    </button>
                </div>
            </div>
            <div class="episode-list" class:open=move || open.get()>
                <For each=move || episode_ids.get() key=|id| *id let:episode_id>
                    <EpisodeRow detail=detail episode_id=episode_id on_toggle=on_toggle_episode/>
                </For>
            </div>
        </div>
    }
}

#[component]
fn EpisodeRow(detail: Detail, episode_id: i64, on_toggle: Callback<i64>) -> impl IntoView {
    let episode = Memo::new(move |_| {
        detail.with(|d| {
            d.as_ref().and_then(|d| {
                d.seasons
                    .iter()
                    .flat_map(|s| s.episodes.iter())
                    .find(|e| e.id == episode_id)
                    .cloned()
            })
        })
    });

    view! {
        <div class="episode-row">
            <input
                type="checkbox"
                prop:checked=move || episode.get().map(|e| e.watched).unwrap_or(false)
                on:change=move |_| on_toggle.run(episode_id)
            />
            <span class="ep-num">
                {move || episode.get().map(|e| format!("E{}", e.tmdb_episode_number)).unwrap_or_default()}
            </span>
            <span class="ep-name">
                {move || {
                    episode
                        .get()
                        .and_then(|e| e.name)
                        .unwrap_or_else(|| "Untitled".to_string())
                }}
            </span>
            <span class="ep-date">
                {move || episode.get().and_then(|e| e.air_date).unwrap_or_default()}
            </span>
        </div>
    }
}
