use clap::Parser;
use crab_imap_sync::{cli::Cli, config::Settings, progress::Reporter, sync::run_migration};
use tracing_subscriber::EnvFilter;

#[tokio::main(flavor = "multi_thread", worker_threads = 4)]
async fn main() {
    let exit_code = real_main().await;
    std::process::exit(exit_code);
}

async fn real_main() -> i32 {
    // rustls needs an explicit crypto provider when multiple are available transitively.
    let _ = tokio_rustls::rustls::crypto::aws_lc_rs::default_provider().install_default();

    let cli = Cli::parse();

    let cli_level = if cli.verbose {
        "debug"
    } else if cli.quiet {
        "error"
    } else {
        "info"
    };
    let filter = EnvFilter::new(format!("crab_imap_sync={cli_level}"));
    // If RUST_LOG is set and the user did not pass -v/-q, honour RUST_LOG.
    let filter = if !cli.verbose && !cli.quiet {
        EnvFilter::try_from_default_env().unwrap_or(filter)
    } else {
        filter
    };
    // With the `tracing-log` feature on tracing-subscriber, log::* calls
    // (e.g. from async-imap) are automatically bridged to tracing.
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let settings = match Settings::from_cli(cli) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("config error: {e}");
            return 2;
        }
    };

    let reporter = Reporter::new(!settings.no_progress);

    match run_migration(&settings, &reporter).await {
        Ok(report) => {
            println!();
            println!("Summary:");
            for f in &report.folders {
                println!(
                    "  {:30} copied={} would_copy={} skipped={} failed={} bytes={}",
                    f.folder, f.copied, f.would_copy, f.skipped, f.failed, f.bytes
                );
            }
            println!(
                "Total: copied={} would_copy={} skipped={} failed={} bytes={}",
                report.total_copied(),
                report.total_would_copy(),
                report.total_skipped(),
                report.total_failed(),
                report.total_bytes()
            );
            if report.total_failed() > 0 {
                1
            } else {
                0
            }
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
