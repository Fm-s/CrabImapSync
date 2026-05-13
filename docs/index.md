---
title: CrabImapSync
layout: default
---

# CrabImapSync 🦀✉️

Memory-bounded streaming IMAP-to-IMAP migration CLI written in Rust.

`imapsync` (Perl) consumed 62 GB of RAM migrating a 4.7 GB mailbox on macOS.
CrabImapSync stays under 200 MB regardless of mailbox size.

## Quick start

```bash
cargo install --git https://github.com/felipemsouza/crab-imap-sync

export SRC_PASS='...'
export DST_PASS='...'

crab-imap-sync \
  --src-host imap.source.example --src-user me@source --src-pass-env SRC_PASS \
  --dst-host imap.dest.example   --dst-user me@dest   --dst-pass-env DST_PASS
```

Or download the prebuilt macOS binary from
[Releases](https://github.com/felipemsouza/crab-imap-sync/releases/latest).

## Documentation

- [OAuth2 setup (Gmail / Microsoft / custom)](OAUTH2.html)
- [Smoke test procedure](SMOKE.html)
- [Source code](https://github.com/felipemsouza/crab-imap-sync)
- [Issue tracker](https://github.com/felipemsouza/crab-imap-sync/issues)

## How it works

For each folder on the source:

1. **Index destination** — fetch every Message-Id already on the destination.
2. **Batch-probe source** — one `UID FETCH … HEADER.FIELDS (MESSAGE-ID)` per 500 UIDs.
3. **Classify** — duplicates skipped, missing UIDs queued for copy.
4. **Stream-copy** — one message at a time: fetch full body, APPEND with original flags + INTERNALDATE preserved, advance.

Memory ceiling is set by a single message buffer plus the per-folder
Message-Id set (~1 MB for a 13k-message INBOX).

## Comparison

|                       | imapsync       | CrabImapSync |
|-----------------------|---------------:|-------------:|
| Peak RAM, 4.7 GB box  | 62 GB          | < 200 MB     |
| Language              | Perl           | Rust         |
| Static binary         | no             | yes          |
| Resumable             | yes (UID)      | yes (Message-Id) |
| OAuth2 (browser PKCE) | yes            | yes          |
| Maturity              | 20+ years      | new          |

## License

MIT
