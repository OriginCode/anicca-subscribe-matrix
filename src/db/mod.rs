use eyre::Result;
use matrix_sdk::ruma::{OwnedUserId, UserId};
use std::path::Path;

#[cfg(feature = "rocksdb")]
mod rocksdb;
#[cfg(feature = "sqlite")]
mod sqlite;

#[cfg(feature = "sqlite")]
pub type DatabaseImpl = sqlite::SqliteDatabase;
#[cfg(feature = "rocksdb")]
pub type DatabaseImpl = rocksdb::RocksDbDatabase;

pub trait Database: Clone + Sync + Send {
    fn new(data: &Path) -> Result<Self>;
    async fn init(&self) -> Result<()>;
    async fn get_packages(&self, user_id: &UserId) -> Result<Vec<String>>;
    async fn subscribe(&self, user_id: &UserId, packages: Vec<String>) -> Result<()>;
    async fn unsubscribe(&self, user_id: &UserId, packages: Vec<String>) -> Result<()>;
    async fn is_notification_enabled(&self, user_id: &UserId) -> Result<bool>;
    async fn enable_notification(&self, user_id: &UserId) -> Result<()>;
    async fn disable_notification(&self, user_id: &UserId) -> Result<()>;
    async fn notification_targets(&self) -> Result<Vec<OwnedUserId>>;
}
