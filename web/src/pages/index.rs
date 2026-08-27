use leptos::prelude::*;
use leptos_router::components::A;
use shared::{Show, ShowCategory};
use std::collections::HashMap;

use crate::api;
use crate::components::Topbar;
use crate::images::poster_url;

const TAB_STORAGE_KEY: &str = "showtime_active_tab";

/// The remembered tab, or Watching. Anything unparseable in local storage
/// falls back rather than throwing the dashboard off.
fn stored_tab() -> ShowCategory {
    window()
        .local_storage()
        .ok()
        .flatten()
        .and_then(|s| s.get_item(TAB_STORAGE_KEY).ok().flatten())
        .and_then(|t| ShowCategory::parse(&t))
        .unwrap_or(ShowCategory::Watching)
}

fn store_tab(tab: ShowCategory) {
    if let Ok(Some(storage)) = window().local_storage() {
        let _ = storage.set_item(TAB_STORAGE_KEY, tab.as_str());
    }
}

/// Everything the dashboard knows about the show list, in one value. The page
/// subscribes to the resource once, here, rather than in each thing that
/// needs a different slice of it.
#[derive(Clone, PartialEq)]
enum LoadState {
    Loading,
    Failed(String),
    Ready(HashMap<ShowCategory, Vec<Show>>),
}

#[component]
pub fn IndexPage() -> impl IntoView {
    let shows = LocalResource::new(|| async { api::get::<Vec<Show>>("/shows").await });
    let active_tab = RwSignal::new(stored_tab());

    let state = Memo::new(move |_| match shows.get() {
        None => LoadState::Loading,
        Some(Err(e)) => LoadState::Failed(e.to_string()),
        Some(Ok(list)) => {
            let mut by_category: HashMap<ShowCategory, Vec<Show>> = HashMap::new();
            for show in list {
                by_category.entry(show.category).or_default().push(show);
            }
            LoadState::Ready(by_category)
        }
    });

    let in_tab = move |tab: ShowCategory| {
        state.with(|s| match s {
            LoadState::Ready(by_category) => by_category.get(&tab).cloned().unwrap_or_default(),
            _ => Vec::new(),
        })
    };
    let count_of = move |tab: ShowCategory| {
        state.with(|s| match s {
            LoadState::Ready(by_category) => by_category.get(&tab).map_or(0, |v| v.len()),
            _ => 0,
        })
    };
    let has_shows = move || {
        state.with(|s| match s {
            LoadState::Ready(by_category) => !by_category.is_empty(),
            _ => false,
        })
    };

    view! {
        <Topbar left=move || {
            view! {
                <Show when=has_shows fallback=|| ()>
                    <div class="tabs">
                        {ShowCategory::ALL
                            .into_iter()
                            .map(|category| {
                                view! {
                                    <button
                                        class="tab-btn"
                                        class:active=move || active_tab.get() == category
                                        on:click=move |_| {
                                            active_tab.set(category);
                                            store_tab(category);
                                        }
                                    >
                                        {category.label()}
                                        <span class="count">{move || count_of(category)}</span>
                                    </button>
                                }
                            })
                            .collect_view()}
                    </div>
                </Show>
            }
        }>
            <A href="/add" attr:class="btn btn-primary">"+ Add Show"</A>
            <A href="/settings" attr:class="btn">"Settings"</A>
        </Topbar>
        <main>
            {move || {
                match state.get() {
                    LoadState::Loading => {
                        view! { <div class="loading">"Loading shows…"</div> }.into_any()
                    }
                    LoadState::Failed(e) => {
                        view! { <div class="empty-state">{format!("Failed to load shows: {e}")}</div> }
                            .into_any()
                    }
                    LoadState::Ready(by_category) if by_category.is_empty() => {
                        view! {
                            <div class="empty-state">
                                "No shows yet. Click \"+ Add Show\" to get started."
                            </div>
                        }
                            .into_any()
                    }
                    LoadState::Ready(_) => {
                        let tab = active_tab.get();
                        if count_of(tab) == 0 {
                            return view! {
                                <div class="empty-state">
                                    {format!("Nothing in {} yet.", tab.label())}
                                </div>
                            }
                                .into_any();
                        }
                        view! {
                            <div class="grid">
                                <For each=move || in_tab(active_tab.get()) key=|s| s.id let:show>
                                    <ShowCard show=show/>
                                </For>
                            </div>
                        }
                            .into_any()
                    }
                }
            }}
        </main>
    }
}

#[component]
fn ShowCard(show: Show) -> impl IntoView {
    let poster = poster_url(&show.poster_path);
    view! {
        <A href=format!("/show/{}", show.id) attr:class="show-card">
            {match poster {
                Some(url) => {
                    view! { <div class="poster" style=format!("background-image:url('{url}')")></div> }
                        .into_any()
                }
                None => view! { <div class="poster">"No image"</div> }.into_any(),
            }}
            <div class="info">
                <div class="title">{show.name.clone()}</div>
                <div class="sub">{show.tmdb_status.clone().unwrap_or_default()}</div>
            </div>
        </A>
    }
}
