use crate::auth::Auth;
use crate::cli::TlsMode;
use crate::error::{Error, Result};
use async_imap::Session;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_rustls::rustls::pki_types::ServerName;
use tokio_rustls::rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;

pub struct Client {
    pub session: Session<TlsStream<TcpStream>>,
}

pub struct ConnectParams<'a> {
    pub host: &'a str,
    pub port: u16,
    pub tls: TlsMode,
    pub insecure: bool,
}

impl Client {
    pub async fn connect_and_auth(params: ConnectParams<'_>, auth: &Auth) -> Result<Self> {
        if params.tls != TlsMode::Imaps {
            return Err(Error::Config(format!(
                "TLS mode {:?} not yet implemented (only imaps in v0.1)",
                params.tls
            )));
        }
        let tcp = TcpStream::connect((params.host, params.port))
            .await
            .map_err(|e| Error::Network(format!("tcp connect: {e}")))?;
        let tls_config = build_tls_config(params.insecure)?;
        let connector = TlsConnector::from(Arc::new(tls_config));
        let server_name = ServerName::try_from(params.host.to_string())
            .map_err(|e| Error::Tls(format!("bad server name: {e}")))?;
        let tls = connector
            .connect(server_name, tcp)
            .await
            .map_err(|e| Error::Tls(format!("tls handshake: {e}")))?;

        let mut client = async_imap::Client::new(tls);
        // Consume the IMAP greeting (server sends * OK ... on connect)
        let _greeting = client
            .read_response()
            .await
            .ok_or_else(|| Error::Network("no IMAP greeting".into()))?
            .map_err(|e| Error::Network(format!("reading greeting: {e}")))?;

        let session = match auth {
            Auth::Login { user, password } => {
                client
                    .login(user, password)
                    .await
                    .map_err(|(e, _)| Error::Auth {
                        user: user.clone(),
                        reason: e.to_string(),
                    })?
            }
            Auth::XOAuth2 { user, access_token } => {
                authenticate_xoauth2(client, user, access_token).await?
            }
        };
        Ok(Self { session })
    }
}

fn build_tls_config(insecure: bool) -> Result<ClientConfig> {
    if insecure {
        let cfg = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
            .with_no_client_auth();
        return Ok(cfg);
    }
    let mut roots = RootCertStore::empty();
    roots
        .roots
        .extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    Ok(ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth())
}

#[derive(Debug)]
struct NoVerifier;

impl tokio_rustls::rustls::client::danger::ServerCertVerifier for NoVerifier {
    fn verify_server_cert(
        &self,
        _: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _: &[tokio_rustls::rustls::pki_types::CertificateDer<'_>],
        _: &tokio_rustls::rustls::pki_types::ServerName<'_>,
        _: &[u8],
        _: tokio_rustls::rustls::pki_types::UnixTime,
    ) -> std::result::Result<
        tokio_rustls::rustls::client::danger::ServerCertVerified,
        tokio_rustls::rustls::Error,
    > {
        Ok(tokio_rustls::rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> std::result::Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> std::result::Result<
        tokio_rustls::rustls::client::danger::HandshakeSignatureValid,
        tokio_rustls::rustls::Error,
    > {
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<tokio_rustls::rustls::SignatureScheme> {
        vec![
            tokio_rustls::rustls::SignatureScheme::RSA_PKCS1_SHA256,
            tokio_rustls::rustls::SignatureScheme::RSA_PKCS1_SHA384,
            tokio_rustls::rustls::SignatureScheme::RSA_PKCS1_SHA512,
            tokio_rustls::rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            tokio_rustls::rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
        ]
    }
}

pub struct FetchedMessage {
    pub body: Vec<u8>,
    pub internal_date: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub flags: Vec<String>,
    pub message_id: Option<String>,
}

impl Client {
    pub async fn list_folders(&mut self) -> Result<Vec<String>> {
        use futures::TryStreamExt;
        let names = self
            .session
            .list(Some(""), Some("*"))
            .await?
            .try_collect::<Vec<_>>()
            .await?
            .into_iter()
            .map(|n| n.name().to_string())
            .collect();
        Ok(names)
    }

    pub async fn logout(mut self) -> Result<()> {
        self.session.logout().await?;
        Ok(())
    }

    pub async fn select_for_write(&mut self, folder: &str) -> Result<()> {
        self.session.select(folder).await?;
        Ok(())
    }

    pub async fn examine(&mut self, folder: &str) -> Result<()> {
        self.session.examine(folder).await?;
        Ok(())
    }

    pub async fn create_folder_if_missing(&mut self, folder: &str) -> Result<()> {
        match self.session.create(folder).await {
            Ok(_) => Ok(()),
            Err(e) => {
                let msg = e.to_string().to_lowercase();
                if msg.contains("exists") {
                    Ok(())
                } else {
                    Err(Error::from(e))
                }
            }
        }
    }

    pub async fn search_all_uids(&mut self) -> Result<Vec<u32>> {
        let set = self.session.uid_search("ALL").await?;
        let mut v: Vec<u32> = set.into_iter().collect();
        v.sort_unstable();
        Ok(v)
    }

    pub async fn fetch_all_message_ids(&mut self) -> Result<std::collections::HashSet<String>> {
        use futures::TryStreamExt;
        let mut ids = std::collections::HashSet::new();
        let mut stream = self.session.uid_fetch("1:*", "BODY.PEEK[HEADER]").await?;
        while let Some(msg) = stream.try_next().await? {
            if let Some(header) = msg.header() {
                if let Some(mid) = parse_message_id(header) {
                    ids.insert(mid);
                }
            }
        }
        Ok(ids)
    }

    pub async fn fetch_full_by_uid(&mut self, uid: u32) -> Result<Option<FetchedMessage>> {
        use futures::TryStreamExt;
        let seq = format!("{uid}");
        let query = "(BODY.PEEK[] INTERNALDATE FLAGS BODY.PEEK[HEADER])";
        let mut stream = self.session.uid_fetch(seq, query).await?;
        if let Some(msg) = stream.try_next().await? {
            let body = msg.body().map(|b| b.to_vec()).unwrap_or_default();
            let internal_date = msg.internal_date();
            let flags: Vec<String> = msg.flags().map(flag_to_imap_string).collect();
            let message_id = msg.header().and_then(parse_message_id);
            return Ok(Some(FetchedMessage {
                body,
                internal_date,
                flags,
                message_id,
            }));
        }
        Ok(None)
    }

    pub async fn append_message(
        &mut self,
        folder: &str,
        body: &[u8],
        flags: &[String],
        internal_date: Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> Result<()> {
        let flag_str = if flags.is_empty() {
            None
        } else {
            Some(format!("({})", flags.join(" ")))
        };
        let date_str = internal_date.map(|dt| dt.format("%d-%b-%Y %H:%M:%S %z").to_string());
        self.session
            .append(folder, flag_str.as_deref(), date_str.as_deref(), body)
            .await?;
        Ok(())
    }
}

/// Convert an async-imap [`Flag`] to its IMAP wire string (e.g. `\Seen`).
fn flag_to_imap_string(flag: async_imap::types::Flag<'_>) -> String {
    use async_imap::types::Flag;
    match flag {
        Flag::Seen => r"\Seen".to_string(),
        Flag::Answered => r"\Answered".to_string(),
        Flag::Flagged => r"\Flagged".to_string(),
        Flag::Deleted => r"\Deleted".to_string(),
        Flag::Draft => r"\Draft".to_string(),
        Flag::Recent => r"\Recent".to_string(),
        Flag::MayCreate => r"\*".to_string(),
        Flag::Custom(s) => s.into_owned(),
    }
}

pub fn parse_message_id(header_bytes: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(header_bytes).ok()?;
    for line in s.lines() {
        if let Some(rest) = line
            .strip_prefix("Message-ID:")
            .or_else(|| line.strip_prefix("Message-Id:"))
            .or_else(|| line.strip_prefix("MESSAGE-ID:"))
        {
            return Some(
                rest.trim()
                    .trim_matches(|c| c == '<' || c == '>')
                    .to_string(),
            );
        }
    }
    None
}

async fn authenticate_xoauth2(
    client: async_imap::Client<TlsStream<TcpStream>>,
    user: &str,
    access_token: &str,
) -> Result<Session<TlsStream<TcpStream>>> {
    use base64::Engine as _;
    let raw = format!("user={user}\x01auth=Bearer {access_token}\x01\x01");
    let encoded = base64::engine::general_purpose::STANDARD.encode(raw);
    client
        .authenticate("XOAUTH2", Xoauth2Auth { token: encoded })
        .await
        .map_err(|(e, _)| Error::Auth {
            user: user.to_string(),
            reason: e.to_string(),
        })
}

struct Xoauth2Auth {
    token: String,
}

impl async_imap::Authenticator for Xoauth2Auth {
    type Response = String;
    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        std::mem::take(&mut self.token)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_message_id;

    #[test]
    fn extracts_message_id() {
        let h = b"Message-ID: <abc@host>\r\n";
        assert_eq!(parse_message_id(h).as_deref(), Some("abc@host"));
    }

    #[test]
    fn missing_message_id_returns_none() {
        let h = b"From: x@y\r\n";
        assert!(parse_message_id(h).is_none());
    }
}
