use crate::auth::{from_login, Auth};
use crate::config::{EndpointSettings, Settings};
use crate::error::{Error, Result};
use crate::imap_client::{Client, ConnectParams};
use crate::oauth::{obtain_token, OAuthRequest, Provider};
use crate::progress::Reporter;
use globset::{Glob, GlobSet, GlobSetBuilder};
use std::str::FromStr;

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
    let mut stats = FolderStats {
        folder: folder.to_string(),
        ..Default::default()
    };

    tracing::info!(folder, "creating destination folder if missing");
    dst.create_folder_if_missing(folder).await?;

    tracing::info!(
        folder,
        "indexing destination message-ids (this may take a while for large folders)"
    );
    dst.select_for_write(folder).await?;
    let mut dst_ids = dst.fetch_all_message_ids().await?;
    tracing::info!(folder, dst_count = dst_ids.len(), "destination indexed");

    tracing::info!(folder, "listing source UIDs");
    src.examine(folder).await?;
    let src_uids = src.search_all_uids().await?;
    tracing::info!(
        folder,
        src_count = src_uids.len(),
        "starting message transfer"
    );

    let total = src_uids.len() as u64;
    let bar = reporter.new_folder_bar(folder, total);
    let log_every: u64 = (total / 40).clamp(20, 500);
    let started = std::time::Instant::now();

    for (idx, uid) in src_uids.iter().copied().enumerate() {
        // 1. Cheap: fetch only Message-Id header to decide if we need the body.
        let probe_id = match src.fetch_message_id_by_uid(uid).await {
            Ok(id) => id,
            Err(e) => {
                stats.failed += 1;
                tracing::warn!(folder = folder, uid, error = %e, "message-id probe failed");
                bar.inc(1);
                continue;
            }
        };

        // 2. If destination already has this Message-Id, skip body fetch entirely.
        if let Some(ref mid) = probe_id {
            if dst_ids.contains(mid) {
                stats.skipped += 1;
                bar.inc(1);
                // periodic progress logging still applies below
                let done = (idx as u64) + 1;
                if done == 1 || done == total || done.is_multiple_of(log_every) {
                    log_progress(folder, done, total, &stats, started);
                }
                continue;
            }
        }

        // 3. Not a duplicate (or no Message-Id at all): fetch the full body.
        match src.fetch_full_by_uid(uid).await {
            Ok(Some(msg)) => {
                let too_big = opts
                    .max_message_size
                    .map(|m| msg.body.len() as u64 > m)
                    .unwrap_or(false);
                if too_big {
                    stats.skipped += 1;
                    bar.inc(1);
                    continue;
                }

                if !opts.dry_run {
                    match dst
                        .append_message(folder, &msg.body, &msg.flags, msg.internal_date)
                        .await
                    {
                        Ok(()) => {
                            stats.copied += 1;
                            stats.bytes += msg.body.len() as u64;
                            if let Some(m) = msg.message_id.or(probe_id) {
                                dst_ids.insert(m);
                            }
                        }
                        Err(e) => {
                            stats.failed += 1;
                            tracing::warn!(folder = folder, uid, error = %e, "append failed");
                        }
                    }
                } else {
                    stats.copied += 1;
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

        let done = (idx as u64) + 1;
        if done == 1 || done == total || done.is_multiple_of(log_every) {
            log_progress(folder, done, total, &stats, started);
        }
    }
    bar.finish();
    Ok(stats)
}

fn log_progress(
    folder: &str,
    done: u64,
    total: u64,
    stats: &FolderStats,
    started: std::time::Instant,
) {
    let elapsed = started.elapsed().as_secs_f64();
    let rate = if elapsed > 0.0 {
        done as f64 / elapsed
    } else {
        0.0
    };
    let eta_secs = if rate > 0.0 {
        ((total - done) as f64 / rate) as u64
    } else {
        0
    };
    let mb = stats.bytes as f64 / 1_048_576.0;
    tracing::info!(
        folder,
        progress = format!("{done}/{total}"),
        copied = stats.copied,
        skipped = stats.skipped,
        failed = stats.failed,
        mb_copied = format!("{mb:.2}"),
        rate = format!("{rate:.1} msg/s"),
        eta_secs,
        "progress"
    );
}

#[derive(Debug, Default)]
pub struct MigrationReport {
    pub folders: Vec<FolderStats>,
}

impl MigrationReport {
    pub fn total_copied(&self) -> u64 {
        self.folders.iter().map(|f| f.copied).sum()
    }
    pub fn total_skipped(&self) -> u64 {
        self.folders.iter().map(|f| f.skipped).sum()
    }
    pub fn total_failed(&self) -> u64 {
        self.folders.iter().map(|f| f.failed).sum()
    }
    pub fn total_bytes(&self) -> u64 {
        self.folders.iter().map(|f| f.bytes).sum()
    }
}

pub async fn run_migration(settings: &Settings, reporter: &Reporter) -> Result<MigrationReport> {
    let src_auth = resolve_auth(&settings.src)?;
    let dst_auth = resolve_auth(&settings.dst)?;

    let mut src = Client::connect_and_auth(
        ConnectParams {
            host: &settings.src.host,
            port: settings.src.port,
            tls: settings.src.tls,
            insecure: settings.src.insecure,
        },
        &src_auth,
    )
    .await?;
    let mut dst = Client::connect_and_auth(
        ConnectParams {
            host: &settings.dst.host,
            port: settings.dst.port,
            tls: settings.dst.tls,
            insecure: settings.dst.insecure,
        },
        &dst_auth,
    )
    .await?;

    let inc = build_globset(&settings.include)?;
    let exc = build_globset(&settings.exclude)?;

    let folders_all = src.list_folders().await?;
    let folders = filter_folders(folders_all, inc.as_ref(), exc.as_ref());

    let opts = SyncOptions {
        max_message_size: settings.max_message_size,
        dry_run: settings.dry_run,
    };

    tracing::info!(folders = folders.len(), "starting migration");
    let mut report = MigrationReport::default();
    for (i, f) in folders.iter().enumerate() {
        tracing::info!(folder = %f, idx = i + 1, total = folders.len(), "==> entering folder");
        match sync_folder(f, &mut src, &mut dst, reporter, &opts).await {
            Ok(stats) => {
                tracing::info!(
                    folder = %f,
                    copied = stats.copied,
                    skipped = stats.skipped,
                    failed = stats.failed,
                    "folder done"
                );
                report.folders.push(stats);
            }
            Err(e) => {
                tracing::error!(folder = %f, error = %e, "folder failed; continuing to next folder");
                report.folders.push(FolderStats {
                    folder: f.clone(),
                    failed: 1,
                    ..Default::default()
                });
            }
        }
    }

    src.logout().await?;
    dst.logout().await?;
    Ok(report)
}

fn resolve_auth(ep: &EndpointSettings) -> Result<Auth> {
    match &ep.auth {
        crate::config::AuthMethod::Login { .. } => from_login(&ep.user, &ep.auth)
            .ok_or_else(|| Error::Config("internal: login resolution failed".into())),
        crate::config::AuthMethod::OAuth2 {
            provider_kind,
            client_id,
            client_secret,
            auth_url,
            token_url,
            scope,
            use_keyring,
        } => {
            let provider = if provider_kind == "custom" {
                Provider::Custom {
                    auth_url: auth_url
                        .clone()
                        .ok_or_else(|| Error::Config("custom oauth needs auth-url".into()))?,
                    token_url: token_url
                        .clone()
                        .ok_or_else(|| Error::Config("custom oauth needs token-url".into()))?,
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
            Ok(Auth::XOAuth2 {
                user: ep.user.clone(),
                access_token: creds.access_token,
            })
        }
    }
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
