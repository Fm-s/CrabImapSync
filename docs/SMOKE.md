---
title: Smoke test
layout: default
---

# Smoke Test

Manual procedure to verify CrabImapSync handles a real IMAP migration with
bounded memory.

## Setup

Build the release binary:

```bash
cargo build --release
```

Export passwords as environment variables (never hardcode them in shell history):

```bash
export SRC_PASS='your-source-password'
export DST_PASS='your-destination-password'
```

## Dry run first

```bash
target/release/crab-imap-sync \
  --src-host imap.source.example --src-user me@source --src-pass-env SRC_PASS \
  --dst-host imap.dest.example   --dst-user me@dest   --dst-pass-env DST_PASS \
  --dry-run -v
```

Expected: connects to both servers, lists folders, prints what would be
copied. RSS stays under 200 MB (`ps -o rss=` while it runs).

## Real run on a small mailbox first

Pick the smallest source mailbox you have. Verify in the destination's webmail
that messages arrive with original flags + dates preserved.

## Memory ceiling check on a large mailbox

For a mailbox with thousands of messages, monitor RSS while it runs:

```bash
target/release/crab-imap-sync \
  --src-host imap.source.example --src-user big@source --src-pass-env SRC_PASS \
  --dst-host imap.dest.example   --dst-user big@dest   --dst-pass-env DST_PASS &
PID=$!
while kill -0 $PID 2>/dev/null; do
  ps -o rss= -p $PID | awk '{ printf "RSS: %.0f MB\n", $1/1024 }'
  sleep 30
done
```

PASS criterion: RSS never exceeds 200 MB. Exit code 0.

## Re-run idempotency

Run the same command a second time after success. Expected: `copied=0` in the
summary — all messages already deduped by Message-Id.
