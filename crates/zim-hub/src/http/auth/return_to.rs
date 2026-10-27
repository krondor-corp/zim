//! `next` — where to land after signing in. Carried from the page that
//! bounced an anonymous visitor to `/auth/login` through whichever
//! provider they pick, then redirected to once the session is minted.
//!
//! Always a same-origin path: anything that could make a browser leave
//! the hub (`//evil.com`, `https://…`, `/\evil.com`, or control
//! characters browsers strip into one of those) collapses to `/`.

use super::LOGIN_PATH;

/// `next` if it's a safe same-origin path, else `/`.
pub fn sanitize(next: Option<&str>) -> &str {
    match next {
        Some(n) if is_same_origin_path(n) => n,
        _ => "/",
    }
}

/// The sign-in page, returning to `next` afterwards.
pub fn login_url(next: &str) -> String {
    match sanitize(Some(next)) {
        "/" => LOGIN_PATH.to_string(),
        next => format!("{LOGIN_PATH}?next={}", urlencoding::encode(next)),
    }
}

fn is_same_origin_path(n: &str) -> bool {
    let mut chars = n.chars();
    chars.next() == Some('/')
        && !matches!(chars.next(), Some('/') | Some('\\'))
        && !n
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deep_links_survive_sign_in() {
        assert_eq!(sanitize(Some("/v/abc")), "/v/abc");
        assert_eq!(
            sanitize(Some("/auth/device?code=XYZ-123")),
            "/auth/device?code=XYZ-123"
        );
    }

    #[test]
    fn missing_or_empty_next_lands_home() {
        assert_eq!(sanitize(None), "/");
        assert_eq!(sanitize(Some("")), "/");
    }

    #[test]
    fn mallory_cannot_bounce_alice_off_the_hub() {
        for evil in [
            "https://evil.com",
            "//evil.com",
            "/\\evil.com",
            "/\t/evil.com",
            "/\n/evil.com",
            "evil.com",
            "javascript:alert(1)",
        ] {
            assert_eq!(sanitize(Some(evil)), "/", "{evil:?}");
        }
    }

    #[test]
    fn login_url_carries_next_only_when_it_matters() {
        assert_eq!(login_url("/"), "/auth/login");
        assert_eq!(login_url("//evil.com"), "/auth/login");
        assert_eq!(
            login_url("/auth/device?code=XYZ"),
            "/auth/login?next=%2Fauth%2Fdevice%3Fcode%3DXYZ"
        );
    }
}
