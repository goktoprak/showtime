use leptos::html;
use leptos::prelude::*;
use leptos_router::components::A;
use shared::{RefreshAllResponse, SetApiKeyRequest, SettingsResponse};

use crate::api;
use crate::components::{Message, Notice, Topbar};

#[component]
pub fn SettingsPage() -> impl IntoView {
    let settings = LocalResource::new(|| async { api::get::<SettingsResponse>("/settings").await });
    let reload = Callback::new(move |()| settings.refetch());

    view! {
        <Topbar>
            <A href="/" attr:class="btn">"← Back"</A>
        </Topbar>
        <main>
            <div class="form-page">
                <h2>"Settings"</h2>
                {move || match settings.get() {
                    None => view! { <div class="loading">"Loading…"</div> }.into_any(),
                    Some(Err(e)) => view! {
                        <div class="msg error">{format!("Could not load settings: {e}")}</div>
                    }
                    .into_any(),
                    Some(Ok(s)) => {
                        if s.has_api_key {
                            view! {
                                <KeySet masked=s.masked_key.unwrap_or_default() on_change=reload/>
                            }
                            .into_any()
                        } else {
                            view! { <AddKeyForm on_saved=reload/> }.into_any()
                        }
                    }
                }}
            </div>
        </main>
    }
}

/// Shown when no key is stored yet.
#[component]
fn AddKeyForm(on_saved: Callback<()>) -> impl IntoView {
    let input: NodeRef<html::Input> = NodeRef::new();
    let message = RwSignal::new(None::<Message>);

    let save = Action::new_local(move |key: &String| {
        let key = key.clone();
        async move {
            api::post_json::<_, serde_json::Value>(
                "/settings/apikey",
                &SetApiKeyRequest { api_key: key },
            )
            .await
        }
    });

    // Report the outcome once the request settles: refetch on success so the
    // page swaps to the key-set view, otherwise surface the server's message.
    Effect::new(move |_| {
        if let Some(result) = save.value().get() {
            match result {
                Ok(_) => on_saved.run(()),
                Err(e) => message.set(Some(Message::error(e.to_string()))),
            }
        }
    });

    let submit = move || {
        let value = input.get().map(|el| el.value()).unwrap_or_default();
        let key = value.trim().to_string();
        if key.is_empty() {
            message.set(Some(Message::error("Please enter an API key.")));
            return;
        }
        message.set(None);
        save.dispatch(key);
    };

    view! {
        <p style="color:var(--text-dim); font-size:13px;">
            "Paste your TMDB API key (v3 auth). Get one free at themoviedb.org → Settings → API."
        </p>
        <label for="apiKey">"TMDB API Key"</label>
        <input
            id="apiKey"
            type="text"
            placeholder="your api key"
            node_ref=input
            on:keydown=move |e| if e.key() == "Enter" { submit() }
        />
        <div class="actions">
            <button class="btn btn-primary" disabled=move || save.pending().get() on:click=move |_| submit()>
                {move || if save.pending().get() { "Saving…" } else { "Save" }}
            </button>
        </div>
        <Notice message=message/>
    }
}

/// Shown when a key is already stored.
#[component]
fn KeySet(masked: String, on_change: Callback<()>) -> impl IntoView {
    let message = RwSignal::new(None::<Message>);

    let refresh_all = Action::new_local(|_: &()| async {
        api::post::<RefreshAllResponse>("/shows/refresh-all").await
    });

    Effect::new(move |_| {
        if let Some(result) = refresh_all.value().get() {
            match result {
                Ok(r) => {
                    let mut parts = vec![format!(
                        "Refreshed {} show{}.",
                        r.refreshed,
                        if r.refreshed == 1 { "" } else { "s" }
                    )];
                    if r.failed != 0 {
                        parts.push(format!("{} failed.", r.failed));
                    }
                    // Info, not Success: this line can carry failures.
                    message.set(Some(Message::info(parts.join(" "))));
                }
                Err(e) => message.set(Some(Message::error(e.to_string()))),
            }
        }
    });

    let delete_key = Action::new_local(|_: &()| async { api::delete("/settings/apikey").await });

    Effect::new(move |_| {
        if let Some(result) = delete_key.value().get() {
            match result {
                Ok(()) => on_change.run(()),
                Err(e) => message.set(Some(Message::error(e.to_string()))),
            }
        }
    });

    view! {
        <p style="color:var(--text-dim); font-size:13px;">"A TMDB API key is currently set."</p>
        <label>"TMDB API Key"</label>
        <input type="text" value=masked disabled/>
        <div class="actions">
            // `refresh-all` pins the width in CSS so swapping in the shorter
            // "Refreshing…" label doesn't make the button shrink.
            <button
                class="btn refresh-all"
                disabled=move || refresh_all.pending().get()
                on:click=move |_| {
                    message.set(None);
                    refresh_all.dispatch(());
                }
            >
                {move || if refresh_all.pending().get() { "Refreshing…" } else { "Refresh All Shows" }}
            </button>
            // A plain anchor, not a router link: this is a server download and
            // must not be intercepted by the client-side router.
            <a class="btn" href="/api/export" download>"Download Backup"</a>
            <button
                class="btn btn-danger"
                disabled=move || delete_key.pending().get()
                on:click=move |_| {
                    let ok = window()
                        .confirm_with_message(
                            "Remove the saved TMDB API key? You'll need to add it again to fetch or refresh shows.",
                        )
                        .unwrap_or(false);
                    if ok {
                        delete_key.dispatch(());
                    }
                }
            >
                {move || if delete_key.pending().get() { "Deleting…" } else { "Delete API Key" }}
            </button>
        </div>
        <Notice message=message/>
    }
}
