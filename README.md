# CrabImapSync 🦀✉️

Memory-bounded, streaming IMAP-to-IMAP migration CLI written in Rust.

## Why

`imapsync` is the de-facto IMAP migration tool, but its default behavior loads
the full message-header tables of both servers into RAM. On a 4.7 GB mailbox
this was observed consuming **62 GB of memory** and was SIGKILLed by the macOS
OOM killer. Workarounds (`--useuid`, `--exitwhenover`, chunked re-runs) help
but turn the migration into a babysitting exercise.

CrabImapSync transfers messages one at a time. Peak memory stays around
~200 MB regardless of mailbox size.

## Install

```bash
cargo install --git https://github.com/felipemsouza/crab-imap-sync
```

Or from source:

```bash
git clone https://github.com/felipemsouza/crab-imap-sync
cd crab-imap-sync
cargo build --release
# binary at target/release/crab-imap-sync
```

## Usage (LOGIN auth)

```bash
export SRC_PASS='...'
export DST_PASS='...'

crab-imap-sync \
  --src-host imap.source.example --src-user me@source --src-pass-env SRC_PASS \
  --dst-host imap.dest.example   --dst-user me@dest   --dst-pass-env DST_PASS
```

Passwords are NEVER passed as direct flags — only via env vars (avoids leaking
through `ps` / shell history / process inspection).

Filter folders:

```bash
crab-imap-sync ... --include 'INBOX*' --exclude 'Trash'
```

Dry-run first:

```bash
crab-imap-sync ... --dry-run
```

## Usage (OAuth2, e.g. Gmail)

Register your own OAuth2 client at
<https://console.cloud.google.com/apis/credentials> (full guide:
[docs/OAUTH2.md](docs/OAUTH2.md)). Set the redirect URI to
`http://127.0.0.1:*` and IMAP scope `https://mail.google.com/`.

```bash
export DST_OAUTH_SECRET='your-client-secret'
crab-imap-sync \
  --src-host imap.source.example --src-user me@source --src-pass-env SRC_PASS \
  --dst-host imap.gmail.com --dst-user me@gmail.com \
  --dst-auth oauth2 --dst-oauth-provider gmail \
  --dst-oauth-client-id 'YOUR-CLIENT-ID.apps.googleusercontent.com' \
  --dst-oauth-client-secret-env DST_OAUTH_SECRET
```

On first run, a browser opens for consent; the refresh token is persisted in
your OS keyring so subsequent runs skip the browser.

## Comparison with imapsync

| | imapsync | CrabImapSync |
|---|---|---|
| Peak RAM (4.7 GB mailbox) | 62 GB | <200 MB |
| Language | Perl | Rust |
| Dependency footprint | CPAN + libssl | static binary |
| Resumable | yes (UID) | yes (Message-Id) |
| OAuth2 | yes | yes (browser PKCE flow) |
| Maturity | 20+ years | new |

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Synced cleanly |
| 1 | Synced with some per-message failures |
| 2 | Config / argument error |
| 3 | Authentication failure |
| 4 | Network failure |
| 5 | TLS / certificate error |

## Status

v0.1 — usable for plain LOGIN migrations. OAuth2 PKCE flow implemented but
not yet auto-tested. Integration tests against Dovecot containers pending.

## License

MIT
