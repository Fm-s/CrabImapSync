use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("configuration error: {0}")]
    Config(String),

    #[error("authentication failed for {user}: {reason}")]
    Auth { user: String, reason: String },

    #[error("IMAP error: {0}")]
    Imap(#[from] async_imap::error::Error),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("network error: {0}")]
    Network(String),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("OAuth2 error: {0}")]
    OAuth(String),

    #[error("message append failed for folder {folder} uid {uid}: {reason}")]
    Append {
        folder: String,
        uid: u32,
        reason: String,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_error_formats() {
        let e = Error::Auth {
            user: "user@example.com".into(),
            reason: "invalid creds".into(),
        };
        assert_eq!(
            e.to_string(),
            "authentication failed for user@example.com: invalid creds"
        );
    }
}
