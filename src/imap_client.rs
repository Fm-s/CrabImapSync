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
            Auth::Login { user, password } => client
                .login(user, password)
                .await
                .map_err(|(e, _)| Error::Auth {
                    user: user.clone(),
                    reason: e.to_string(),
                })?,
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

/// XOAUTH2 stub — full implementation in Phase 7.
async fn authenticate_xoauth2(
    _client: async_imap::Client<TlsStream<TcpStream>>,
    user: &str,
    _access_token: &str,
) -> Result<Session<TlsStream<TcpStream>>> {
    Err(Error::Auth {
        user: user.to_string(),
        reason: "XOAUTH2 not yet implemented".into(),
    })
}
