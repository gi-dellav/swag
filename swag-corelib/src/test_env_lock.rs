//! Test-only mutex serializing tests that mutate process env (`SWAG_HOME`).

use std::sync::Mutex;

/// Serialize tests that mutate process env (`SWAG_HOME`).
pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());
