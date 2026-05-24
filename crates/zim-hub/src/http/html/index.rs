use askama::Template;
use axum::response::Html;

#[derive(Template)]
#[template(path = "pages/index.html")]
struct IndexTemplate;

pub async fn handler() -> Html<String> {
    Html(IndexTemplate.render().unwrap_or_default())
}
