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
    pub host: String,
    pub port: u16,
    pub tls: TlsMode,
    pub insecure: bool,
    pub auth: Auth,
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
        Ok(Self {
            session,
            host: params.host.to_string(),
            port: params.port,
            tls: params.tls,
            insecure: params.insecure,
            auth: auth.clone(),
        })
    }

    /// Re-establish the TCP+TLS connection and re-authenticate using the stored
    /// credentials.  Replaces `self.session` in-place so all other stored fields
    /// (host, port, …) are preserved.
    pub async fn reconnect(&mut self) -> Result<()> {
        tracing::warn!(host = %self.host, "reconnecting IMAP session");
        let new = Self::connect_and_auth(
            ConnectParams {
                host: &self.host,
                port: self.port,
                tls: self.tls,
                insecure: self.insecure,
            },
            &self.auth,
        )
        .await?;
        self.session = new.session;
        Ok(())
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
        use tokio_rustls::rustls::SignatureScheme as S;
        vec![
            S::RSA_PKCS1_SHA256,
            S::RSA_PKCS1_SHA384,
            S::RSA_PKCS1_SHA512,
            S::RSA_PSS_SHA256,
            S::RSA_PSS_SHA384,
            S::RSA_PSS_SHA512,
            S::ECDSA_NISTP256_SHA256,
            S::ECDSA_NISTP384_SHA384,
            S::ECDSA_NISTP521_SHA512,
            S::ED25519,
            S::ED448,
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
        use async_imap::types::NameAttribute;
        use futures::TryStreamExt;
        let names = self
            .session
            .list(Some(""), Some("*"))
            .await?
            .try_collect::<Vec<_>>()
            .await?
            .into_iter()
            .filter(|n| {
                !n.attributes()
                    .iter()
                    .any(|a| matches!(a, NameAttribute::NoSelect))
            })
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
        // Use the narrower HEADER.FIELDS form to avoid downloading entire headers.
        // imap-proto 0.16 maps both BODY[HEADER] and BODY[HEADER.FIELDS (...)] to
        // SectionPath::Full(MessageSection::Header), so msg.header() still works.
        let mut stream = self
            .session
            .uid_fetch("1:*", "BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)]")
            .await?;
        while let Some(msg) = stream.try_next().await? {
            if let Some(header) = msg.header() {
                if let Some(mid) = parse_message_id(header) {
                    ids.insert(mid);
                }
            }
        }
        Ok(ids)
    }

    /// Batch-fetch Message-Id headers for the given UIDs.
    /// Returns a map UID → Message-Id. UIDs whose response lacked a parseable
    /// Message-Id header are absent from the map.
    pub async fn fetch_message_ids_for_uids(
        &mut self,
        uids: &[u32],
    ) -> Result<std::collections::HashMap<u32, String>> {
        use futures::TryStreamExt;
        let mut out = std::collections::HashMap::with_capacity(uids.len());
        if uids.is_empty() {
            return Ok(out);
        }
        // Build a comma-separated UID set for the IMAP UID FETCH command.
        let seq = uids
            .iter()
            .map(|u| u.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let messages: Vec<_> = self
            .session
            .uid_fetch(seq, "BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)]")
            .await?
            .try_collect()
            .await?;
        for msg in messages {
            let uid = msg.uid;
            if let (Some(uid), Some(hdr)) = (uid, msg.header()) {
                if let Some(mid) = parse_message_id(hdr) {
                    out.insert(uid, mid);
                }
            }
        }
        Ok(out)
    }

    pub async fn fetch_full_by_uid(&mut self, uid: u32) -> Result<Option<FetchedMessage>> {
        use futures::TryStreamExt;
        let seq = format!("{uid}");
        let query = "(BODY.PEEK[] INTERNALDATE FLAGS BODY.PEEK[HEADER])";
        let messages: Vec<_> = self
            .session
            .uid_fetch(seq, query)
            .await?
            .try_collect()
            .await?;
        if let Some(msg) = messages.first() {
            let body = msg.body().map(|b| b.to_vec()).unwrap_or_default();
            let internal_date = msg.internal_date();
            let flags: Vec<String> = msg.flags().filter_map(flag_to_imap_string).collect();
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
///
/// Returns `None` for flags that are not valid to include in an APPEND command:
/// - `\Recent` is server-assigned and MUST NOT be set by a client.
/// - `\*` (MayCreate) is a special indicator, not an appendable flag.
fn flag_to_imap_string(flag: async_imap::types::Flag<'_>) -> Option<String> {
    use async_imap::types::Flag;
    match flag {
        Flag::Seen => Some(r"\Seen".to_string()),
        Flag::Answered => Some(r"\Answered".to_string()),
        Flag::Flagged => Some(r"\Flagged".to_string()),
        Flag::Deleted => Some(r"\Deleted".to_string()),
        Flag::Draft => Some(r"\Draft".to_string()),
        Flag::Recent | Flag::MayCreate => None,
        Flag::Custom(s) => Some(s.into_owned()),
    }
}

pub fn parse_message_id(header_bytes: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(header_bytes).ok()?;

    // Unfold per RFC 5322: continuation lines start with whitespace and belong
    // to the previous header line.
    let mut unfolded: Vec<String> = Vec::new();
    for raw in s.split("\r\n") {
        if raw.is_empty() {
            continue;
        }
        let is_continuation = raw.starts_with(' ') || raw.starts_with('\t');
        if is_continuation {
            if let Some(last) = unfolded.last_mut() {
                last.push(' ');
                last.push_str(raw.trim_start());
                continue;
            }
        }
        unfolded.push(raw.to_string());
    }

    const KEY: &str = "message-id:";
    for line in &unfolded {
        let trimmed = line.trim_start();
        if trimmed.len() < KEY.len() {
            continue;
        }
        if !trimmed[..KEY.len()].eq_ignore_ascii_case(KEY) {
            continue;
        }
        let value = trimmed[KEY.len()..]
            .trim()
            .trim_matches(|c| c == '<' || c == '>')
            .trim();
        if value.is_empty() {
            continue;
        }
        return Some(value.to_string());
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

    #[test]
    fn handles_folded_message_id() {
        let h = b"Message-ID:\r\n <folded@host>\r\n";
        assert_eq!(parse_message_id(h).as_deref(), Some("folded@host"));
    }

    #[test]
    fn rejects_empty_message_id() {
        assert!(parse_message_id(b"Message-ID: <>\r\n").is_none());
        assert!(parse_message_id(b"Message-ID:\r\n").is_none());
        assert!(parse_message_id(b"Message-ID:   \r\n").is_none());
    }

    #[test]
    fn case_insensitive_header_name() {
        assert_eq!(
            parse_message_id(b"message-id: <a@b>\r\n").as_deref(),
            Some("a@b")
        );
        assert_eq!(
            parse_message_id(b"MESSAGE-ID: <a@b>\r\n").as_deref(),
            Some("a@b")
        );
    }
}
