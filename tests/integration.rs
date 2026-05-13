mod common;

#[tokio::test]
#[ignore] // requires Docker; see tests/common/mod.rs
async fn end_to_end_minimal_sync() {
    // Pending: start 2 Dovecot containers, append fixture .eml to source INBOX,
    // run sync::run_migration, assert destination INBOX has the same Message-Id.
}
