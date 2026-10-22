//! `GET /api/v0/auth/providers` — the sign-in options this hub offers.
//!
//! The SPA renders one landing page for anonymous visitors from this
//! list: a button per OAuth provider (Google), the dev-trust
//! email form when that's on. Each provider's own routes live
//! under `/auth/<provider>/`.

use axum::extract::State;
use axum::Json;
// Shared wire types — the SPA deserializes the same `AuthProviders`.
use zim_api::hub::{AuthProviders, SignInOption};

use crate::config::{AuthConfig, AuthProvider};
use crate::state::AppState;

pub async fn handler(State(state): State<AppState>) -> Json<AuthProviders> {
    Json(AuthProviders {
        providers: sign_in_options(&state.auth),
    })
}

fn sign_in_options(auth: &AuthConfig) -> Vec<SignInOption> {
    auth.providers
        .iter()
        .map(|provider| match provider {
            AuthProvider::Google(_) => SignInOption::OAuth {
                id: provider.name().to_string(),
                label: "Sign in with Google".to_string(),
                path: "/auth/google/login".to_string(),
            },
            #[cfg(feature = "dev-login")]
            AuthProvider::DevTrust => SignInOption::DevTrust {
                action: "/auth/dev/login".to_string(),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::GoogleOAuth;

    fn hub(providers: Vec<AuthProvider>) -> AuthConfig {
        AuthConfig {
            host_name: "http://127.0.0.1:17190".into(),
            admin_emails: vec!["alice@example.com".into()],
            providers,
            service_secret: "secret".into(),
        }
    }

    fn google() -> AuthProvider {
        AuthProvider::Google(GoogleOAuth {
            client_id: "id".into(),
            client_secret: "secret".into(),
        })
    }

    #[test]
    fn google_is_offered_as_a_button_to_the_oauth_handshake() {
        let options = sign_in_options(&hub(vec![google()]));
        assert_eq!(
            options,
            [SignInOption::OAuth {
                id: "google".into(),
                label: "Sign in with Google".into(),
                path: "/auth/google/login".into(),
            }]
        );
    }

    #[cfg(feature = "dev-login")]
    #[test]
    fn dev_hub_offers_both_in_configured_order() {
        let options = sign_in_options(&hub(vec![AuthProvider::DevTrust, google()]));

        assert_eq!(
            options[0],
            SignInOption::DevTrust {
                action: "/auth/dev/login".into(),
            }
        );
        assert!(matches!(&options[1], SignInOption::OAuth { id, .. } if id == "google"));
    }
}
