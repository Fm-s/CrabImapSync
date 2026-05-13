# CrabImapSync — Design Spec

**Status:** Draft v1
**Date:** 2026-05-13
**License:** MIT

## Problem

Existing IMAP migration tools (`imapsync`, `offlineimap`) have severe memory bloat on large mailboxes — observed `imapsync` consuming **62 GB of RAM** while migrating a 4.7 GB mailbox (~13× the data size), triggering macOS jetsam SIGKILLs. The root cause is loading full header tables for both sides into memory before transferring. Workarounds (`--useuid`, `--exitwhenover`, chunked re-runs) make migrations slow and operationally painful.

CrabImapSync is a memory-bounded, streaming IMAP-to-IMAP migration CLI written in Rust. It transfers messages one at a time, keeping memory proportional to a single message plus a per-folder Message-Id set — not to the entire mailbox.

## Non-Goals

- General-purpose mail client (use `himalaya` or Thunderbird).
- POP3 / Exchange Web Services / proprietary protocols.
- Bidirectional sync. CrabImapSync is one-way: source → destination.
- Multi-account batch in a single binary invocation. Users wrap with shell loops (same UX as imapsync).
- A library API. v1 ships a CLI binary only. The internals are organized as modules but no stable public Rust API is promised.

## High-level Architecture

```
        ┌──────────────────────┐
        │   clap (CLI args)    │
        └──────────┬───────────┘
                   ▼
        ┌──────────────────────┐
        │  config::Settings    │  ← merges CLI + env vars
        └──────────┬───────────┘
                   ▼
        ┌──────────────────────┐         ┌────────────────────┐
        │      auth::Auth      │◄────────┤ oauth (browser PKCE)│
        └──────────┬───────────┘         └────────────────────┘
                   ▼
   ┌───────────────────────────────────┐
   │  imap_client::Client (src + dst)  │  async-imap + tokio-rustls
   └────┬──────────────────────────────┘
        ▼
   ┌───────────────────────────────────┐
   │       sync::run_migration         │  ◄── streaming, per-folder
   └────┬──────────────────────────────┘
        ▼
   ┌───────────────────────────────────┐
   │   progress::Reporter (indicatif)  │
   └───────────────────────────────────┘
```

Module ownership:

| Module | Responsibility |
|---|---|
| `main.rs` | Entrypoint. Parse args, set up tracing, dispatch to `sync::run_migration`, map errors to exit codes. |
| `cli.rs` | `clap` derive struct. Argument grouping (src-*, dst-*). Validation rules. |
| `config.rs` | Resolves CLI args + env vars + secure prompts into a `Settings` struct. Reads passwords from env vars only (never as direct flags). |
| `auth.rs` | `enum Auth { Login { user, pass }, XOAuth2 { user, access_token } }`. Builders that take `Settings` and produce an `Auth`. |
| `oauth.rs` | OAuth2 + PKCE browser flow: opens system browser, runs local `tiny_http` callback listener, exchanges code for tokens, stores refresh token in keyring. |
| `imap_client.rs` | Thin wrapper over `async-imap`: `connect_tls`, `authenticate`, `list_folders`, `select`, `examine`, `search_uids`, `fetch_message`, `fetch_message_ids`, `append`. |
| `sync.rs` | The migration orchestrator (see below). |
| `progress.rs` | Wraps `indicatif::MultiProgress` with an overall bar (folders) and a per-folder bar (messages). |
| `error.rs` | `thiserror` enum; conversions from `async_imap::error::Error`, IO, OAuth, etc. |

## CLI Surface

```text
crab-imap-sync [OPTIONS]

REQUIRED:
  --src-host <HOST>                 Source IMAP host
  --src-user <USER>                 Source username
  --dst-host <HOST>                 Destination IMAP host
  --dst-user <USER>                 Destination username

CONNECTION (per side, prefix --src-* / --dst-*):
  --*-port <PORT>                   IMAP port (default 993)
  --*-tls <MODE>                    none | starttls | imaps (default imaps)
  --*-insecure                      Skip TLS cert verification (NOT for prod)

AUTH (per side):
  --*-auth <KIND>                   login | oauth2 (default login)

  # login
  --*-pass-env <ENVVAR>             Env var holding password (required for login)

  # oauth2
  --*-oauth-provider <NAME>         gmail | microsoft | custom
  --*-oauth-client-id <ID>          OAuth2 client ID
  --*-oauth-client-secret-env <ENV> Env var holding client secret (may be empty for public clients)
  --*-oauth-auth-url <URL>          For provider=custom
  --*-oauth-token-url <URL>         For provider=custom
  --*-oauth-scope <SCOPE>           Comma-separated scopes (default per provider)
  --*-oauth-no-keyring              Skip refresh-token persistence

SYNC OPTIONS:
  --include <PATTERN>...            Folder include patterns (glob; repeatable)
  --exclude <PATTERN>...            Folder exclude patterns (glob; repeatable)
  --max-message-size <BYTES>        Skip messages larger than this
  --dry-run                         Show what would be transferred; don't write
  --timeout-secs <N>                Per-IMAP-op timeout (default 300)
  --retries <N>                     Network retries with exp backoff (default 3)

OUTPUT:
  -v / --verbose                    More detailed logs
  --quiet                           Only errors
  --no-progress                     Disable progress bars (CI / logging mode)
  --log-file <PATH>                 Tee logs to file
```

Secrets policy: passwords/tokens **never** as direct CLI flags. Only via env vars or OAuth2 browser flow. This avoids leaking via `ps`, shell history, and process inspection.

## Sync Algorithm

For each invocation (one source → one destination account):

```
1. Connect TLS to source & destination.
2. Authenticate both (LOGIN or XOAUTH2).
3. LIST folders on source.
4. Apply --include / --exclude filters.
5. For each remaining folder F:
     a. EXAMINE F on source (read-only).
     b. CREATE F on destination if missing. Folder names are matched verbatim (case-sensitive). `CREATE` returning "already exists" is treated as success.
     c. SELECT F on destination.
     d. Fetch destination Message-Ids:
        UID SEARCH ALL → list of dst UIDs
        UID FETCH ... BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)]
        Parse Message-Id headers → HashSet<String> `dst_ids`.
     e. UID SEARCH ALL on source → list of src UIDs.
     f. Initialize per-folder progress bar (total = len(src_uids)).
     g. For each src UID (streaming, no batch):
          i.   FETCH ENVELOPE → extract Message-Id.
          ii.  If Message-Id ∈ dst_ids: increment bar (skipped), continue.
          iii. FETCH BODY.PEEK[] + INTERNALDATE + FLAGS.
               Stream body to in-memory Vec<u8> (bounded by --max-message-size).
          iv.  APPEND to destination F with preserved flags + internal date.
          v.   Add Message-Id to dst_ids (defensive against same-folder dupes).
          vi.  Increment bar.
     h. Close folder selection on both sides.
6. LOGOUT both connections cleanly.
7. Print summary: per-folder copied / skipped / failed counts; total bytes.
```

Memory profile: O(1) per active message buffer + O(|dst_ids|) per folder (released between folders). For a 13k-message folder with 80-byte Message-Ids: ~1 MB. Across all folders: peak ~ 1 MB + biggest message.

Failure handling per message:
- Individual fetch/append failures: log structured error, increment failure counter, **continue** to next message.
- Network connection drop: retry with exponential backoff (default 3 tries: 1s, 4s, 16s).
- Persistent auth failure: abort the run with non-zero exit code.

Exit codes:
| Code | Meaning |
|---|---|
| 0 | All folders synced; zero per-message failures |
| 1 | Synced with some per-message failures (count in summary) |
| 2 | Config / argument error |
| 3 | Authentication failure |
| 4 | Network failure exhausted retries |
| 5 | TLS / certificate error |

## OAuth2 Browser Flow

For Gmail and Microsoft 365, password auth is deprecated. CrabImapSync ships a real PKCE flow:

```
1. User runs: crab-imap-sync --src-auth oauth2 --src-oauth-provider gmail ...
2. CrabImapSync checks keyring for stored refresh_token (key = "crabimap:<provider>:<user>").
   If present: refresh access_token, use it, done.
3. If not present (or refresh fails):
     a. Generate PKCE code_verifier + code_challenge.
     b. Bind tiny_http listener on 127.0.0.1:RANDOM_PORT.
     c. Construct provider auth URL with:
          client_id, redirect_uri=http://127.0.0.1:PORT/cb,
          scope (e.g. https://mail.google.com/), state, code_challenge.
     d. Open URL in system browser via `webbrowser` crate.
     e. User completes consent in browser.
     f. Provider redirects to local callback with code + state.
     g. Verify state, exchange code for tokens at provider token endpoint.
     h. Store refresh_token in keyring (unless --*-oauth-no-keyring).
     i. Show "Authentication successful — you can close this tab" in browser.
4. Use access_token in IMAP via SASL XOAUTH2 mechanism.
```

Provider presets:
| Provider | Auth URL | Token URL | Default scope |
|---|---|---|---|
| gmail | `https://accounts.google.com/o/oauth2/v2/auth` | `https://oauth2.googleapis.com/token` | `https://mail.google.com/` |
| microsoft | `https://login.microsoftonline.com/common/oauth2/v2.0/authorize` | `https://login.microsoftonline.com/common/oauth2/v2.0/token` | `https://outlook.office.com/IMAP.AccessAsUser.All offline_access` |
| custom | user-supplied | user-supplied | user-supplied |

User responsibility: register their own OAuth2 app at the provider's console and pass `--*-oauth-client-id`/`--*-oauth-client-secret-env`. README documents the steps for Gmail and Microsoft.

## Dependencies

```toml
[dependencies]
# Async runtime + IMAP
tokio = { version = "1", features = ["rt-multi-thread", "macros", "net", "io-util", "time", "fs"] }
async-imap = "0.10"
tokio-rustls = "0.26"
rustls-pemfile = "2"
webpki-roots = "0.26"

# CLI
clap = { version = "4", features = ["derive", "env"] }

# OAuth2
oauth2 = "5"
tiny_http = "0.12"
webbrowser = "1"
keyring = "3"
url = "2"

# UI
indicatif = "0.17"

# Errors / logging
thiserror = "1"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Parsing
mail-parser = "0.9"
globset = "0.4"     # --include / --exclude glob matching

[dev-dependencies]
testcontainers = "0.23"
tempfile = "3"
```

All deps are pure Rust where possible (`rustls` not OpenSSL). The binary should build statically on Linux/macOS/Windows without external system libs.

## Testing Strategy

**Unit tests** (in each module):
- `cli`: parsing valid + invalid argument combinations.
- `auth`: building Auth from Settings; rejection of password-as-flag.
- `oauth`: state validation, PKCE challenge generation. No live OAuth calls.
- `sync`: filter logic (include/exclude); pure helper functions.

**Integration tests** (`tests/`):
- Spin up two `dovecot` containers via `testcontainers`.
- Seed source with fixture messages (varied folders, sizes, flags).
- Run the binary against source+destination.
- Assert destination matches expected message set; rerun shows zero new copies (idempotency).

**Smoke (manual, documented)**:
- The user's own cPanel → Hostinger migration. Documented in `docs/SMOKE.md`.

## Repo Layout

```
crab-imap-sync/
├── Cargo.toml
├── Cargo.lock
├── LICENSE                 # MIT
├── README.md               # Quickstart, comparison vs imapsync, OAuth2 setup
├── CHANGELOG.md
├── .gitignore
├── .github/
│   └── workflows/
│       └── ci.yml          # cargo fmt + clippy + test (Linux + macOS)
├── src/
│   ├── main.rs
│   ├── cli.rs
│   ├── config.rs
│   ├── auth.rs
│   ├── oauth.rs
│   ├── imap_client.rs
│   ├── sync.rs
│   ├── progress.rs
│   └── error.rs
├── tests/
│   ├── common/
│   │   └── mod.rs          # testcontainers helpers
│   └── integration.rs
└── docs/
    ├── OAUTH2.md           # Gmail / Microsoft app registration walkthrough
    └── SMOKE.md
```

## v1 Release Criteria

- Migration completes between two plain-IMAP servers (LOGIN auth) with > 5 GB / 13k messages without exceeding 200 MB of process RSS.
- OAuth2 to Gmail works end-to-end (manual smoke).
- Idempotent: second run after success transfers zero messages.
- All folders preserved with original flags + internal dates.
- `cargo clippy -- -D warnings` and `cargo fmt --check` clean.
- README explains setup, usage, and limitations.

## Open Questions

None blocking. Design is ready for implementation planning.

## Out of Scope (parking lot for v2+)

- Parallel folder transfer (worker pool).
- SQLite-backed state cache for faster resume.
- Bidirectional / mirror sync.
- TOML config for multi-account batch.
- Web UI / TUI.
- Source mailbox deletion after successful copy (`--delete-source`).
- Migration from POP3 or Maildir.
