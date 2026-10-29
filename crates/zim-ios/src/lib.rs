//! iOS shell for the zim hub. The webview loads the hub's own pages, so
//! the SPA, its cookies and the browser-key passphrase flow work as in a
//! browser tab. The one thing a webview can't do is Google sign-in
//! (Google refuses embedded webviews): when the page follows a sign-in
//! link, the navigation is cancelled and the same sign-in runs in a
//! system web-auth sheet instead, which hands back a one-time token the
//! webview redeems for its own session cookie (see [`handoff`]).
//!
//! Remote content gets no commands and no plugin access.

mod handoff;
mod hub;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tauri_plugin_zim_web_auth::{Error as SheetError, WebAuthExt};
use url::Url;

const WINDOW: &str = "main";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_zim_web_auth::init())
        .setup(|app| {
            let hub = hub::url(app.config());
            let app_handle = app.handle().clone();
            let sheet_open = Arc::new(AtomicBool::new(false));
            let nav_hub = hub.clone();
            WebviewWindowBuilder::new(app, WINDOW, WebviewUrl::External(hub))
                .background_color(hub::BACKGROUND)
                .on_navigation(move |nav| {
                    let Some(sign_in) = hub::sign_in_start(&nav_hub, nav) else {
                        return true;
                    };
                    // One sheet at a time; a double tap is swallowed.
                    if !sheet_open.swap(true, Ordering::SeqCst) {
                        let (app, hub, open) =
                            (app_handle.clone(), nav_hub.clone(), sheet_open.clone());
                        tauri::async_runtime::spawn(async move {
                            sign_in_with_sheet(&app, &hub, sign_in).await;
                            open.store(false, Ordering::SeqCst);
                        });
                    }
                    false
                })
                .build()?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the zim iOS shell");
}

/// Run `sign_in` in the system sheet, then point the webview at the
/// result: the hub's redeem route on success, the sign-in page on
/// failure, nowhere if the sheet was dismissed.
async fn sign_in_with_sheet(app: &AppHandle, hub: &Url, sign_in: hub::SignIn) {
    let Some(window) = app.get_webview_window(WINDOW) else {
        return;
    };
    let verifier = handoff::new_verifier();
    let sheet = handoff::sheet_url(hub, sign_in.path, sign_in.next.as_deref(), &verifier);
    let landing = match app
        .web_auth()
        .authenticate(sheet.as_str(), handoff::CALLBACK_SCHEME)
        .await
    {
        Ok(callback) => match handoff::parse_callback(&callback) {
            handoff::Callback::Token(token) => handoff::complete_url(hub, &token, &verifier),
            handoff::Callback::Failed => handoff::failed_url(hub),
        },
        Err(SheetError::Cancelled) => return,
        Err(e) => {
            tracing::error!("sign-in sheet failed: {e}");
            handoff::failed_url(hub)
        }
    };
    navigate(&window, landing);
}

fn navigate(window: &WebviewWindow, url: Url) {
    if let Err(e) = window.navigate(url) {
        tracing::error!("webview navigation failed: {e}");
    }
}
