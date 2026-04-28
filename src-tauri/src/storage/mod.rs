// Persistence: SQLite, encrypted vault, E-WBN archiving, storage budget.
pub mod db;
pub mod vault;
pub mod freeze;
pub mod guard;
pub mod versioning;

pub use db::Db;
pub use vault::VaultStore;
pub use freeze::get_or_init_master_key;
pub use guard::{StorageBudget, StorageReport};
pub mod warc_export;
