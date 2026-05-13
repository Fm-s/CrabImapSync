# Smoke Test: cPanel → Hostinger

Manual procedure to verify CrabImapSync handles a real IMAP migration with
bounded memory.

## Setup

Build the release binary:

```bash
cargo build --release
```

Export passwords (never hardcode):

```bash
export SRC_PASS='<SRC_PASSWORD_HERE>'
export DST_PASS='<DST_PASSWORD_HERE>'
```

## Dry run first

```bash
target/release/crab-imap-sync \
  --src-host imap.source.example \
  --src-user me@source \
  --src-pass-env SRC_PASS \
  --dst-host imap.dest.example \
  --dst-user me@source \
  --dst-pass-env DST_PASS \
  --dry-run -v
```

Expected: connects to both servers, lists folders, prints what would be
copied. RSS stays under 200 MB throughout.

## Real run on a tiny mailbox

Pick the smallest source mailbox (e.g. `almoxarifado01` — 81 KB):

```bash
target/release/crab-imap-sync \
  --src-host imap.source.example \
  --src-user me@source \
  --src-pass-env SRC_PASS \
  --dst-host imap.dest.example \
  --dst-user me@source \
  --dst-pass-env DST_PASS
```

Verify in Hostinger webmail that messages arrive.

## Memory ceiling check on the big one

`contato@` has ~13k messages / 4.7 GB. Monitor RSS:

```bash
target/release/crab-imap-sync \
  --src-host imap.source.example \
  --src-user me@source \
  --src-pass-env SRC_PASS \
  --dst-host imap.dest.example \
  --dst-user me@source \
  --dst-pass-env DST_PASS &
PID=$!
while kill -0 $PID 2>/dev/null; do
  ps -o rss= -p $PID | awk '{ printf "RSS: %.0f MB\n", $1/1024 }'
  sleep 30
done
```

PASS criterion: RSS never exceeds 200 MB. Exit code 0.

## Re-run idempotency

Run the same command a second time after success. Expected: copied=0 in the
summary (all messages already deduped by Message-Id).
