use crate::config::AuthMethod;

#[derive(Debug, Clone)]
pub enum Auth {
    Login { user: String, password: String },
    XOAuth2 { user: String, access_token: String },
}

impl Auth {
    pub fn login(user: impl Into<String>, password: impl Into<String>) -> Self {
        Self::Login {
            user: user.into(),
            password: password.into(),
        }
    }

    pub fn user(&self) -> &str {
        match self {
            Self::Login { user, .. } | Self::XOAuth2 { user, .. } => user,
        }
    }
}

/// Build an Auth from an EndpointSettings.user + AuthMethod. Returns None for
/// OAuth2 — that path is async and handled separately by oauth::resolve.
pub fn from_login(user: &str, method: &AuthMethod) -> Option<Auth> {
    match method {
        AuthMethod::Login { password } => Some(Auth::login(user, password)),
        AuthMethod::OAuth2 { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_login_auth() {
        let m = AuthMethod::Login {
            password: "p".into(),
        };
        let a = from_login("u", &m).unwrap();
        assert!(matches!(a, Auth::Login { .. }));
        assert_eq!(a.user(), "u");
    }

    #[test]
    fn oauth_returns_none_in_sync_resolver() {
        let m = AuthMethod::OAuth2 {
            provider_kind: "gmail".into(),
            client_id: "id".into(),
            client_secret: None,
            auth_url: None,
            token_url: None,
            scope: None,
            use_keyring: true,
        };
        assert!(from_login("u", &m).is_none());
    }
}
