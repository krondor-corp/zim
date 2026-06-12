use axum::routing::post;
use axum::Router;

use crate::ServiceState;

pub mod add;
pub mod approve;
pub mod cat;
pub mod create;
pub mod delete;
pub mod export;
pub mod files_publish;
pub mod files_unpublish;
pub mod folders_publish;
pub mod folders_unpublish;
pub mod history;
pub mod ignore;
pub mod list;
pub mod ls;
pub mod mirror;
pub mod mkdir;
pub mod mv;
pub mod ping;
pub mod published_get;
pub mod rename;
pub mod share;
pub mod shares;
pub mod stat;
pub mod sync;
pub mod unshare;
pub mod update;
pub mod viewer;

// Re-export for convenience
pub use approve::ApproveRequest;
pub use create::CreateRequest;
pub use ignore::IgnoreRequest;
pub use list::ListRequest;
pub use share::ShareRequest;
pub use shares::SharesRequest;
pub use unshare::UnshareRequest;

pub fn router(state: ServiceState) -> Router<ServiceState> {
    Router::new()
        .route("/", post(create::handler))
        .route("/list", post(list::handler))
        .route("/add", post(add::handler))
        .route("/update", post(update::handler))
        .route("/rename", post(rename::handler))
        .route("/mv", post(mv::handler))
        .route("/delete", post(delete::handler))
        .route("/mkdir", post(mkdir::handler))
        .route("/ls", post(ls::handler))
        .route("/cat", post(cat::handler).get(cat::handler_get))
        .route("/ping", post(ping::handler))
        .route("/share", post(share::handler))
        .route("/shares", post(shares::handler))
        .route("/unshare", post(unshare::handler))
        .route("/files/publish", post(files_publish::handler))
        .route("/files/unpublish", post(files_unpublish::handler))
        .route("/folders/publish", post(folders_publish::handler))
        .route("/folders/unpublish", post(folders_unpublish::handler))
        .route("/published/get", post(published_get::handler))
        .route("/export", post(export::handler))
        .route("/history", post(history::handler))
        .route("/stat", post(stat::handler))
        .route("/approve", post(approve::handler))
        .route("/ignore", post(ignore::handler))
        .route("/viewers/list", post(viewer::list_handler))
        .route("/viewers/authorize", post(viewer::authorize_handler))
        .route("/viewers/deauthorise", post(viewer::deauthorise_handler))
        .route("/relays/list", post(mirror::list_handler))
        .route("/relays/add", post(mirror::add_handler))
        .route("/relays/remove", post(mirror::remove_handler))
        .route("/sync/add", post(sync::add_handler))
        .route("/sync/remove", post(sync::remove_handler))
        .route("/sync/list", post(sync::list_handler))
        .route("/sync/pause", post(sync::pause_handler))
        .route("/sync/resume", post(sync::resume_handler))
        .with_state(state)
}
