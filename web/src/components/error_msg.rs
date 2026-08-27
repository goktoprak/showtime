use leptos::prelude::*;

/// Inline error text. Renders nothing when there is no error.
///
/// Replaces the `alert()` calls the show page used to make, so that every
/// page reports failures the same way.
#[component]
pub fn ErrorMsg(#[prop(into)] message: Signal<Option<String>>) -> impl IntoView {
    move || {
        message
            .get()
            .map(|m| view! { <div class="msg error">{m}</div> })
    }
}
