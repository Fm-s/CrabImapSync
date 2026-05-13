// Test harness for IMAP integration tests.
//
// Intended approach: spin up two IMAP server containers (e.g. dovecot/dovecot
// or analogic/poste.io) via testcontainers, seed source with fixture messages,
// run crab-imap-sync as a library function, assert destination state.
//
// Filling out the live harness is half-day work — left for follow-up once the
// manual smoke (docs/SMOKE.md) confirms behavior against real servers.

#[allow(dead_code)]
pub struct DovecotInstance {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
}
