//! The timings the hub hands the app in `index.html` (see
//! [`ClientConfig`]), parsed with a fallback: the desktop app loads this
//! SPA from local files, not a hub, so there may be no tag at all.

use zim_api::hub::ClientConfig;

/// The config from the `#zim-config` tag's text, or the built-in
/// defaults when the tag is missing or unreadable — a bad tag must never
/// stop the app from booting.
pub fn parse(tag_text: Option<&str>) -> ClientConfig {
    tag_text
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default()
}

/// `ms` as a `setTimeout` delay, which takes an `i32`.
pub fn timeout_ms(ms: u32) -> i32 {
    i32::try_from(ms).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alice_on_the_desktop_app_gets_the_default_timings() {
        // The desktop app serves the SPA itself: no hub, no config tag.
        let config = parse(None);

        // Alice saves every 30s and follows every 3s, as in production.
        assert_eq!(config, ClientConfig::default());
        assert_eq!(config.autosave_ms, 30_000);
        assert_eq!(config.follow_ms, 3_000);
    }

    #[test]
    fn bob_on_a_dev_hub_gets_the_hubs_timings() {
        let config = parse(Some(r#"{"autosave_ms":1500,"follow_ms":1000}"#));

        assert_eq!(config.autosave_ms, 1_500);
        assert_eq!(config.follow_ms, 1_000);
    }

    #[test]
    fn a_garbled_tag_falls_back_rather_than_breaking_the_app() {
        assert_eq!(parse(Some("not json")), ClientConfig::default());
    }

    #[test]
    fn a_tag_missing_a_field_keeps_that_fields_default() {
        let config = parse(Some(r#"{"follow_ms":1000}"#));

        assert_eq!(config.autosave_ms, ClientConfig::DEFAULT_AUTOSAVE_MS);
        assert_eq!(config.follow_ms, 1_000);
    }

    #[test]
    fn an_absurd_delay_saturates_instead_of_wrapping_negative() {
        assert_eq!(timeout_ms(1_500), 1_500);
        assert_eq!(timeout_ms(u32::MAX), i32::MAX);
    }
}
