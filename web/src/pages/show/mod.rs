mod store;

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::{use_navigate, use_params_map};
use shared::ShowDetail;

use crate::api;
use crate::components::{Notice, Topbar};
use crate::images::{backdrop_url, poster_url};

use store::DetailStore;

/// The page is in exactly one of these. `Ready` holds the store, which
/// survives mutations - so the season panels keep their open/closed state
/// instead of being rebuilt on every checkbox click.
#[derive(Clone)]
enum Phase {
    Loading,
    Failed(String),
    Ready(DetailStore),
}

#[component]
pub fn ShowPage() -> impl IntoView {
    let params = use_params_map();
    let show_id = Memo::new(move |_| params.read().get("id").and_then(|s| s.parse::<i64>().ok()));
    let phase = RwSignal::new(Phase::Loading);

    // Re-runs if the route id changes, e.g. navigating between two shows
    // without unmounting this component.
    Effect::new(move |_| {
        let Some(id) = show_id.get() else {
            phase.set(Phase::Failed("Invalid show id.".to_string()));
            return;
        };
        phase.set(Phase::Loading);
        leptos::task::spawn_local(async move {
            match api::get::<ShowDetail>(&format!("/shows/{id}")).await {
                Ok(detail) => phase.set(Phase::Ready(DetailStore::new(detail))),
                Err(e) => phase.set(Phase::Failed(format!("Failed to load show: {e}"))),
            }
        });
    });

    view! {
        <Topbar>
            <A href="/" attr:class="btn">"← Back"</A>
            <A href="/settings" attr:class="btn">"Settings"</A>
        </Topbar>
        <main>
            {move || match phase.get() {
                Phase::Loading => view! { <div class="loading">"Loading…"</div> }.into_any(),
                Phase::Failed(msg) => view! { <div class="empty-state">{msg}</div> }.into_any(),
                Phase::Ready(store) => view! { <Loaded store=store/> }.into_any(),
            }}
        </main>
    }
}

#[component]
fn Loaded(store: DetailStore) -> impl IntoView {
    let navigate = use_navigate();

    let header_style = move || {
        store.with(|i| {
            backdrop_url(&i.show().backdrop_path).map(|url| {
                format!(
                    "background-image:linear-gradient(90deg, rgba(26,29,36,1) 30%, rgba(26,29,36,0.6)), url('{url}'); background-size:cover; background-position:center;"
                )
            })
        })
    };

    let delete_show = move || {
        if !window()
            .confirm_with_message("Delete this show and all its watched progress?")
            .unwrap_or(false)
        {
            return;
        }
        let navigate = navigate.clone();
        store.delete(move || navigate("/", Default::default()));
    };

    let season_ids = Memo::new(move |_| store.with(|i| i.season_ids()));
    let refreshing = store.refreshing();

    view! {
        <div class="show-header" style=header_style>
            {move || {
                match store.with(|i| poster_url(&i.show().poster_path)) {
                    Some(url) => {
                        view! { <div class="poster" style=format!("background-image:url('{url}')")></div> }
                            .into_any()
                    }
                    None => view! { <div class="poster"></div> }.into_any(),
                }
            }}
            <div class="details">
                <div class=move || {
                    format!("status-badge {}", store.with(|i| i.show().category.as_str()))
                }>{move || store.with(|i| i.show().category.label())}</div>
                <h2>{move || store.with(|i| i.show().name.clone())}</h2>
                <div class="overview">
                    {move || store.with(|i| i.show().overview.clone().unwrap_or_default())}
                </div>
                <div class="actions">
                    <button
                        class="btn"
                        disabled=move || refreshing.get()
                        on:click=move |_| store.refresh()
                    >
                        {move || if refreshing.get() { "Refreshing…" } else { "Refresh Metadata" }}
                    </button>
                    <button class="btn" on:click=move |_| store.mark_all()>"Mark All Watched"</button>
                    <button class="btn btn-danger" on:click=move |_| delete_show()>"Delete Show"</button>
                </div>
                <Notice message=store.error()/>
            </div>
        </div>
        <div id="seasons">
            <For each=move || season_ids.get() key=|id| *id let:season_id>
                <SeasonPanel store=store season_id=season_id/>
            </For>
        </div>
    }
}

#[component]
fn SeasonPanel(store: DetailStore, season_id: i64) -> impl IntoView {
    let open = RwSignal::new(false);

    // Each of these recomputes only when its own slice changes, and each is
    // an O(1) lookup rather than a scan of the whole show.
    let progress = Memo::new(move |_| store.with(|i| i.season_progress(season_id)));
    let episode_ids = Memo::new(move |_| store.with(|i| i.episode_ids(season_id)));
    let name = move || store.with(|i| i.season_name(season_id));

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
                            store.mark_season(season_id);
                        }
                    >
                        "Mark season watched"
                    </button>
                </div>
            </div>
            <div class="episode-list" class:open=move || open.get()>
                <For each=move || episode_ids.get() key=|id| *id let:episode_id>
                    <EpisodeRow store=store episode_id=episode_id/>
                </For>
            </div>
        </div>
    }
}

#[component]
fn EpisodeRow(store: DetailStore, episode_id: i64) -> impl IntoView {
    let watched = Memo::new(move |_| store.with(|i| i.episode(episode_id).is_some_and(|e| e.watched)));
    let number = Memo::new(move |_| {
        store.with(|i| {
            i.episode(episode_id)
                .map(|e| format!("E{}", e.tmdb_episode_number))
                .unwrap_or_default()
        })
    });
    let name = Memo::new(move |_| {
        store.with(|i| {
            i.episode(episode_id)
                .and_then(|e| e.name.clone())
                .unwrap_or_else(|| "Untitled".to_string())
        })
    });
    let air_date = Memo::new(move |_| {
        store.with(|i| {
            i.episode(episode_id)
                .and_then(|e| e.air_date.clone())
                .unwrap_or_default()
        })
    });

    view! {
        <div class="episode-row">
            <input
                type="checkbox"
                prop:checked=move || watched.get()
                on:change=move |_| store.toggle_episode(episode_id)
            />
            <span class="ep-num">{move || number.get()}</span>
            <span class="ep-name">{move || name.get()}</span>
            <span class="ep-date">{move || air_date.get()}</span>
        </div>
    }
}
