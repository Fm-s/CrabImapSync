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

    tracing::info!(folder, "indexing destination message-ids (this may take a while for large folders)");
    dst.select_for_write(folder).await?;
    let mut dst_ids = dst.fetch_all_message_ids().await.unwrap_or_default();
    tracing::info!(folder, dst_count = dst_ids.len(), "destination indexed");

    tracing::info!(folder, "listing source UIDs");
    src.examine(folder).await?;
    let src_uids = src.search_all_uids().await?;
    tracing::info!(folder, src_count = src_uids.len(), "starting message transfer");

    let bar = reporter.new_folder_bar(folder, src_uids.len() as u64);

    for uid in src_uids {
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
                let dup = msg
                    .message_id
                    .as_ref()
                    .map(|m| dst_ids.contains(m))
                    .unwrap_or(false);
                if dup {
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
    }
    bar.finish();
    Ok(stats)
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
        let stats = sync_folder(f, &mut src, &mut dst, reporter, &opts).await?;
        tracing::info!(
            folder = %f,
            copied = stats.copied,
            skipped = stats.skipped,
            failed = stats.failed,
            "folder done"
        );
        report.folders.push(stats);
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
