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
    async fn users(&self) -> Result<Vec<OwnedUserId>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use eyre::Result;
    use matrix_sdk::ruma::{OwnedUserId, UserId};
    use once_cell::sync::Lazy;
    use serial_test::serial;
    use tempfile::TempDir;

    static TMPDIR: Lazy<TempDir> = Lazy::new(|| tempfile::tempdir().unwrap());
    static DB: Lazy<DatabaseImpl> = Lazy::new(|| DatabaseImpl::new(TMPDIR.path()).unwrap());
    static USER: Lazy<OwnedUserId> = Lazy::new(|| UserId::parse("@abc:example.com").unwrap());
    static PACKAGES: Lazy<Vec<String>> = Lazy::new(|| vec!["abc".to_owned(), "xyz".to_owned()]);

    #[tokio::test]
    #[serial]
    async fn test_subscribe() -> Result<()> {
        DB.init().await?;
        DB.subscribe(&USER, PACKAGES.clone()).await?;
        assert_eq!(DB.get_packages(&USER).await?, PACKAGES.clone());
        assert_eq!(DB.users().await?, vec![USER.clone()]);
        DB.unsubscribe(&USER, vec!["xyz".to_owned()]).await?;
        assert_eq!(DB.get_packages(&USER).await?, vec!["abc".to_owned()]);
        DB.unsubscribe(&USER, vec!["abc".to_owned()]).await?;
        assert_eq!(DB.get_packages(&USER).await?, Vec::<String>::new());
        assert_eq!(DB.users().await?, Vec::<OwnedUserId>::new());

        Ok(())
    }

    #[tokio::test]
    #[serial]
    async fn test_notification() -> Result<()> {
        DB.init().await?;
        DB.enable_notification(&USER).await?;
        assert_eq!(DB.is_notification_enabled(&USER).await?, true);
        assert_eq!(DB.notification_targets().await?, vec![USER.clone()]);
        assert_eq!(DB.users().await?, vec![USER.clone()]);
        DB.disable_notification(&USER).await?;
        assert_eq!(DB.is_notification_enabled(&USER).await?, false);
        assert_eq!(DB.users().await?, Vec::<OwnedUserId>::new());

        Ok(())
    }
}
