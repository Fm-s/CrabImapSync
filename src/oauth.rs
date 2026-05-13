use crate::error::{Error, Result};
use std::str::FromStr;
use std::time::Duration;

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

/// Perform a PKCE browser flow (or refresh from keyring) and return OAuth2 credentials.
pub fn obtain_token(req: OAuthRequest<'_>) -> Result<OAuthCreds> {
    use oauth2::basic::BasicClient;
    use oauth2::reqwest;
    use oauth2::{
        AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
        RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
    };

    // 1. Keyring service key
    let provider_name = match &req.provider {
        Provider::Gmail => "gmail",
        Provider::Microsoft => "microsoft",
        Provider::Custom { .. } => "custom",
    };
    let service = format!("crabimap:{provider_name}");

    // Build the oauth2 BasicClient (used for both refresh and full flow)
    let auth_url = AuthUrl::new(req.provider.auth_url().to_string())
        .map_err(|e| Error::OAuth(format!("invalid auth URL: {e}")))?;
    let token_url = TokenUrl::new(req.provider.token_url().to_string())
        .map_err(|e| Error::OAuth(format!("invalid token URL: {e}")))?;

    let mut oauth_client = BasicClient::new(ClientId::new(req.client_id.to_string()))
        .set_auth_uri(auth_url)
        .set_token_uri(token_url);

    if let Some(secret) = req.client_secret {
        oauth_client = oauth_client.set_client_secret(ClientSecret::new(secret.to_string()));
    }

    // Build the blocking HTTP client (redirects disabled to avoid SSRF)
    let http_client = reqwest::blocking::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| Error::OAuth(format!("failed to build HTTP client: {e}")))?;

    // 2. Try keyring refresh token
    if req.use_keyring {
        if let Ok(entry) = keyring::Entry::new(&service, req.user) {
            if let Ok(stored_rt) = entry.get_password() {
                let refresh_token = RefreshToken::new(stored_rt);
                match oauth_client
                    .exchange_refresh_token(&refresh_token)
                    .request(&http_client)
                {
                    Ok(token_result) => {
                        let access_token = token_result.access_token().secret().clone();
                        let new_rt = token_result
                            .refresh_token()
                            .map(|t| t.secret().clone())
                            .or_else(|| Some(refresh_token.secret().clone()));

                        // Persist updated refresh token
                        if let Some(ref rt_val) = new_rt {
                            let _ = entry.set_password(rt_val);
                        }

                        return Ok(OAuthCreds {
                            access_token,
                            refresh_token: new_rt,
                        });
                    }
                    Err(e) => {
                        eprintln!(
                            "Stored refresh token failed ({}), starting browser flow…",
                            e
                        );
                        // Fall through to full browser flow
                    }
                }
            }
        }
    }

    // 3. Bind a local server on a random port
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|e| Error::OAuth(format!("failed to bind local server: {e}")))?;
    let port = server
        .server_addr()
        .to_ip()
        .ok_or_else(|| Error::OAuth("could not get server address".into()))?
        .port();

    // 4. Redirect URL
    let redirect_uri = format!("http://127.0.0.1:{port}/cb");
    let redirect_url = RedirectUrl::new(redirect_uri.clone())
        .map_err(|e| Error::OAuth(format!("invalid redirect URL: {e}")))?;

    let oauth_client = oauth_client.set_redirect_uri(redirect_url);

    // 5-7. Build authorize URL with PKCE
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();

    // Build scopes — each whitespace-separated token is its own scope
    let scope_str = req.provider.default_scope().to_string();
    let scopes: Vec<Scope> = scope_str
        .split_whitespace()
        .map(|s| Scope::new(s.to_string()))
        .collect();

    let mut auth_request = oauth_client.authorize_url(CsrfToken::new_random);
    for scope in scopes {
        auth_request = auth_request.add_scope(scope);
    }
    let (auth_url, csrf_token) = auth_request.set_pkce_challenge(pkce_challenge).url();

    let url_str = auth_url.to_string();

    // 8. Open browser or print URL
    println!("Opening browser for OAuth2 authorization…");
    if webbrowser::open(&url_str).is_err() {
        println!("Open this URL manually:\n{url_str}");
    } else {
        println!("If the browser did not open, visit:\n{url_str}");
    }

    // 9. Wait up to 5 minutes for the callback
    let request = server
        .recv_timeout(Duration::from_secs(300))
        .map_err(|e| Error::OAuth(format!("server error waiting for callback: {e}")))?
        .ok_or_else(|| Error::OAuth("timed out waiting for OAuth2 callback (5 min)".into()))?;

    // Parse query params from the request URL
    let raw_url = format!("http://127.0.0.1{}", request.url());
    let parsed =
        url::Url::parse(&raw_url).map_err(|e| Error::OAuth(format!("bad callback URL: {e}")))?;

    let mut code_param: Option<String> = None;
    let mut state_param: Option<String> = None;
    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "code" => code_param = Some(value.into_owned()),
            "state" => state_param = Some(value.into_owned()),
            _ => {}
        }
    }

    // Respond to the browser
    let html = "<html><body><h1>Authentication successful!</h1><p>You may close this tab.</p></body></html>";
    let response = tiny_http::Response::from_string(html)
        .with_header(
            "Content-Type: text/html"
                .parse::<tiny_http::Header>()
                .unwrap(),
        );
    let _ = request.respond(response);

    let code = code_param.ok_or_else(|| Error::OAuth("no code in callback".into()))?;
    let state = state_param.ok_or_else(|| Error::OAuth("no state in callback".into()))?;

    // 10. Verify CSRF state
    if state != *csrf_token.secret() {
        return Err(Error::OAuth("CSRF state mismatch".into()));
    }

    // 11. Exchange code for tokens
    let token_result = oauth_client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(pkce_verifier)
        .request(&http_client)
        .map_err(|e| Error::OAuth(format!("token exchange failed: {e}")))?;

    let access_token = token_result.access_token().secret().clone();
    let refresh_token = token_result.refresh_token().map(|t| t.secret().clone());

    // 12. Persist refresh token in keyring
    if req.use_keyring {
        if let Some(ref rt) = refresh_token {
            if let Ok(entry) = keyring::Entry::new(&service, req.user) {
                let _ = entry.set_password(rt);
            }
        }
    }

    // 13. Return credentials
    Ok(OAuthCreds {
        access_token,
        refresh_token,
    })
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
