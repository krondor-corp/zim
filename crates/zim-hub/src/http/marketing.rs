//! `GET /` — public marketing page.
//!
//! Zero user context. Zero gates. Just hero + features + a sign-in
//! CTA pointing at `/auth/google/login`. Lives outside `/app` so
//! anonymous visitors hit it without redirect-bouncing through the
//! login flow.

use askama::Template;
use axum::response::Html;

use crate::errors::Result;

#[derive(Template)]
#[template(path = "pages/marketing.html")]
struct MarketingTemplate {
    hub_version: &'static str,
}

pub async fn home() -> Result<Html<String>> {
    let tmpl = MarketingTemplate {
        hub_version: env!("CARGO_PKG_VERSION"),
    };
    Ok(Html(tmpl.render()?))
}
