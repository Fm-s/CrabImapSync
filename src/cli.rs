use clap::Parser;

#[derive(Debug, Parser, PartialEq, Eq)]
#[command(name = "crab-imap-sync", version, about, long_about = None)]
pub struct Cli {
    // ---------- Source ----------
    #[arg(long)]
    pub src_host: String,
    #[arg(long, default_value_t = 993)]
    pub src_port: u16,
    #[arg(long, default_value = "imaps")]
    pub src_tls: TlsMode,
    #[arg(long)]
    pub src_user: String,
    #[arg(long, default_value = "login")]
    pub src_auth: AuthKind,
    #[arg(long)]
    pub src_pass_env: Option<String>,
    #[arg(long, default_value_t = false)]
    pub src_insecure: bool,

    #[arg(long)]
    pub src_oauth_provider: Option<String>,
    #[arg(long)]
    pub src_oauth_client_id: Option<String>,
    #[arg(long)]
    pub src_oauth_client_secret_env: Option<String>,
    #[arg(long)]
    pub src_oauth_auth_url: Option<String>,
    #[arg(long)]
    pub src_oauth_token_url: Option<String>,
    #[arg(long)]
    pub src_oauth_scope: Option<String>,
    #[arg(long, default_value_t = false)]
    pub src_oauth_no_keyring: bool,

    // ---------- Destination ----------
    #[arg(long)]
    pub dst_host: String,
    #[arg(long, default_value_t = 993)]
    pub dst_port: u16,
    #[arg(long, default_value = "imaps")]
    pub dst_tls: TlsMode,
    #[arg(long)]
    pub dst_user: String,
    #[arg(long, default_value = "login")]
    pub dst_auth: AuthKind,
    #[arg(long)]
    pub dst_pass_env: Option<String>,
    #[arg(long, default_value_t = false)]
    pub dst_insecure: bool,

    #[arg(long)]
    pub dst_oauth_provider: Option<String>,
    #[arg(long)]
    pub dst_oauth_client_id: Option<String>,
    #[arg(long)]
    pub dst_oauth_client_secret_env: Option<String>,
    #[arg(long)]
    pub dst_oauth_auth_url: Option<String>,
    #[arg(long)]
    pub dst_oauth_token_url: Option<String>,
    #[arg(long)]
    pub dst_oauth_scope: Option<String>,
    #[arg(long, default_value_t = false)]
    pub dst_oauth_no_keyring: bool,

    // ---------- Sync options ----------
    #[arg(long)]
    pub include: Vec<String>,
    #[arg(long)]
    pub exclude: Vec<String>,
    #[arg(long)]
    pub max_message_size: Option<u64>,
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    #[arg(long, default_value_t = 300)]
    pub timeout_secs: u64,
    #[arg(long, default_value_t = 3)]
    pub retries: u32,

    // ---------- Output ----------
    #[arg(short, long, default_value_t = false)]
    pub verbose: bool,
    #[arg(long, default_value_t = false)]
    pub quiet: bool,
    #[arg(long, default_value_t = false)]
    pub no_progress: bool,
    #[arg(long)]
    pub log_file: Option<std::path::PathBuf>,
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
            "--src-host",
            "src.example.com",
            "--src-user",
            "a@x",
            "--src-pass-env",
            "SRC_PASS",
            "--dst-host",
            "dst.example.com",
            "--dst-user",
            "a@y",
            "--dst-pass-env",
            "DST_PASS",
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
