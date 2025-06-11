use eyre::Result;
use matrix_sdk::ruma::{OwnedUserId, UserId};

mod sqlite;

#[cfg(feature = "sqlite")]
pub type DatabaseImpl = sqlite::SqliteDatabase;

pub trait Database: Clone + Sync + Send {
    async fn init(&self) -> Result<()>;
    async fn get_packages(&self, user_id: &UserId) -> Result<Vec<String>>;
    async fn subscribe(&self, user_id: &UserId, packages: Vec<String>) -> Result<()>;
    async fn unsubscribe(&self, user_id: &UserId, packages: Vec<String>) -> Result<()>;
    async fn is_notification_enabled(&self, user_id: &UserId) -> Result<bool>;
    async fn enable_notification(&self, user_id: &UserId) -> Result<()>;
    async fn disable_notification(&self, user_id: &UserId) -> Result<()>;
    async fn notification_targets(&self) -> Result<Vec<OwnedUserId>>;
}
