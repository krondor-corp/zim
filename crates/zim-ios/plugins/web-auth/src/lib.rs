//! One system web-auth sheet, driven from Rust. Google refuses OAuth
//! inside embedded webviews, so the app opens the sign-in in
//! `ASWebAuthenticationSession` and waits for it to land on the app's
//! callback scheme. Nothing is exposed to the webview: the plugin has no
//! commands, so remote content can't open the sheet itself.

use std::marker::PhantomData;

use tauri::plugin::{Builder, TauriPlugin};
use tauri::{Manager, Runtime};

#[cfg(target_os = "ios")]
use tauri::plugin::PluginHandle;

#[cfg(target_os = "ios")]
tauri::ios_plugin_binding!(init_plugin_zim_web_auth);

/// Why the sheet didn't return a callback URL.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// The person dismissed the sheet.
    Cancelled,
    /// Not running on iOS.
    Unsupported,
    /// The system or the plugin failed.
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Cancelled => write!(f, "sign-in cancelled"),
            Error::Unsupported => write!(f, "web-auth sessions are only available on iOS"),
            Error::Failed(why) => write!(f, "web-auth session failed: {why}"),
        }
    }
}

impl std::error::Error for Error {}

/// Handle to the sheet; reach it with `app.web_auth()`.
pub struct WebAuth<R: Runtime> {
    #[cfg(target_os = "ios")]
    handle: PluginHandle<R>,
    _runtime: PhantomData<fn() -> R>,
}

impl<R: Runtime> WebAuth<R> {
    /// Show `url` in the sheet until it redirects to `callback_scheme://…`;
    /// that final URL. Shares cookies with Safari, so a Google session
    /// already there is reused.
    #[cfg(target_os = "ios")]
    pub async fn authenticate(&self, url: &str, callback_scheme: &str) -> Result<String, Error> {
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Args<'a> {
            url: &'a str,
            callback_scheme: &'a str,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Callback {
            callback_url: String,
        }
        let res: Callback = self
            .handle
            .run_mobile_plugin_async(
                "authenticate",
                Args {
                    url,
                    callback_scheme,
                },
            )
            .await
            .map_err(|e| match e {
                tauri::plugin::mobile::PluginInvokeError::InvokeRejected(r)
                    if r.message.as_deref() == Some("cancelled") =>
                {
                    Error::Cancelled
                }
                other => Error::Failed(other.to_string()),
            })?;
        Ok(res.callback_url)
    }

    #[cfg(not(target_os = "ios"))]
    pub async fn authenticate(&self, _url: &str, _callback_scheme: &str) -> Result<String, Error> {
        Err(Error::Unsupported)
    }
}

/// `app.web_auth()` on anything that holds an app handle.
pub trait WebAuthExt<R: Runtime> {
    fn web_auth(&self) -> &WebAuth<R>;
}

impl<R: Runtime, T: Manager<R>> WebAuthExt<R> for T {
    fn web_auth(&self) -> &WebAuth<R> {
        self.state::<WebAuth<R>>().inner()
    }
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("zim-web-auth")
        .setup(|app, api| {
            #[cfg(target_os = "ios")]
            let handle = api.register_ios_plugin(init_plugin_zim_web_auth)?;
            #[cfg(not(target_os = "ios"))]
            let _ = api;
            app.manage(WebAuth::<R> {
                #[cfg(target_os = "ios")]
                handle,
                _runtime: PhantomData,
            });
            Ok(())
        })
        .build()
}
