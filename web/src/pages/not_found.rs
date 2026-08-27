use leptos::prelude::*;
use leptos_router::components::A;

use crate::components::Topbar;

/// Rendered for any client route that doesn't match. Deliberately a real
/// view rather than a redirect to "/", so a broken link shows up as a
/// broken link instead of silently landing on the dashboard.
#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <Topbar>
            <A href="/" attr:class="btn">"← Back"</A>
        </Topbar>
        <main>
            <div class="empty-state">"That page doesn't exist."</div>
        </main>
    }
}
