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

    let filter_level = if cli.verbose {
        "debug"
    } else if cli.quiet {
        "error"
    } else {
        "info"
    };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("crab_imap_sync={filter_level}")));
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
                    "  {:30} copied={} skipped={} failed={} bytes={}",
                    f.folder, f.copied, f.skipped, f.failed, f.bytes
                );
            }
            println!(
                "Total: copied={} skipped={} failed={} bytes={}",
                report.total_copied(),
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
