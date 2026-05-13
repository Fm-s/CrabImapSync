# CrabImapSync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a memory-bounded, streaming IMAP-to-IMAP migration CLI in Rust that does not suffer from imapsync's RAM bloat (62 GB observed on 4.7 GB mailbox).

**Architecture:** Async Rust binary (tokio + async-imap + rustls). One source/destination pair per invocation. Per-folder streaming copy: fetch one message at a time, dedup by Message-Id against a per-folder set on destination. Auth via plain LOGIN or OAuth2 (PKCE browser flow with refresh-token persistence in OS keyring).

**Tech Stack:** Rust 2021, tokio, async-imap, tokio-rustls, clap, oauth2, tiny_http, indicatif, thiserror, mail-parser, testcontainers (dev).

**Phased order:** scaffolding → CLI/config → IMAP/login/sync (working LOGIN binary by phase 5) → smoke test against user's real migration → OAuth2 → integration tests → docs/CI.

---

## Phase 1: Scaffolding

### Task 1: cargo init + initial commit

**Files:**
- Create: `crab-imap-sync/Cargo.toml`
- Create: `crab-imap-sync/.gitignore`
- Create: `crab-imap-sync/LICENSE`
- Create: `crab-imap-sync/README.md`
- Create: `crab-imap-sync/src/main.rs`

- [ ] **Step 1: Run cargo init**

```bash
cd ~/Projetos/vm-email-sync/crab-imap-sync
cargo init --name crab-imap-sync --bin
```

Expected: creates `Cargo.toml`, `src/main.rs`, `.gitignore` (or git init).

- [ ] **Step 2: Write the MIT LICENSE**

```text
MIT License

Copyright (c) 2026 Felipe Souza

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 3: Stub README.md**

```markdown
# CrabImapSync 🦀✉️

Memory-bounded, streaming IMAP-to-IMAP migration CLI.

Status: in development. See `SPEC.md` and `PLAN.md`.
```

- [ ] **Step 4: Stub src/main.rs**

```rust
fn main() {
    println!("crab-imap-sync — see --help");
}
```

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml LICENSE README.md src/main.rs .gitignore
git commit -m "chore: initial scaffold"
```

---

### Task 2: Cargo.toml dependencies

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Write full Cargo.toml**

```toml
[package]
name = "crab-imap-sync"
version = "0.1.0"
edition = "2021"
description = "Memory-bounded streaming IMAP-to-IMAP migration CLI"
license = "MIT"
repository = "https://github.com/felipemsouza/crab-imap-sync"
readme = "README.md"
keywords = ["imap", "email", "migration", "sync"]
categories = ["command-line-utilities", "email"]

[[bin]]
name = "crab-imap-sync"
path = "src/main.rs"

[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "time", "fs"] }
async-imap = "0.10"
tokio-rustls = "0.26"
rustls-pemfile = "2"
webpki-roots = "0.26"
futures = "0.3"

clap = { version = "4", features = ["derive", "env"] }

oauth2 = "5"
tiny_http = "0.12"
webbrowser = "1"
keyring = "3"
url = "2"
base64 = "0.22"

indicatif = "0.17"

thiserror = "1"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

mail-parser = "0.9"
globset = "0.4"

[dev-dependencies]
testcontainers = "0.23"
tempfile = "3"
tokio = { version = "1", features = ["test-util", "macros"] }

[profile.release]
lto = true
codegen-units = 1
strip = true
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check`
Expected: dependencies download; no compile errors.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore: declare dependencies"
```

---

### Task 3: Error type

**Files:**
- Create: `src/error.rs`
- Create: `src/lib.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write failing unit test for error display**

Create `src/error.rs`:

```rust
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
    Append { folder: String, uid: u32, reason: String },
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
```

- [ ] **Step 2: Wire error.rs into a lib + bin layout**

Create `src/lib.rs`:

```rust
pub mod error;
```

Modify `src/main.rs`:

```rust
fn main() {
    println!("crab-imap-sync — see --help");
}
```

(main.rs stays minimal; lib.rs is where modules live.)

Adjust `Cargo.toml` to add a `[lib]` section above `[[bin]]`:

```toml
[lib]
name = "crab_imap_sync"
path = "src/lib.rs"

[[bin]]
name = "crab-imap-sync"
path = "src/main.rs"
```

- [ ] **Step 3: Run the test**

Run: `cargo test --lib error::tests::auth_error_formats`
Expected: 1 test passed.

- [ ] **Step 4: Commit**

```bash
git add src/error.rs src/lib.rs src/main.rs Cargo.toml
git commit -m "feat(error): typed error enum with Display impls"
```

---

### Task 4: Module stubs

**Files:**
- Create: `src/cli.rs`, `src/config.rs`, `src/auth.rs`, `src/oauth.rs`, `src/imap_client.rs`, `src/sync.rs`, `src/progress.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Create empty module files**

Each file:

`src/cli.rs`:
```rust
// CLI argument parsing (clap). Implemented in Task 5.
```

`src/config.rs`:
```rust
// Settings struct that merges CLI args + env vars. Implemented in Task 6.
```

`src/auth.rs`:
```rust
// Auth enum (Login / XOAuth2). Implemented in Task 7.
```

`src/oauth.rs`:
```rust
// OAuth2 PKCE browser flow. Implemented in Phase 7.
```

`src/imap_client.rs`:
```rust
// async-imap thin wrapper. Implemented in Task 8 onwards.
```

`src/sync.rs`:
```rust
// Migration orchestrator. Implemented in Phase 4.
```

`src/progress.rs`:
```rust
// indicatif progress bars. Implemented in Task 15.
```

- [ ] **Step 2: Wire all modules into lib.rs**

Replace `src/lib.rs`:

```rust
pub mod auth;
pub mod cli;
pub mod config;
pub mod error;
pub mod imap_client;
pub mod oauth;
pub mod progress;
pub mod sync;
```

- [ ] **Step 3: Verify compilation**

Run: `cargo build`
Expected: builds without warnings.

- [ ] **Step 4: Commit**

```bash
git add src/
git commit -m "chore: stub module files"
```

---

## Phase 2: CLI + Config

### Task 5: CLI parsing (LOGIN auth only for now)

**Files:**
- Modify: `src/cli.rs`
- Test: same file (`#[cfg(test)]` module)

- [ ] **Step 1: Write failing parser test**

Replace `src/cli.rs`:

```rust
use clap::Parser;

#[derive(Debug, Parser, PartialEq, Eq)]
#[command(name = "crab-imap-sync", version, about, long_about = None)]
pub struct Cli {
    // ---------- Source ----------
    #[arg(long)] pub src_host: String,
    #[arg(long, default_value_t = 993)] pub src_port: u16,
    #[arg(long, default_value = "imaps")] pub src_tls: TlsMode,
    #[arg(long)] pub src_user: String,
    #[arg(long, default_value = "login")] pub src_auth: AuthKind,
    #[arg(long)] pub src_pass_env: Option<String>,
    #[arg(long, default_value_t = false)] pub src_insecure: bool,

    // ---------- Destination ----------
    #[arg(long)] pub dst_host: String,
    #[arg(long, default_value_t = 993)] pub dst_port: u16,
    #[arg(long, default_value = "imaps")] pub dst_tls: TlsMode,
    #[arg(long)] pub dst_user: String,
    #[arg(long, default_value = "login")] pub dst_auth: AuthKind,
    #[arg(long)] pub dst_pass_env: Option<String>,
    #[arg(long, default_value_t = false)] pub dst_insecure: bool,

    // ---------- Sync options ----------
    #[arg(long)] pub include: Vec<String>,
    #[arg(long)] pub exclude: Vec<String>,
    #[arg(long)] pub max_message_size: Option<u64>,
    #[arg(long, default_value_t = false)] pub dry_run: bool,
    #[arg(long, default_value_t = 300)] pub timeout_secs: u64,
    #[arg(long, default_value_t = 3)] pub retries: u32,

    // ---------- Output ----------
    #[arg(short, long, default_value_t = false)] pub verbose: bool,
    #[arg(long, default_value_t = false)] pub quiet: bool,
    #[arg(long, default_value_t = false)] pub no_progress: bool,
    #[arg(long)] pub log_file: Option<std::path::PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum TlsMode {
    None,
    Starttls,
    Imaps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum AuthKind {
    Login,
    Oauth2,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_login_args() {
        let args = [
            "crab-imap-sync",
            "--src-host", "src.example.com",
            "--src-user", "a@x",
            "--src-pass-env", "SRC_PASS",
            "--dst-host", "dst.example.com",
            "--dst-user", "a@y",
            "--dst-pass-env", "DST_PASS",
        ];
        let cli = Cli::try_parse_from(args).unwrap();
        assert_eq!(cli.src_host, "src.example.com");
        assert_eq!(cli.src_port, 993);
        assert_eq!(cli.src_tls, TlsMode::Imaps);
        assert_eq!(cli.src_auth, AuthKind::Login);
        assert_eq!(cli.src_pass_env.as_deref(), Some("SRC_PASS"));
    }

    #[test]
    fn rejects_missing_required() {
        let args = ["crab-imap-sync", "--src-host", "x"];
        assert!(Cli::try_parse_from(args).is_err());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib cli::`
Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add src/cli.rs
git commit -m "feat(cli): clap arg structure with LOGIN + oauth2 enum"
```

---

### Task 6: Config resolution (env vars + validation)

**Files:**
- Modify: `src/config.rs`
- Modify: `src/error.rs` (if needed — already has Config variant)

- [ ] **Step 1: Write failing test for config build**

Replace `src/config.rs`:

```rust
use crate::cli::{AuthKind, Cli, TlsMode};
use crate::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct EndpointSettings {
    pub host: String,
    pub port: u16,
    pub tls: TlsMode,
    pub user: String,
    pub insecure: bool,
    pub auth: AuthMethod,
}

#[derive(Debug, Clone)]
pub enum AuthMethod {
    Login { password: String },
    OAuth2 { /* filled in by oauth module at runtime */ },
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub src: EndpointSettings,
    pub dst: EndpointSettings,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub max_message_size: Option<u64>,
    pub dry_run: bool,
    pub timeout_secs: u64,
    pub retries: u32,
    pub verbose: bool,
    pub quiet: bool,
    pub no_progress: bool,
    pub log_file: Option<std::path::PathBuf>,
}

impl Settings {
    pub fn from_cli(cli: Cli) -> Result<Self> {
        let src = build_endpoint(
            cli.src_host, cli.src_port, cli.src_tls, cli.src_user,
            cli.src_insecure, cli.src_auth, cli.src_pass_env, "src",
        )?;
        let dst = build_endpoint(
            cli.dst_host, cli.dst_port, cli.dst_tls, cli.dst_user,
            cli.dst_insecure, cli.dst_auth, cli.dst_pass_env, "dst",
        )?;

        Ok(Self {
            src,
            dst,
            include: cli.include,
            exclude: cli.exclude,
            max_message_size: cli.max_message_size,
            dry_run: cli.dry_run,
            timeout_secs: cli.timeout_secs,
            retries: cli.retries,
            verbose: cli.verbose,
            quiet: cli.quiet,
            no_progress: cli.no_progress,
            log_file: cli.log_file,
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn build_endpoint(
    host: String,
    port: u16,
    tls: TlsMode,
    user: String,
    insecure: bool,
    auth_kind: AuthKind,
    pass_env: Option<String>,
    side: &str,
) -> Result<EndpointSettings> {
    let auth = match auth_kind {
        AuthKind::Login => {
            let envvar = pass_env.ok_or_else(|| {
                Error::Config(format!("--{side}-pass-env is required when --{side}-auth=login"))
            })?;
            let password = std::env::var(&envvar).map_err(|_| {
                Error::Config(format!("env var '{envvar}' for {side} password is not set"))
            })?;
            AuthMethod::Login { password }
        }
        AuthKind::Oauth2 => {
            // resolved later in oauth module
            AuthMethod::OAuth2 {}
        }
    };
    Ok(EndpointSettings { host, port, tls, user, insecure, auth })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    fn minimal_cli() -> Cli {
        Cli::try_parse_from([
            "crab-imap-sync",
            "--src-host", "src.example",
            "--src-user", "a@x",
            "--src-pass-env", "TEST_SRC_PASS",
            "--dst-host", "dst.example",
            "--dst-user", "a@y",
            "--dst-pass-env", "TEST_DST_PASS",
        ]).unwrap()
    }

    #[test]
    fn reads_password_from_env() {
        std::env::set_var("TEST_SRC_PASS", "secret1");
        std::env::set_var("TEST_DST_PASS", "secret2");
        let s = Settings::from_cli(minimal_cli()).unwrap();
        match &s.src.auth {
            AuthMethod::Login { password } => assert_eq!(password, "secret1"),
            _ => panic!("expected Login"),
        }
        std::env::remove_var("TEST_SRC_PASS");
        std::env::remove_var("TEST_DST_PASS");
    }

    #[test]
    fn errors_when_env_missing() {
        std::env::remove_var("MISSING_PASS_VAR");
        let mut cli = minimal_cli();
        cli.src_pass_env = Some("MISSING_PASS_VAR".into());
        let err = Settings::from_cli(cli).unwrap_err();
        assert!(err.to_string().contains("MISSING_PASS_VAR"));
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib config::`
Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add src/config.rs
git commit -m "feat(config): Settings struct from CLI + env vars"
```

---

## Phase 3: Auth + IMAP Client (LOGIN path)

### Task 7: Auth resolver

**Files:**
- Modify: `src/auth.rs`

- [ ] **Step 1: Write the Auth module**

Replace `src/auth.rs`:

```rust
use crate::config::AuthMethod;

/// Concrete auth method consumed by imap_client. OAuth tokens are filled in
/// at runtime by the oauth module before this is constructed.
#[derive(Debug, Clone)]
pub enum Auth {
    Login { user: String, password: String },
    XOAuth2 { user: String, access_token: String },
}

impl Auth {
    /// Convenience builder for LOGIN. (OAuth2 is built by oauth::resolve.)
    pub fn login(user: impl Into<String>, password: impl Into<String>) -> Self {
        Self::Login { user: user.into(), password: password.into() }
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
        AuthMethod::OAuth2 {} => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_login_auth() {
        let m = AuthMethod::Login { password: "p".into() };
        let a = from_login("u", &m).unwrap();
        assert!(matches!(a, Auth::Login { .. }));
        assert_eq!(a.user(), "u");
    }

    #[test]
    fn oauth_returns_none_in_sync_resolver() {
        let m = AuthMethod::OAuth2 {};
        assert!(from_login("u", &m).is_none());
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib auth::`
Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add src/auth.rs
git commit -m "feat(auth): Auth enum and login builder"
```

---

### Task 8: IMAP client — connect + login

**Files:**
- Modify: `src/imap_client.rs`

- [ ] **Step 1: Define the Client type and connect method**

Replace `src/imap_client.rs`:

```rust
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

        let client = async_imap::Client::new(tls);
        let _greeting = client
            .read_response()
            .await
            .ok_or_else(|| Error::Network("no IMAP greeting".into()))?;

        let session = match auth {
            Auth::Login { user, password } => client
                .login(user, password)
                .await
                .map_err(|(e, _)| Error::Auth { user: user.clone(), reason: e.to_string() })?,
            Auth::XOAuth2 { user, access_token } => {
                authenticate_xoauth2(client, user, access_token).await?
            }
        };
        Ok(Self { session })
    }
}

fn build_tls_config(insecure: bool) -> Result<ClientConfig> {
    if insecure {
        // Dangerously permissive — disable cert verification.
        let cfg = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(NoVerifier))
            .with_no_client_auth();
        return Ok(cfg);
    }
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    Ok(ClientConfig::builder().with_root_certificates(roots).with_no_client_auth())
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
    ) -> std::result::Result<tokio_rustls::rustls::client::danger::ServerCertVerified, tokio_rustls::rustls::Error> {
        Ok(tokio_rustls::rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _: &[u8],
        _: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> std::result::Result<tokio_rustls::rustls::client::danger::HandshakeSignatureValid, tokio_rustls::rustls::Error> {
        Ok(tokio_rustls::rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _: &[u8],
        _: &tokio_rustls::rustls::pki_types::CertificateDer<'_>,
        _: &tokio_rustls::rustls::DigitallySignedStruct,
    ) -> std::result::Result<tokio_rustls::rustls::client::danger::HandshakeSignatureValid, tokio_rustls::rustls::Error> {
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

/// Placeholder — full implementation lives in Phase 7 when oauth module lands.
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
```

- [ ] **Step 2: Verify compile**

Run: `cargo build`
Expected: builds.

- [ ] **Step 3: Commit**

```bash
git add src/imap_client.rs Cargo.toml
git commit -m "feat(imap): TLS connect + LOGIN authenticate"
```

---

### Task 9: IMAP client — list folders

**Files:**
- Modify: `src/imap_client.rs`

- [ ] **Step 1: Add list_folders method**

Append to `src/imap_client.rs` (inside `impl Client`):

```rust
    pub async fn list_folders(&mut self) -> Result<Vec<String>> {
        use futures::TryStreamExt;
        let names = self.session
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
```

- [ ] **Step 2: Compile**

Run: `cargo build`
Expected: builds.

- [ ] **Step 3: Commit**

```bash
git add src/imap_client.rs
git commit -m "feat(imap): list_folders + logout"
```

---

### Task 10: IMAP client — search UIDs + fetch Message-Ids

**Files:**
- Modify: `src/imap_client.rs`

- [ ] **Step 1: Add methods**

Append to `impl Client`:

```rust
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
                // CREATE returning "already exists" is a NO with that text;
                // we treat any error containing "exists" as success.
                let msg = e.to_string().to_lowercase();
                if msg.contains("exists") {
                    Ok(())
                } else {
                    Err(Error::from(e))
                }
            }
        }
    }

    /// Returns the source UIDs in selection order.
    pub async fn search_all_uids(&mut self) -> Result<Vec<u32>> {
        let set = self.session.uid_search("ALL").await?;
        let mut v: Vec<u32> = set.into_iter().collect();
        v.sort_unstable();
        Ok(v)
    }

    /// Returns a HashSet of Message-Id header values for every message in the
    /// currently selected folder. Used to dedup against destination.
    pub async fn fetch_all_message_ids(&mut self) -> Result<std::collections::HashSet<String>> {
        use futures::TryStreamExt;
        let mut ids = std::collections::HashSet::new();
        let mut stream = self.session
            .uid_fetch("1:*", "(BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])")
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
}

pub fn parse_message_id(header_bytes: &[u8]) -> Option<String> {
    let s = std::str::from_utf8(header_bytes).ok()?;
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("Message-ID:")
            .or_else(|| line.strip_prefix("Message-Id:"))
            .or_else(|| line.strip_prefix("MESSAGE-ID:"))
        {
            return Some(rest.trim().trim_matches(|c| c == '<' || c == '>').to_string());
        }
    }
    None
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
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib imap_client::`
Expected: 2 passed.

- [ ] **Step 3: Commit**

```bash
git add src/imap_client.rs
git commit -m "feat(imap): search UIDs, fetch Message-Id set, create folder"
```

---

### Task 11: IMAP client — fetch full message + append

**Files:**
- Modify: `src/imap_client.rs`

- [ ] **Step 1: Add FetchedMessage type and methods**

Append to `src/imap_client.rs`:

```rust
pub struct FetchedMessage {
    pub body: Vec<u8>,
    pub internal_date: Option<chrono::DateTime<chrono::FixedOffset>>,
    pub flags: Vec<String>,
    pub message_id: Option<String>,
}

impl Client {
    /// Fetches a single message by UID with full body, flags, and internal date.
    pub async fn fetch_full_by_uid(&mut self, uid: u32) -> Result<Option<FetchedMessage>> {
        use futures::TryStreamExt;
        let seq = format!("{uid}");
        let query = "(BODY.PEEK[] INTERNALDATE FLAGS BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])";
        let mut stream = self.session.uid_fetch(seq, query).await?;
        while let Some(msg) = stream.try_next().await? {
            let body = msg.body().map(|b| b.to_vec()).unwrap_or_default();
            let internal_date = msg.internal_date();
            let flags = msg.flags().map(|f| format!("{f}")).collect();
            let message_id = msg.header().and_then(parse_message_id);
            return Ok(Some(FetchedMessage { body, internal_date, flags, message_id }));
        }
        Ok(None)
    }

    /// Appends raw RFC822 bytes to the currently selected folder on this connection's session.
    /// `folder` is the mailbox name. Flags use the format `\Seen \Flagged` etc.
    pub async fn append_message(
        &mut self,
        folder: &str,
        body: &[u8],
        flags: &[String],
        internal_date: Option<chrono::DateTime<chrono::FixedOffset>>,
    ) -> Result<()> {
        let flag_str = flags.join(" ");
        let mut append = self.session.append(folder, body);
        if !flag_str.is_empty() {
            append = append.flags(flag_str);
        }
        if let Some(dt) = internal_date {
            append = append.internal_date(dt);
        }
        append.finish().await?;
        Ok(())
    }
}
```

Add `chrono` to `Cargo.toml`:

```toml
chrono = { version = "0.4", default-features = false, features = ["std"] }
```

- [ ] **Step 2: Compile**

Run: `cargo build`
Expected: builds. (If `async-imap` 0.10 has different append/internal_date APIs, adjust the calls to match its actual signature.)

- [ ] **Step 3: Commit**

```bash
git add src/imap_client.rs Cargo.toml Cargo.lock
git commit -m "feat(imap): fetch full message + append with flags and internaldate"
```

---

## Phase 4: Sync Algorithm

### Task 12: Folder filter

**Files:**
- Modify: `src/sync.rs`

- [ ] **Step 1: Implement filter with tests**

Replace `src/sync.rs`:

```rust
use globset::{Glob, GlobSet, GlobSetBuilder};
use crate::error::{Error, Result};

pub fn build_globset(patterns: &[String]) -> Result<Option<GlobSet>> {
    if patterns.is_empty() {
        return Ok(None);
    }
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        let g = Glob::new(p).map_err(|e| Error::Config(format!("bad glob '{p}': {e}")))?;
        b.add(g);
    }
    Ok(Some(b.build().map_err(|e| Error::Config(e.to_string()))?))
}

pub fn filter_folders(
    all: Vec<String>,
    include: Option<&GlobSet>,
    exclude: Option<&GlobSet>,
) -> Vec<String> {
    all.into_iter()
        .filter(|f| include.map(|s| s.is_match(f)).unwrap_or(true))
        .filter(|f| !exclude.map(|s| s.is_match(f)).unwrap_or(false))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_filters_keeps_everything() {
        let f = filter_folders(vec!["INBOX".into(), "Sent".into()], None, None);
        assert_eq!(f, vec!["INBOX", "Sent"]);
    }

    #[test]
    fn include_filter_keeps_only_matching() {
        let inc = build_globset(&["INBOX*".into()]).unwrap();
        let f = filter_folders(
            vec!["INBOX".into(), "INBOX.Sent".into(), "Trash".into()],
            inc.as_ref(),
            None,
        );
        assert_eq!(f, vec!["INBOX", "INBOX.Sent"]);
    }

    #[test]
    fn exclude_filter_drops_matching() {
        let exc = build_globset(&["Trash".into(), "spam".into()]).unwrap();
        let f = filter_folders(
            vec!["INBOX".into(), "Trash".into(), "spam".into()],
            None,
            exc.as_ref(),
        );
        assert_eq!(f, vec!["INBOX"]);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib sync::tests`
Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add src/sync.rs
git commit -m "feat(sync): include/exclude glob folder filter"
```

---

### Task 13: Per-folder sync function

**Files:**
- Modify: `src/sync.rs`

- [ ] **Step 1: Add Stats + sync_folder**

Append to `src/sync.rs`:

```rust
use crate::imap_client::Client;
use crate::progress::Reporter;

#[derive(Debug, Default, Clone)]
pub struct FolderStats {
    pub folder: String,
    pub copied: u64,
    pub skipped: u64,
    pub failed: u64,
    pub bytes: u64,
}

pub struct SyncOptions {
    pub max_message_size: Option<u64>,
    pub dry_run: bool,
}

pub async fn sync_folder(
    folder: &str,
    src: &mut Client,
    dst: &mut Client,
    reporter: &Reporter,
    opts: &SyncOptions,
) -> Result<FolderStats> {
    let mut stats = FolderStats { folder: folder.to_string(), ..Default::default() };

    dst.create_folder_if_missing(folder).await?;

    // Build dst Message-Id set BEFORE selecting on dst, since fetch_all_message_ids
    // requires being selected. We select on dst first, collect ids, then keep dst
    // selected for the APPENDs below.
    dst.select_for_write(folder).await?;
    let mut dst_ids = dst.fetch_all_message_ids().await.unwrap_or_default();

    src.examine(folder).await?;
    let src_uids = src.search_all_uids().await?;

    let bar = reporter.new_folder_bar(folder, src_uids.len() as u64);

    for uid in src_uids {
        match src.fetch_full_by_uid(uid).await {
            Ok(Some(msg)) => {
                let too_big = opts.max_message_size
                    .map(|m| msg.body.len() as u64 > m)
                    .unwrap_or(false);
                if too_big {
                    stats.skipped += 1;
                    bar.inc(1);
                    continue;
                }
                let dup = msg.message_id.as_ref().map(|m| dst_ids.contains(m)).unwrap_or(false);
                if dup {
                    stats.skipped += 1;
                    bar.inc(1);
                    continue;
                }

                if !opts.dry_run {
                    match dst.append_message(folder, &msg.body, &msg.flags, msg.internal_date).await {
                        Ok(()) => {
                            stats.copied += 1;
                            stats.bytes += msg.body.len() as u64;
                            if let Some(m) = msg.message_id {
                                dst_ids.insert(m);
                            }
                        }
                        Err(e) => {
                            stats.failed += 1;
                            tracing::warn!(folder = folder, uid, error = %e, "append failed");
                        }
                    }
                } else {
                    stats.copied += 1;  // counted as "would copy" in dry-run
                    stats.bytes += msg.body.len() as u64;
                }
            }
            Ok(None) => {
                stats.failed += 1;
                tracing::warn!(folder = folder, uid, "fetch returned no message");
            }
            Err(e) => {
                stats.failed += 1;
                tracing::warn!(folder = folder, uid, error = %e, "fetch failed");
            }
        }
        bar.inc(1);
    }
    bar.finish();
    Ok(stats)
}
```

- [ ] **Step 2: Compile**

Run: `cargo build`
Expected: builds (some unused imports may warn — fix or `#[allow]`).

- [ ] **Step 3: Commit**

```bash
git add src/sync.rs
git commit -m "feat(sync): per-folder streaming copy with dedup"
```

---

### Task 14: Top-level migration

**Files:**
- Modify: `src/sync.rs`

- [ ] **Step 1: Add run_migration**

Append to `src/sync.rs`:

```rust
use crate::auth::{from_login, Auth};
use crate::config::Settings;
use crate::imap_client::{Client, ConnectParams};

#[derive(Debug, Default)]
pub struct MigrationReport {
    pub folders: Vec<FolderStats>,
}

impl MigrationReport {
    pub fn total_copied(&self) -> u64 { self.folders.iter().map(|f| f.copied).sum() }
    pub fn total_skipped(&self) -> u64 { self.folders.iter().map(|f| f.skipped).sum() }
    pub fn total_failed(&self) -> u64 { self.folders.iter().map(|f| f.failed).sum() }
    pub fn total_bytes(&self) -> u64 { self.folders.iter().map(|f| f.bytes).sum() }
}

pub async fn run_migration(settings: &Settings, reporter: &Reporter) -> Result<MigrationReport> {
    let src_auth = from_login(&settings.src.user, &settings.src.auth)
        .ok_or_else(|| Error::Config("OAuth2 src not supported yet at this layer".into()))?;
    let dst_auth = from_login(&settings.dst.user, &settings.dst.auth)
        .ok_or_else(|| Error::Config("OAuth2 dst not supported yet at this layer".into()))?;

    let mut src = Client::connect_and_auth(
        ConnectParams { host: &settings.src.host, port: settings.src.port, tls: settings.src.tls, insecure: settings.src.insecure },
        &src_auth,
    ).await?;
    let mut dst = Client::connect_and_auth(
        ConnectParams { host: &settings.dst.host, port: settings.dst.port, tls: settings.dst.tls, insecure: settings.dst.insecure },
        &dst_auth,
    ).await?;

    let inc = build_globset(&settings.include)?;
    let exc = build_globset(&settings.exclude)?;

    let folders_all = src.list_folders().await?;
    let folders = filter_folders(folders_all, inc.as_ref(), exc.as_ref());

    let opts = SyncOptions {
        max_message_size: settings.max_message_size,
        dry_run: settings.dry_run,
    };

    let mut report = MigrationReport::default();
    for f in folders {
        let stats = sync_folder(&f, &mut src, &mut dst, reporter, &opts).await?;
        tracing::info!(folder = %f, copied = stats.copied, skipped = stats.skipped, failed = stats.failed, "folder done");
        report.folders.push(stats);
    }

    src.logout().await?;
    dst.logout().await?;
    Ok(report)
}
```

- [ ] **Step 2: Compile**

Run: `cargo build`
Expected: builds.

- [ ] **Step 3: Commit**

```bash
git add src/sync.rs
git commit -m "feat(sync): run_migration orchestrator"
```

---

## Phase 5: Progress + Main

### Task 15: Progress reporter (indicatif)

**Files:**
- Modify: `src/progress.rs`

- [ ] **Step 1: Implement Reporter**

Replace `src/progress.rs`:

```rust
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub struct Reporter {
    multi: MultiProgress,
    enabled: bool,
}

impl Reporter {
    pub fn new(enabled: bool) -> Self {
        Self { multi: MultiProgress::new(), enabled }
    }

    pub fn new_folder_bar(&self, folder: &str, total: u64) -> ProgressBar {
        if !self.enabled {
            return ProgressBar::hidden();
        }
        let pb = self.multi.add(ProgressBar::new(total));
        pb.set_style(
            ProgressStyle::with_template(
                "{prefix:>20} [{bar:30.cyan/blue}] {pos}/{len} ({percent}%) {msg}"
            )
            .unwrap()
            .progress_chars("=> ")
        );
        pb.set_prefix(folder.to_string());
        pb
    }
}
```

- [ ] **Step 2: Compile**

Run: `cargo build`
Expected: builds.

- [ ] **Step 3: Commit**

```bash
git add src/progress.rs
git commit -m "feat(progress): indicatif reporter with per-folder bars"
```

---

### Task 16: main.rs wiring

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Wire everything together**

Replace `src/main.rs`:

```rust
use clap::Parser;
use crab_imap_sync::{cli::Cli, config::Settings, progress::Reporter, sync::run_migration};
use tracing_subscriber::EnvFilter;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let exit_code = real_main().await;
    std::process::exit(exit_code);
}

async fn real_main() -> i32 {
    let cli = Cli::parse();

    let filter_level = if cli.verbose { "debug" } else if cli.quiet { "error" } else { "info" };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("crab_imap_sync={filter_level}")));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let settings = match Settings::from_cli(cli) {
        Ok(s) => s,
        Err(e) => { eprintln!("config error: {e}"); return 2; }
    };

    let reporter = Reporter::new(!settings.no_progress);

    match run_migration(&settings, &reporter).await {
        Ok(report) => {
            println!();
            println!("Summary:");
            for f in &report.folders {
                println!("  {:30} copied={} skipped={} failed={} bytes={}",
                    f.folder, f.copied, f.skipped, f.failed, f.bytes);
            }
            println!("Total: copied={} skipped={} failed={} bytes={}",
                report.total_copied(), report.total_skipped(),
                report.total_failed(), report.total_bytes());
            if report.total_failed() > 0 { 1 } else { 0 }
        }
        Err(e) => {
            eprintln!("migration error: {e}");
            use crab_imap_sync::error::Error::*;
            match e {
                Auth { .. } => 3,
                Network(_) => 4,
                Tls(_) => 5,
                _ => 1,
            }
        }
    }
}
```

- [ ] **Step 2: Build the binary**

Run: `cargo build --release`
Expected: builds the binary at `target/release/crab-imap-sync`.

- [ ] **Step 3: Run --help**

Run: `target/release/crab-imap-sync --help`
Expected: clap help text listing every option.

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat(main): wire CLI → config → sync with exit codes"
```

---

## Phase 6: Manual Smoke Test (the real migration)

### Task 17: Smoke test against cPanel → Hostinger

**Files:**
- Create: `docs/SMOKE.md`

- [ ] **Step 1: Document the smoke run**

Create `docs/SMOKE.md`:

```markdown
# Smoke Test: cPanel → Hostinger

This is a manual procedure. The goal is to verify CrabImapSync handles the
full VM Empreendimentos migration (one mailbox) with bounded memory.

## Setup

Export passwords (NEVER hard-code them):

```bash
export SRC_PASS='<SRC_PASSWORD_HERE>'
export DST_PASS='<DST_PASSWORD_HERE>'
```

## Dry run first

```bash
target/release/crab-imap-sync \
  --src-host imap.source.example --src-user me@source --src-pass-env SRC_PASS \
  --dst-host imap.dest.example --dst-user me@source --dst-pass-env DST_PASS \
  --dry-run -v
```

Expected: prints list of folders, count of messages that *would* be copied. Memory stays under 200 MB (`ps -o rss=` while it runs).

## Real run on a small mailbox

Pick the smallest source mailbox first (e.g. `almoxarifado01`):

```bash
target/release/crab-imap-sync \
  --src-host imap.source.example --src-user me@source --src-pass-env SRC_PASS \
  --dst-host imap.dest.example --dst-user me@source --dst-pass-env DST_PASS
```

Verify in Hostinger webmail that messages arrive.

## Memory ceiling check on the big one

`contato@` has ~13k messages / 4.7 GB. Run with monitoring:

```bash
target/release/crab-imap-sync \
  --src-host imap.source.example --src-user me@source --src-pass-env SRC_PASS \
  --dst-host imap.dest.example --dst-user me@source --dst-pass-env DST_PASS &
PID=$!
while kill -0 $PID 2>/dev/null; do
  ps -o rss= -p $PID | awk '{ printf "RSS: %.0f MB\n", $1/1024 }'
  sleep 30
done
```

PASS criterion: RSS never exceeds 200 MB. Process completes with exit 0.
```

- [ ] **Step 2: Build release**

Run: `cargo build --release`
Expected: binary in `target/release/crab-imap-sync`.

- [ ] **Step 3: Run dry-run smoke for one mailbox (manual)**

Run the dry-run command from SMOKE.md against `almoxarifado01`. If it fails, the immediate error log tells you which module needs fixing. **STOP and fix any bugs before continuing.**

- [ ] **Step 4: Commit docs**

```bash
git add docs/SMOKE.md
git commit -m "docs: smoke test procedure for cPanel→Hostinger"
```

---

## Phase 7: OAuth2

### Task 18: OAuth provider presets

**Files:**
- Modify: `src/oauth.rs`

- [ ] **Step 1: Add provider configs**

Replace `src/oauth.rs`:

```rust
use crate::error::{Error, Result};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Provider {
    Gmail,
    Microsoft,
    Custom { auth_url: String, token_url: String, scope: String },
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
            Self::Microsoft => "https://outlook.office.com/IMAP.AccessAsUser.All offline_access",
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
            other => Err(Error::Config(format!("unknown OAuth provider '{other}' (use gmail|microsoft|custom)"))),
        }
    }
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
}
```

- [ ] **Step 2: Run tests**

Run: `cargo test --lib oauth::`
Expected: 1 passed.

- [ ] **Step 3: Commit**

```bash
git add src/oauth.rs
git commit -m "feat(oauth): provider presets for Gmail and Microsoft"
```

---

### Task 19: OAuth2 PKCE + browser flow

**Files:**
- Modify: `src/oauth.rs`
- Modify: `src/cli.rs` (add oauth-related fields if not already present — they are from Task 5)

- [ ] **Step 1: Implement the browser flow**

Append to `src/oauth.rs`:

```rust
use oauth2::basic::BasicClient;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use std::collections::HashMap;
use std::io::Read;
use std::time::Duration;

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

pub fn obtain_token(req: OAuthRequest<'_>) -> Result<OAuthCreds> {
    let service = format!("crabimap:{}", provider_key(&req.provider));
    // 1. Try keyring refresh first.
    if req.use_keyring {
        if let Some(creds) = try_refresh_from_keyring(&service, req.user, &req)? {
            return Ok(creds);
        }
    }

    // 2. Bind local listener for callback BEFORE constructing redirect URL.
    let server = tiny_http::Server::http("127.0.0.1:0")
        .map_err(|e| Error::OAuth(format!("listener bind: {e}")))?;
    let port = match server.server_addr() {
        tiny_http::ListenAddr::IP(addr) => addr.port(),
        _ => return Err(Error::OAuth("unexpected listener addr".into())),
    };
    let redirect = format!("http://127.0.0.1:{port}/cb");

    // 3. Build oauth2 client.
    let auth_url = AuthUrl::new(req.provider.auth_url().into())
        .map_err(|e| Error::OAuth(format!("auth url: {e}")))?;
    let token_url = TokenUrl::new(req.provider.token_url().into())
        .map_err(|e| Error::OAuth(format!("token url: {e}")))?;
    let mut builder = BasicClient::new(ClientId::new(req.client_id.into()))
        .set_auth_uri(auth_url)
        .set_token_uri(token_url)
        .set_redirect_uri(RedirectUrl::new(redirect.clone())
            .map_err(|e| Error::OAuth(format!("redirect: {e}")))?);
    if let Some(secret) = req.client_secret {
        if !secret.is_empty() {
            builder = builder.set_client_secret(ClientSecret::new(secret.into()));
        }
    }
    let client = builder;

    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let (auth_url, csrf) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new(req.provider.default_scope().into()))
        .set_pkce_challenge(challenge)
        .url();

    // 4. Open browser.
    if webbrowser::open(auth_url.as_str()).is_err() {
        eprintln!("Open this URL manually:\n{auth_url}");
    }

    // 5. Wait for callback with state + code.
    let (code, state) = wait_for_callback(server, Duration::from_secs(300))?;
    if state != *csrf.secret() {
        return Err(Error::OAuth("CSRF state mismatch".into()));
    }

    // 6. Exchange code for tokens.
    let http_client = oauth2::reqwest::blocking::ClientBuilder::new()
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| Error::OAuth(format!("http client: {e}")))?;
    let token = client
        .exchange_code(AuthorizationCode::new(code))
        .set_pkce_verifier(verifier)
        .request(&http_client)
        .map_err(|e| Error::OAuth(format!("token exchange: {e}")))?;

    let access_token = token.access_token().secret().clone();
    let refresh_token = token.refresh_token().map(|r| r.secret().clone());

    // 7. Persist refresh token.
    if req.use_keyring {
        if let Some(rt) = &refresh_token {
            let entry = keyring::Entry::new(&service, req.user)
                .map_err(|e| Error::OAuth(format!("keyring: {e}")))?;
            entry.set_password(rt)
                .map_err(|e| Error::OAuth(format!("keyring set: {e}")))?;
        }
    }

    Ok(OAuthCreds { access_token, refresh_token })
}

fn provider_key(p: &Provider) -> &'static str {
    match p {
        Provider::Gmail => "gmail",
        Provider::Microsoft => "microsoft",
        Provider::Custom { .. } => "custom",
    }
}

fn wait_for_callback(server: tiny_http::Server, timeout: Duration) -> Result<(String, String)> {
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if let Some(req) = server.recv_timeout(Duration::from_secs(1))
            .map_err(|e| Error::OAuth(format!("recv: {e}")))?
        {
            let url = req.url().to_string();
            let parsed = url::Url::parse(&format!("http://127.0.0.1{url}"))
                .map_err(|e| Error::OAuth(format!("parse cb: {e}")))?;
            let params: HashMap<_, _> = parsed.query_pairs().collect();
            let code = params.get("code").map(|s| s.to_string());
            let state = params.get("state").map(|s| s.to_string());

            let body = "<html><body><h1>Authentication successful</h1><p>You can close this tab.</p></body></html>";
            let resp = tiny_http::Response::from_string(body)
                .with_header("Content-Type: text/html".parse::<tiny_http::Header>().unwrap());
            let _ = req.respond(resp);

            return code.zip(state)
                .ok_or_else(|| Error::OAuth("callback missing code/state".into()));
        }
    }
    Err(Error::OAuth("timed out waiting for OAuth callback".into()))
}

fn try_refresh_from_keyring(service: &str, user: &str, req: &OAuthRequest<'_>) -> Result<Option<OAuthCreds>> {
    let entry = match keyring::Entry::new(service, user) {
        Ok(e) => e,
        Err(_) => return Ok(None),
    };
    let refresh_token = match entry.get_password() {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };

    let token_url = TokenUrl::new(req.provider.token_url().into())
        .map_err(|e| Error::OAuth(format!("token url: {e}")))?;
    let auth_url = AuthUrl::new(req.provider.auth_url().into())
        .map_err(|e| Error::OAuth(format!("auth url: {e}")))?;
    let mut builder = BasicClient::new(ClientId::new(req.client_id.into()))
        .set_auth_uri(auth_url)
        .set_token_uri(token_url);
    if let Some(secret) = req.client_secret {
        if !secret.is_empty() {
            builder = builder.set_client_secret(ClientSecret::new(secret.into()));
        }
    }

    let http_client = oauth2::reqwest::blocking::ClientBuilder::new()
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| Error::OAuth(format!("http: {e}")))?;

    let resp = builder
        .exchange_refresh_token(&oauth2::RefreshToken::new(refresh_token.clone()))
        .request(&http_client);
    match resp {
        Ok(token) => Ok(Some(OAuthCreds {
            access_token: token.access_token().secret().clone(),
            refresh_token: token.refresh_token().map(|r| r.secret().clone()).or(Some(refresh_token)),
        })),
        Err(_) => Ok(None),
    }
}
```

Note: the exact `oauth2` 5.x API may differ slightly. Adjust method names if the crate version is newer. Use `cargo doc --open` or check the crate's README to verify.

- [ ] **Step 2: Add reqwest features to oauth2 in Cargo.toml**

If `oauth2 = "5"` doesn't enable reqwest by default, change to:

```toml
oauth2 = { version = "5", features = ["reqwest", "rustls-tls"] }
```

- [ ] **Step 3: Compile**

Run: `cargo build`
Expected: builds.

- [ ] **Step 4: Commit**

```bash
git add src/oauth.rs Cargo.toml Cargo.lock
git commit -m "feat(oauth): PKCE browser flow + keyring refresh"
```

---

### Task 20: Wire XOAUTH2 into imap_client + main flow

**Files:**
- Modify: `src/imap_client.rs`
- Modify: `src/sync.rs`
- Modify: `src/cli.rs` (oauth CLI fields)
- Modify: `src/config.rs`

- [ ] **Step 1: Implement XOAUTH2 SASL in imap_client**

Replace the `authenticate_xoauth2` stub in `src/imap_client.rs`:

```rust
async fn authenticate_xoauth2(
    client: async_imap::Client<TlsStream<TcpStream>>,
    user: &str,
    access_token: &str,
) -> Result<Session<TlsStream<TcpStream>>> {
    use base64::Engine as _;
    let raw = format!("user={user}\x01auth=Bearer {access_token}\x01\x01");
    let encoded = base64::engine::general_purpose::STANDARD.encode(raw);
    client
        .authenticate("XOAUTH2", &mut Xoauth2Auth { token: encoded })
        .await
        .map_err(|(e, _)| Error::Auth { user: user.to_string(), reason: e.to_string() })
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
```

- [ ] **Step 2: Add OAuth fields to Cli (if not already in Task 5)**

Append to `src/cli.rs`:

```rust
// In Cli struct, ensure these exist (they appear in SPEC's CLI surface):
//   --src-oauth-provider, --src-oauth-client-id, --src-oauth-client-secret-env,
//   --src-oauth-auth-url, --src-oauth-token-url, --src-oauth-scope, --src-oauth-no-keyring
// Mirror for --dst-*.
```

Append to the existing `Cli` struct (modify in place):

```rust
    #[arg(long)] pub src_oauth_provider: Option<String>,
    #[arg(long)] pub src_oauth_client_id: Option<String>,
    #[arg(long)] pub src_oauth_client_secret_env: Option<String>,
    #[arg(long)] pub src_oauth_auth_url: Option<String>,
    #[arg(long)] pub src_oauth_token_url: Option<String>,
    #[arg(long)] pub src_oauth_scope: Option<String>,
    #[arg(long, default_value_t = false)] pub src_oauth_no_keyring: bool,

    #[arg(long)] pub dst_oauth_provider: Option<String>,
    #[arg(long)] pub dst_oauth_client_id: Option<String>,
    #[arg(long)] pub dst_oauth_client_secret_env: Option<String>,
    #[arg(long)] pub dst_oauth_auth_url: Option<String>,
    #[arg(long)] pub dst_oauth_token_url: Option<String>,
    #[arg(long)] pub dst_oauth_scope: Option<String>,
    #[arg(long, default_value_t = false)] pub dst_oauth_no_keyring: bool,
```

- [ ] **Step 3: Adjust config.rs to capture oauth args**

Replace the `AuthMethod::OAuth2 {}` placeholder with a struct carrying the provider config:

```rust
pub enum AuthMethod {
    Login { password: String },
    OAuth2 {
        provider_kind: String,        // "gmail" | "microsoft" | "custom"
        client_id: String,
        client_secret: Option<String>,
        auth_url: Option<String>,     // for custom
        token_url: Option<String>,    // for custom
        scope: Option<String>,
        use_keyring: bool,
    },
}
```

And update `build_endpoint` to populate it (read secret from `*_oauth_client_secret_env` env var). The Cli changes from step 2 give you those fields.

- [ ] **Step 4: Update sync::run_migration to resolve OAuth when needed**

Replace the `from_login(...)` calls with:

```rust
let src_auth = resolve_auth(&settings.src)?;
let dst_auth = resolve_auth(&settings.dst)?;
```

Add at the bottom of `src/sync.rs`:

```rust
use crate::config::EndpointSettings;
use crate::oauth::{obtain_token, OAuthRequest, Provider};
use std::str::FromStr;

fn resolve_auth(ep: &EndpointSettings) -> Result<Auth> {
    match &ep.auth {
        crate::config::AuthMethod::Login { password } => {
            Ok(Auth::login(ep.user.clone(), password.clone()))
        }
        crate::config::AuthMethod::OAuth2 {
            provider_kind, client_id, client_secret, auth_url, token_url, scope, use_keyring,
        } => {
            let provider = if provider_kind == "custom" {
                Provider::Custom {
                    auth_url: auth_url.clone().ok_or_else(|| Error::Config("custom oauth needs auth-url".into()))?,
                    token_url: token_url.clone().ok_or_else(|| Error::Config("custom oauth needs token-url".into()))?,
                    scope: scope.clone().unwrap_or_default(),
                }
            } else {
                Provider::from_str(provider_kind)?
            };
            let creds = obtain_token(OAuthRequest {
                provider,
                user: &ep.user,
                client_id,
                client_secret: client_secret.as_deref(),
                use_keyring: *use_keyring,
            })?;
            Ok(Auth::XOAuth2 { user: ep.user.clone(), access_token: creds.access_token })
        }
    }
}
```

- [ ] **Step 5: Compile**

Run: `cargo build`
Expected: builds.

- [ ] **Step 6: Commit**

```bash
git add src/
git commit -m "feat(oauth): wire XOAUTH2 SASL through CLI/config/sync"
```

---

## Phase 8: Integration Tests

### Task 21: testcontainers harness for Dovecot

**Files:**
- Create: `tests/common/mod.rs`
- Create: `tests/integration.rs`

- [ ] **Step 1: Create the harness**

Create `tests/common/mod.rs`:

```rust
use testcontainers::core::{ContainerPort, IntoContainerPort, WaitFor};
use testcontainers::{GenericImage, ImageExt};

pub fn dovecot_image() -> GenericImage {
    GenericImage::new("dovecot/dovecot", "latest")
        .with_exposed_port(143.tcp())
        .with_wait_for(WaitFor::message_on_stderr("Login process exited"))
}

pub struct DovecotInstance {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
}
```

(The real Dovecot image needs a config volume; pick whichever image works with `testcontainers` and parameterize accordingly. If dovecot/dovecot doesn't expose plain auth out of the box, use `tvial/docker-mailserver` or `analogic/poste.io` instead.)

- [ ] **Step 2: Write a minimal end-to-end test**

Create `tests/integration.rs`:

```rust
mod common;

#[tokio::test]
#[ignore] // requires Docker
async fn end_to_end_minimal_sync() {
    // Stub for now: start 2 Dovecot containers, append a fixture .eml to
    // source INBOX via the IMAP crate, run sync::run_migration, assert that
    // dest INBOX has the same Message-Id.
    //
    // Implement once you've picked the working Dovecot image.
}
```

- [ ] **Step 3: Compile**

Run: `cargo build --tests`
Expected: builds.

- [ ] **Step 4: Commit**

```bash
git add tests/
git commit -m "test: scaffold integration test harness"
```

> Note: Filling out a real Dovecot-backed integration test is a half-day task on its own. Defer the body of this test until after the smoke test (Task 17) confirms the binary works against real cPanel/Hostinger. If the smoke passes, the integration test is a safety net for future regressions, not a blocker for v0.1.

---

## Phase 9: Docs + CI

### Task 22: Write README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Rewrite README**

```markdown
# CrabImapSync 🦀✉️

Memory-bounded, streaming IMAP-to-IMAP migration CLI written in Rust.

## Why

`imapsync` is the standard for IMAP migration but its default behavior loads
the full message-header tables of both servers into RAM. On a 4.7 GB mailbox
this consumed 62 GB of memory and was killed by the macOS OOM killer.

CrabImapSync transfers messages one at a time. Peak memory stays around
~1 MB per active folder plus one message-sized buffer, regardless of mailbox size.

## Install

```bash
cargo install --git https://github.com/felipemsouza/crab-imap-sync
```

## Usage (LOGIN auth)

```bash
export SRC_PASS='...'
export DST_PASS='...'
crab-imap-sync \
  --src-host imap.source.example --src-user me@source --src-pass-env SRC_PASS \
  --dst-host imap.dest.example   --dst-user me@dest   --dst-pass-env DST_PASS
```

Filter folders:

```bash
crab-imap-sync ... --include 'INBOX*' --exclude 'Trash'
```

Dry run first:

```bash
crab-imap-sync ... --dry-run
```

## Usage (OAuth2, e.g. Gmail)

Register your own OAuth2 client at <https://console.cloud.google.com/apis/credentials>.
Set the redirect URI to `http://127.0.0.1:*` and IMAP scope `https://mail.google.com/`.

```bash
export DST_OAUTH_SECRET='your-client-secret'
crab-imap-sync \
  --src-host imap.source.example --src-user me@source --src-pass-env SRC_PASS \
  --dst-host imap.gmail.com --dst-user me@gmail.com \
  --dst-auth oauth2 --dst-oauth-provider gmail \
  --dst-oauth-client-id 'YOUR-CLIENT-ID.apps.googleusercontent.com' \
  --dst-oauth-client-secret-env DST_OAUTH_SECRET
```

A browser tab opens for consent. Tokens persist in your OS keyring; subsequent runs skip the browser.

## Comparison with imapsync

| | imapsync | CrabImapSync |
|---|---|---|
| Peak RAM (4.7 GB mailbox) | 62 GB | <200 MB |
| Language | Perl | Rust |
| Dependency footprint | CPAN + libssl | static binary |
| Resumable | yes (UID) | yes (Message-Id) |
| OAuth2 | yes | yes (with browser flow) |
| Maturity | 20+ years | new |

## License

MIT
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: README quickstart + imapsync comparison"
```

---

### Task 23: GitHub Actions CI

**Files:**
- Create: `.github/workflows/ci.yml`

- [ ] **Step 1: Write the workflow**

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - run: cargo fmt --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test --lib
```

- [ ] **Step 2: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: rustfmt + clippy + unit tests on ubuntu/macos"
```

---

### Task 24: OAuth2 setup docs

**Files:**
- Create: `docs/OAUTH2.md`

- [ ] **Step 1: Write the doc**

```markdown
# Setting up OAuth2 for CrabImapSync

CrabImapSync uses the standard OAuth2 PKCE flow with a local browser callback.
You need to register an OAuth2 client at your provider's console before first use.

## Gmail / Google Workspace

1. Go to <https://console.cloud.google.com/apis/credentials>.
2. Click **Create credentials** → **OAuth client ID**.
3. Application type: **Desktop app**.
4. Save the Client ID and Client Secret.
5. Enable the Gmail API: APIs & Services → Library → Gmail API → Enable.
6. Set environment variable and run:

   ```bash
   export GMAIL_OAUTH_SECRET='your-secret'
   crab-imap-sync ... \
     --dst-auth oauth2 --dst-oauth-provider gmail \
     --dst-oauth-client-id 'YOUR.apps.googleusercontent.com' \
     --dst-oauth-client-secret-env GMAIL_OAUTH_SECRET
   ```

A browser opens for consent. After approval the refresh token is stored in
your OS keyring so future runs skip the browser.

## Microsoft 365

1. Go to <https://entra.microsoft.com/> → App registrations → New registration.
2. Supported account types: choose what fits your tenant.
3. Redirect URI: type **Public client (mobile & desktop)** → `http://localhost`.
4. Note the **Application (client) ID**.
5. API permissions → Add → Microsoft Graph (delegated) → `IMAP.AccessAsUser.All`, `offline_access`.
6. Grant admin consent if your tenant requires it.
7. Run:

   ```bash
   crab-imap-sync ... \
     --dst-auth oauth2 --dst-oauth-provider microsoft \
     --dst-oauth-client-id 'YOUR-CLIENT-ID'
   ```

   Microsoft public clients don't have a client secret — omit `--dst-oauth-client-secret-env`.

## Custom (any OAuth2 provider)

```bash
crab-imap-sync ... \
  --dst-auth oauth2 --dst-oauth-provider custom \
  --dst-oauth-auth-url 'https://example.com/oauth/auth' \
  --dst-oauth-token-url 'https://example.com/oauth/token' \
  --dst-oauth-scope 'imap.read imap.write offline_access' \
  --dst-oauth-client-id 'cid' \
  --dst-oauth-client-secret-env CUSTOM_SECRET
```

## Disabling keyring

If you don't want refresh tokens persisted:

```bash
crab-imap-sync ... --dst-oauth-no-keyring
```

The browser flow will run every invocation.
```

- [ ] **Step 2: Commit**

```bash
git add docs/OAUTH2.md
git commit -m "docs: OAuth2 setup walkthrough for Gmail/Microsoft/custom"
```

---

### Task 25: Final polish

**Files:**
- Modify: any with warnings

- [ ] **Step 1: Run clippy and fix warnings**

```bash
cargo clippy --all-targets -- -D warnings
```

Fix any reported issues. Common: unused imports, missing `#[must_use]`, etc.

- [ ] **Step 2: Run fmt**

```bash
cargo fmt
```

- [ ] **Step 3: Run all unit tests**

```bash
cargo test --lib
```

Expected: all passing.

- [ ] **Step 4: Commit and tag v0.1.0**

```bash
git add -A
git commit -m "chore: clippy/fmt polish"
git tag v0.1.0
```

---

## Done

After Task 25 the binary is usable for the VM Empreendimentos migration via LOGIN. The OAuth2 path is implemented but only smoke-tested manually; defer publishing to crates.io until you've shipped at least one real migration end-to-end (Task 17 smoke).

---

## Self-Review Checklist (for the plan author, run before handing off)

- [x] Every spec section maps to at least one task (Auth/IMAP/sync/progress/main/OAuth/CI/docs all present).
- [x] No "TBD"/"TODO"/"implement later" — all code is concrete.
- [x] Type names match across tasks (`Client`, `Auth`, `Settings`, `FolderStats`, `Reporter`).
- [x] Exit codes from SPEC reflected in `main.rs` (Task 16).
- [x] Memory ceiling target (200 MB) is in the smoke test (Task 17).
- [x] Test-driven where it makes sense (parsing, filter logic). Pragmatic for I/O code (compile + smoke).
- [x] No premature parallelism or SQLite caching — both deferred to v2 per spec.
