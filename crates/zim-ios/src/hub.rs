//! Which hub the webview loads, and which of its navigations are the
//! start of a sign-in the system web-auth sheet must take over.

use url::Url;

/// Production hub.
const RELEASE_HUB: &str = "https://hub.zim.krondor.org";
/// Debug default: the canonical dev hub. Per-worktree hubs have other
/// ports (`./bin/dev ports`); point at one with `--config` (see [`url`]).
const DEBUG_HUB: &str = "http://127.0.0.1:17190";

/// The hub's dark-theme `--background` (`hsl(195 6% 12%)` in
/// `zim-hub/static/style.css`), painted behind the webview so nothing
/// white shows in the status-bar area or before the first page lands.
pub const BACKGROUND: tauri::window::Color = tauri::window::Color(29, 32, 32, 255);

/// The hub to load: `plugins.zim-hub.url` from the Tauri config when
/// set (`cargo tauri ios dev --config '{"plugins":{"zim-hub":{"url":"…"}}}'`),
/// else the default for this build profile.
pub fn url(config: &tauri::Config) -> Url {
    let raw = config
        .plugins
        .0
        .get("zim-hub")
        .and_then(|c| c.get("url"))
        .and_then(|u| u.as_str())
        .unwrap_or(if cfg!(debug_assertions) {
            DEBUG_HUB
        } else {
            RELEASE_HUB
        });
    Url::parse(raw).unwrap_or_else(|e| panic!("hub url {raw:?} is invalid: {e}"))
}

/// The hub's own sign-in start routes whose handshake can't run inside
/// the webview (Google refuses embedded webviews), and — in debug builds
/// — the dev-trust form, so the same handoff is testable without Google.
fn start_paths() -> &'static [&'static str] {
    if cfg!(debug_assertions) {
        &["/auth/google/login", "/auth/dev/login"]
    } else {
        &["/auth/google/login"]
    }
}

/// A navigation to a sign-in start route.
#[derive(Debug, PartialEq, Eq)]
pub struct SignIn {
    pub path: &'static str,
    /// The hub path to land on afterwards, as the page asked for it.
    pub next: Option<String>,
}

/// `Some` when `nav` is the webview following a "Sign in with …" link on
/// `hub`; those never load in the webview, the system sheet takes them.
pub fn sign_in_start(hub: &Url, nav: &Url) -> Option<SignIn> {
    if nav.origin() != hub.origin() {
        return None;
    }
    let path = start_paths().iter().find(|p| **p == nav.path())?;
    let next = nav
        .query_pairs()
        .find(|(k, _)| k == "next")
        .map(|(_, v)| v.into_owned());
    Some(SignIn { path, next })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hub() -> Url {
        Url::parse("https://hub.example.org").unwrap()
    }

    #[test]
    fn alice_taps_sign_in_with_google_and_the_sheet_takes_over() {
        let nav = Url::parse("https://hub.example.org/auth/google/login?next=%2Fv%2Fabc").unwrap();
        assert_eq!(
            sign_in_start(&hub(), &nav),
            Some(SignIn {
                path: "/auth/google/login",
                next: Some("/v/abc".into())
            })
        );
    }

    #[test]
    fn ordinary_hub_pages_load_in_the_webview() {
        for page in [
            "/",
            "/v/abc",
            "/auth/login",
            "/auth/mobile/complete?token=t&verifier=v",
        ] {
            let nav = hub().join(page).unwrap();
            assert_eq!(sign_in_start(&hub(), &nav), None, "{page}");
        }
    }

    #[test]
    fn another_site_with_the_same_path_is_not_the_hub() {
        let nav = Url::parse("https://evil.example.com/auth/google/login").unwrap();
        assert_eq!(sign_in_start(&hub(), &nav), None);
        let http = Url::parse("http://hub.example.org/auth/google/login").unwrap();
        assert_eq!(sign_in_start(&hub(), &http), None);
    }
}
