mod api;
mod components;
mod pages;

use leptos::prelude::*;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;

use pages::{AddPage, IndexPage, NotFound, SettingsPage, ShowPage};

#[component]
fn App() -> impl IntoView {
    view! {
        <Router>
            <Routes fallback=NotFound>
                <Route path=path!("/") view=IndexPage/>
                <Route path=path!("/show/:id") view=ShowPage/>
                <Route path=path!("/add") view=AddPage/>
                <Route path=path!("/settings") view=SettingsPage/>
            </Routes>
        </Router>
    }
}

fn main() {
    // Turns a wasm panic into a readable browser console trace instead of
    // "unreachable executed".
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
