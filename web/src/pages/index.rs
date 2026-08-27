use leptos::prelude::*;
use leptos_router::components::A;
use shared::Show;

use crate::api;
use crate::components::Topbar;
use crate::images::poster_url;

const CATEGORIES: [(&str, &str); 4] = [
    ("watching", "Watching"),
    ("ongoing", "Ongoing"),
    ("watchlist", "Watch List"),
    ("finished", "Finished"),
];

const TAB_STORAGE_KEY: &str = "showtime_active_tab";

/// Maps a show's stored status onto a dashboard category. Anything
/// unrecognised lands in the watch list, matching the old
/// `byCategory[s.status] || byCategory.watchlist` fallback.
fn bucket(status: &str) -> &'static str {
    match status {
        "watching" => "watching",
        "ongoing" => "ongoing",
        "finished" => "finished",
        _ => "watchlist",
    }
}

fn stored_tab() -> String {
    window()
        .local_storage()
        .ok()
        .flatten()
        .and_then(|s| s.get_item(TAB_STORAGE_KEY).ok().flatten())
        .filter(|t| CATEGORIES.iter().any(|(key, _)| key == t))
        .unwrap_or_else(|| "watching".to_string())
}

fn store_tab(tab: &str) {
    if let Ok(Some(storage)) = window().local_storage() {
        let _ = storage.set_item(TAB_STORAGE_KEY, tab);
    }
}

#[component]
pub fn IndexPage() -> impl IntoView {
    let shows = LocalResource::new(|| async { api::get::<Vec<Show>>("/shows").await });
    let active_tab = RwSignal::new(stored_tab());

    // Everything below derives from this one memo, so a refetch that returns
    // an identical list re-renders nothing.
    let all = Memo::new(move |_| shows.get().and_then(|r| r.ok()).unwrap_or_default());
    let load_error = Memo::new(move |_| match shows.get() {
        Some(Err(e)) => Some(e.to_string()),
        _ => None,
    });

    let in_category = move |key: &'static str| {
        Memo::new(move |_| {
            all.get()
                .into_iter()
                .filter(|s| bucket(&s.status) == key)
                .collect::<Vec<_>>()
        })
    };
    let watching = in_category("watching");
    let ongoing = in_category("ongoing");
    let watchlist = in_category("watchlist");
    let finished = in_category("finished");

    let for_key = move |key: &str| match key {
        "watching" => watching.get(),
        "ongoing" => ongoing.get(),
        "finished" => finished.get(),
        _ => watchlist.get(),
    };
    let count_for = move |key: &str| match key {
        "watching" => watching.read().len(),
        "ongoing" => ongoing.read().len(),
        "finished" => finished.read().len(),
        _ => watchlist.read().len(),
    };

    let has_shows = move || !all.read().is_empty();
    let active_label =
        move || CATEGORIES.iter().find(|(k, _)| *k == active_tab.get()).map(|(_, l)| *l).unwrap_or("this category");

    view! {
        <Topbar left=move || {
            view! {
                <Show when=has_shows fallback=|| ()>
                    <div class="tabs">
                        {CATEGORIES
                            .iter()
                            .map(|(key, label)| {
                                view! {
                                    <button
                                        class="tab-btn"
                                        class:active=move || active_tab.get() == *key
                                        on:click=move |_| {
                                            active_tab.set(key.to_string());
                                            store_tab(key);
                                        }
                                    >
                                        {*label}
                                        <span class="count">{move || count_for(key)}</span>
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
                if let Some(e) = load_error.get() {
                    return view! {
                        <div class="empty-state">{format!("Failed to load shows: {e}")}</div>
                    }
                        .into_any();
                }
                if shows.get().is_none() {
                    return view! { <div class="loading">"Loading shows…"</div> }.into_any();
                }
                if !has_shows() {
                    return view! {
                        <div class="empty-state">
                            "No shows yet. Click \"+ Add Show\" to get started."
                        </div>
                    }
                        .into_any();
                }
                let list = for_key(&active_tab.get());
                if list.is_empty() {
                    return view! {
                        <div class="empty-state">
                            {format!("Nothing in {} yet.", active_label())}
                        </div>
                    }
                        .into_any();
                }
                view! {
                    <div class="grid">
                        <For each=move || for_key(&active_tab.get()) key=|s| s.id let:show>
                            <ShowCard show=show/>
                        </For>
                    </div>
                }
                    .into_any()
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
