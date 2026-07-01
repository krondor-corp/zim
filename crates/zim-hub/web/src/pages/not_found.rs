//! 404.

use yew::prelude::*;
use yew_router::prelude::*;

use crate::routes::Route;

#[function_component(NotFound)]
pub fn not_found() -> Html {
    html! {
        <div class="onb"><main class="onb__main"><div class="onb__inner" style="text-align:center;">
            <span class="page-eyebrow">{ "404" }</span>
            <h1>{ "Page not found" }</h1>
            <p class="page-lead">{ "That page doesn't exist." }</p>
            <Link<Route> to={Route::Workspace} classes="btn btn-primary">
                { "Back to workspace" }
            </Link<Route>>
        </div></main></div>
    }
}
