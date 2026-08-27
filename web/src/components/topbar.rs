use leptos::prelude::*;
use leptos_router::components::A;

/// The sticky header shared by every page. `children` fills the right-hand
/// nav; `left` is for anything that sits beside the title, which today is
/// only the dashboard's category tabs.
#[component]
pub fn Topbar(
    #[prop(optional, into)] left: Option<ViewFn>,
    children: Children,
) -> impl IntoView {
    view! {
        <div class="topbar">
            <div class="left">
                <A href="/" attr:class="site-title">
                    <h1>"📺 ShowTime"</h1>
                </A>
                {left.map(|l| l.run())}
            </div>
            <nav>{children()}</nav>
        </div>
    }
}
