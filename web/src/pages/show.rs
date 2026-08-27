use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::Topbar;

/// Stub: ported in stage 5.
#[component]
pub fn ShowPage() -> impl IntoView {
    view! {
        <Topbar>
            <A href="/" attr:class="btn">"← Back"</A>
            <A href="/settings" attr:class="btn">"Settings"</A>
        </Topbar>
        <main>
            <div class="empty-state">"Show detail — not ported yet."</div>
        </main>
    }
}
