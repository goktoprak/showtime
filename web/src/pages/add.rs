use leptos::html;
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;
use shared::{AddShowRequest, Show};
use std::time::Duration;

use crate::api;
use crate::components::{Message, Notice, Topbar};

/// Where the form is in its one-shot lifecycle. The button is disabled for
/// anything but `Idle`: once the redirect is queued a second click would fire
/// another add while the timeout is still pending.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Idle,
    Submitting,
    Redirecting,
}

#[component]
pub fn AddPage() -> impl IntoView {
    let input: NodeRef<html::Input> = NodeRef::new();
    let message = RwSignal::new(None::<Message>);
    let phase = RwSignal::new(Phase::Idle);
    let navigate = use_navigate();

    let add = Action::new_local(|tmdb_id: &i64| {
        let req = AddShowRequest { tmdb_id: *tmdb_id };
        async move { api::post_json::<_, Show>("/shows", &req).await }
    });

    Effect::new(move |_| {
        if let Some(result) = add.value().get() {
            match result {
                Ok(show) => {
                    message.set(Some(Message::success(format!(
                        "Added \"{}\". Redirecting…",
                        show.name
                    ))));
                    phase.set(Phase::Redirecting);
                    let navigate = navigate.clone();
                    let id = show.id;
                    set_timeout(
                        move || navigate(&format!("/show/{id}"), Default::default()),
                        Duration::from_millis(700),
                    );
                }
                Err(e) => {
                    message.set(Some(Message::error(e.to_string())));
                    phase.set(Phase::Idle);
                }
            }
        }
    });

    // A blank field, a non-number, or 0 are all rejected, matching what the
    // old `if (!tmdbId)` check did after parseInt.
    let submit = move || {
        // The button is disabled while busy, but the Enter key isn't, so the
        // phase has to be checked here too.
        if phase.get_untracked() != Phase::Idle {
            return;
        }
        let raw = input.get().map(|el| el.value()).unwrap_or_default();
        match raw.trim().parse::<i64>() {
            Ok(id) if id != 0 => {
                message.set(None);
                phase.set(Phase::Submitting);
                add.dispatch(id);
            }
            _ => message.set(Some(Message::error("Please enter a valid TMDB ID."))),
        }
    };

    let busy = move || phase.get() != Phase::Idle;

    view! {
        <Topbar>
            <A href="/" attr:class="btn">"← Back"</A>
            <A href="/settings" attr:class="btn">"Settings"</A>
        </Topbar>
        <main>
            <div class="form-page">
                <h2>"Add a Show"</h2>
                <p style="color: var(--text-dim); font-size: 13px">
                    "Enter a TMDB TV show ID. You can find it in the URL of a show's page on themoviedb.org, e.g. themoviedb.org/tv/"
                    <strong>"1399"</strong>
                    "-game-of-thrones."
                </p>
                <label for="tmdbId">"TMDB ID"</label>
                <input
                    id="tmdbId"
                    type="number"
                    placeholder="e.g. 1399"
                    autofocus
                    node_ref=input
                    on:keydown=move |e| if e.key() == "Enter" { submit() }
                />
                <div class="actions">
                    <button class="btn btn-primary" disabled=busy on:click=move |_| submit()>
                        {move || if busy() { "Adding…" } else { "Add Show" }}
                    </button>
                </div>
                <Notice message=message/>
            </div>
        </main>
    }
}
