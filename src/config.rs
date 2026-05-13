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
    Login {
        password: String,
    },
    OAuth2 {
        provider_kind: String,
        client_id: String,
        client_secret: Option<String>,
        auth_url: Option<String>,
        token_url: Option<String>,
        scope: Option<String>,
        use_keyring: bool,
    },
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
        let src = EndpointInputs {
            host: cli.src_host,
            port: cli.src_port,
            tls: cli.src_tls,
            user: cli.src_user,
            insecure: cli.src_insecure,
            auth_kind: cli.src_auth,
            pass_env: cli.src_pass_env,
            oauth_provider: cli.src_oauth_provider,
            oauth_client_id: cli.src_oauth_client_id,
            oauth_client_secret_env: cli.src_oauth_client_secret_env,
            oauth_auth_url: cli.src_oauth_auth_url,
            oauth_token_url: cli.src_oauth_token_url,
            oauth_scope: cli.src_oauth_scope,
            oauth_no_keyring: cli.src_oauth_no_keyring,
            side: "src",
        }
        .build()?;

        let dst = EndpointInputs {
            host: cli.dst_host,
            port: cli.dst_port,
            tls: cli.dst_tls,
            user: cli.dst_user,
            insecure: cli.dst_insecure,
            auth_kind: cli.dst_auth,
            pass_env: cli.dst_pass_env,
            oauth_provider: cli.dst_oauth_provider,
            oauth_client_id: cli.dst_oauth_client_id,
            oauth_client_secret_env: cli.dst_oauth_client_secret_env,
            oauth_auth_url: cli.dst_oauth_auth_url,
            oauth_token_url: cli.dst_oauth_token_url,
            oauth_scope: cli.dst_oauth_scope,
            oauth_no_keyring: cli.dst_oauth_no_keyring,
            side: "dst",
        }
        .build()?;

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

struct EndpointInputs {
    host: String,
    port: u16,
    tls: TlsMode,
    user: String,
    insecure: bool,
    auth_kind: AuthKind,
    pass_env: Option<String>,
    oauth_provider: Option<String>,
    oauth_client_id: Option<String>,
    oauth_client_secret_env: Option<String>,
    oauth_auth_url: Option<String>,
    oauth_token_url: Option<String>,
    oauth_scope: Option<String>,
    oauth_no_keyring: bool,
    side: &'static str,
}

impl EndpointInputs {
    fn build(self) -> Result<EndpointSettings> {
        let side = self.side;
        if self.host.is_empty() {
            return Err(Error::Config(format!("--{side}-host cannot be empty")));
        }
        if self.user.is_empty() {
            return Err(Error::Config(format!("--{side}-user cannot be empty")));
        }
        let auth = match self.auth_kind {
            AuthKind::Login => {
                let envvar = self.pass_env.ok_or_else(|| {
                    Error::Config(format!(
                        "--{side}-pass-env is required when --{side}-auth=login"
                    ))
                })?;
                let password = std::env::var(&envvar).map_err(|_| {
                    Error::Config(format!("env var '{envvar}' for {side} password is not set"))
                })?;
                AuthMethod::Login { password }
            }
            AuthKind::Oauth2 => {
                let provider_kind = self.oauth_provider.ok_or_else(|| {
                    Error::Config(format!("--{side}-oauth-provider required for oauth2"))
                })?;
                let client_id = self.oauth_client_id.ok_or_else(|| {
                    Error::Config(format!("--{side}-oauth-client-id required for oauth2"))
                })?;
                let client_secret = if let Some(envvar) = self.oauth_client_secret_env {
                    Some(std::env::var(&envvar).map_err(|_| {
                        Error::Config(format!(
                            "env var '{envvar}' for {side} oauth client secret is not set"
                        ))
                    })?)
                } else {
                    None
                };
                AuthMethod::OAuth2 {
                    provider_kind,
                    client_id,
                    client_secret,
                    auth_url: self.oauth_auth_url,
                    token_url: self.oauth_token_url,
                    scope: self.oauth_scope,
                    use_keyring: !self.oauth_no_keyring,
                }
            }
        };
        Ok(EndpointSettings {
            host: self.host,
            port: self.port,
            tls: self.tls,
            user: self.user,
            insecure: self.insecure,
            auth,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    fn minimal_cli() -> Cli {
        Cli::try_parse_from([
            "crab-imap-sync",
            "--src-host",
            "src.example",
            "--src-user",
            "a@x",
            "--src-pass-env",
            "TEST_SRC_PASS",
            "--dst-host",
            "dst.example",
            "--dst-user",
            "a@y",
            "--dst-pass-env",
            "TEST_DST_PASS",
        ])
        .unwrap()
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
