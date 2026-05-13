use crate::error::{Error, Result};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provider {
    Gmail,
    Microsoft,
    Custom {
        auth_url: String,
        token_url: String,
        scope: String,
    },
}

impl Provider {
    pub fn auth_url(&self) -> &str {
        match self {
            Self::Gmail => "https://accounts.google.com/o/oauth2/v2/auth",
            Self::Microsoft => "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
            Self::Custom { auth_url, .. } => auth_url,
        }
    }
    pub fn token_url(&self) -> &str {
        match self {
            Self::Gmail => "https://oauth2.googleapis.com/token",
            Self::Microsoft => "https://login.microsoftonline.com/common/oauth2/v2.0/token",
            Self::Custom { token_url, .. } => token_url,
        }
    }
    pub fn default_scope(&self) -> &str {
        match self {
            Self::Gmail => "https://mail.google.com/",
            Self::Microsoft => {
                "https://outlook.office.com/IMAP.AccessAsUser.All offline_access"
            }
            Self::Custom { scope, .. } => scope,
        }
    }
}

impl FromStr for Provider {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "gmail" => Ok(Self::Gmail),
            "microsoft" => Ok(Self::Microsoft),
            other => Err(Error::Config(format!(
                "unknown OAuth provider '{other}' (use gmail|microsoft|custom)"
            ))),
        }
    }
}

pub struct OAuthCreds {
    pub access_token: String,
    pub refresh_token: Option<String>,
}

pub struct OAuthRequest<'a> {
    pub provider: Provider,
    pub user: &'a str,
    pub client_id: &'a str,
    pub client_secret: Option<&'a str>,
    pub use_keyring: bool,
}

/// Stub — full PKCE browser flow lands in Task 19.
pub fn obtain_token(_req: OAuthRequest<'_>) -> Result<OAuthCreds> {
    Err(Error::OAuth(
        "OAuth2 browser flow not yet implemented (coming in Task 19)".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gmail_urls() {
        let p = Provider::Gmail;
        assert!(p.auth_url().starts_with("https://accounts.google.com"));
        assert!(p.default_scope().contains("mail.google.com"));
    }

    #[test]
    fn unknown_provider_errors() {
        assert!(Provider::from_str("yahoo").is_err());
    }
}
