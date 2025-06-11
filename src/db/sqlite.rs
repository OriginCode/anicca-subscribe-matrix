use deadpool_sqlite::{Config, Pool, Runtime};
use eyre::Result;
use matrix_sdk::ruma::{OwnedUserId, UserId};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SqliteDatabase {
    pool: Pool,
}

impl super::Database for SqliteDatabase {
    fn new(data_dir: &Path) -> Result<Self> {
        let cfg = Config::new(data_dir.join("anicca.db"));
        let pool = cfg.create_pool(Runtime::Tokio1)?;
        Ok(Self { pool })
    }

    async fn init(&self) -> Result<()> {
        let db_conn = self.pool.get().await?;
        db_conn
            .interact(|db_conn| {
                db_conn.execute(
                    "CREATE TABLE IF NOT EXISTS subscription ( user_id TEXT NOT NULL, package TEXT NOT NULL )",
                    (),
                )?;
                db_conn.execute(
                    "CREATE TABLE IF NOT EXISTS notification ( user_id TEXT )",
                    (),
                )?;
                Ok::<(), rusqlite::Error>(())
            })
            .await
            .unwrap()?;
        Ok(())
    }

    async fn get_packages(&self, user_id: &UserId) -> Result<Vec<String>> {
        let db_conn = self.pool.get().await?;
        let user_id_str = user_id.to_string();
        let packages = db_conn
            .interact(move |db_conn| {
                let mut stmt =
                    db_conn.prepare("SELECT package FROM subscription WHERE user_id = ?1")?;
                let rows = stmt.query_map([&user_id_str], |row| row.get(0))?;
                let mut packages: Vec<String> = Vec::new();
                for row in rows {
                    packages.push(row?);
                }
                Ok::<Vec<String>, rusqlite::Error>(packages)
            })
            .await
            .unwrap()?;
        Ok(packages)
    }

    async fn subscribe(&self, user_id: &UserId, packages: Vec<String>) -> Result<()> {
        let db_conn = self.pool.get().await?;
        let user_id_str = user_id.to_string();
        db_conn
            .interact(move |db_conn| {
                let transaction = db_conn.transaction()?;
                let mut stmt = transaction
                    .prepare("INSERT INTO subscription (user_id, package) VALUES (?1, ?2)")?;
                for package in packages {
                    stmt.execute([&user_id_str, &package])?;
                }
                drop(stmt);
                transaction.commit()?;
                Ok::<(), rusqlite::Error>(())
            })
            .await
            .unwrap()?;
        Ok(())
    }

    async fn unsubscribe(&self, user_id: &UserId, packages: Vec<String>) -> Result<()> {
        let db_conn = self.pool.get().await?;
        let user_id_str = user_id.to_string();
        db_conn
            .interact(move |db_conn| {
                let transaction = db_conn.transaction()?;
                let mut stmt = transaction
                    .prepare("DELETE FROM subscription WHERE user_id = ?1 AND package = ?2")?;
                for package in packages {
                    stmt.execute([&user_id_str, &package])?;
                }
                drop(stmt);
                transaction.commit()?;
                Ok::<(), rusqlite::Error>(())
            })
            .await
            .unwrap()?;
        Ok(())
    }

    async fn is_notification_enabled(&self, user_id: &UserId) -> Result<bool> {
        let db_conn = self.pool.get().await?;
        let user_id_str = user_id.as_str().to_owned();
        let count: i32 = db_conn
            .interact(move |db_conn| {
                let mut stmt =
                    db_conn.prepare("SELECT COUNT(*) FROM notification WHERE user_id = ?1")?;
                stmt.query_row([&user_id_str], |row| row.get(0))
            })
            .await
            .unwrap()?;
        Ok(count > 0)
    }

    async fn enable_notification(&self, user_id: &UserId) -> Result<()> {
        let db_conn = self.pool.get().await?;
        let user_id_str = user_id.to_string();
        db_conn
            .interact(move |db_conn| {
                db_conn.execute(
                    "INSERT INTO notification (user_id) VALUES (?1)",
                    [&user_id_str],
                )
            })
            .await
            .unwrap()?;
        Ok(())
    }

    async fn disable_notification(&self, user_id: &UserId) -> Result<()> {
        let db_conn = self.pool.get().await?;
        let user_id_str = user_id.to_string();
        db_conn
            .interact(move |db_conn| {
                db_conn.execute(
                    "DELETE FROM notification WHERE user_id = ?1",
                    [&user_id_str],
                )
            })
            .await
            .unwrap()?;
        Ok(())
    }

    async fn notification_targets(&self) -> Result<Vec<OwnedUserId>> {
        let db_conn = self.pool.get().await?;
        Ok(db_conn
            .interact(|db_conn| {
                let mut stmt = db_conn.prepare("SELECT user_id FROM notification")?;
                let mut rows = stmt.query([])?;

                let mut targets = Vec::new();
                while let Some(row) = rows.next()? {
                    let user_id = UserId::parse(row.get::<_, String>(0)?).unwrap();
                    targets.push(user_id);
                }
                Ok::<Vec<OwnedUserId>, rusqlite::Error>(targets)
            })
            .await
            .unwrap()?)
    }
}
