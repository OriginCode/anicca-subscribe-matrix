use eyre::Result;
use matrix_sdk::ruma::{OwnedUserId, UserId};
use rocksdb::{DBWithThreadMode, MultiThreaded, Options};
use std::{path::Path, sync::Arc};
use tokio::task::spawn_blocking;
use wincode::{SchemaRead, SchemaWrite, config};

type WincodeConfig = config::Configuration<
    true,
    { usize::MAX },
    wincode::len::BincodeLen,
    wincode::int_encoding::LittleEndian,
    wincode::int_encoding::VarInt,
    u32,
>;

#[derive(Clone)]
pub struct RocksDbDatabase {
    db: Arc<DBWithThreadMode<MultiThreaded>>,
    wincode_config: WincodeConfig,
}

#[derive(SchemaWrite, SchemaRead, Debug, Clone, Default)]
pub struct User {
    packages: Vec<String>,
    notification_enabled: bool,
}

impl RocksDbDatabase {
    async fn get_user_data(&self, user_id: &UserId) -> Result<Option<User>> {
        let user_id_str = user_id.to_string();
        let db = self.db.clone();
        let data = spawn_blocking(move || db.get(user_id_str.as_bytes())).await??;
        if let Some(data) = data {
            Ok(Some(config::deserialize::<User, WincodeConfig>(
                &data,
                self.wincode_config,
            )?))
        } else {
            Ok(None)
        }
    }

    async fn get_user_data_or_create(&self, user_id: &UserId) -> Result<User> {
        Ok(self
            .get_user_data(user_id)
            .await?
            .unwrap_or(User::default()))
    }
}

impl super::Database for RocksDbDatabase {
    fn new(data_dir: &Path) -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);

        let db = Arc::new(DBWithThreadMode::<MultiThreaded>::open(
            &opts,
            data_dir.join("anicca"),
        )?);
        let wincode_config = config::Configuration::default()
            .disable_preallocation_size_limit()
            .with_varint_encoding();
        Ok(Self { db, wincode_config })
    }

    async fn init(&self) -> Result<()> {
        // RocksDB does not require explicit table creation like SQLite.
        Ok(())
    }

    async fn get_packages(&self, user_id: &UserId) -> Result<Vec<String>> {
        if let Some(user) = self.get_user_data(user_id).await? {
            Ok(user.packages)
        } else {
            Ok(Vec::new())
        }
    }

    async fn subscribe(&self, user_id: &UserId, packages: Vec<String>) -> Result<()> {
        let mut user = self.get_user_data_or_create(user_id).await?;
        user.packages.extend(packages);
        user.packages.dedup();
        let encoded = config::serialize(&user, self.wincode_config)?;
        let user_id_str = user_id.to_string();
        let db = self.db.clone();
        spawn_blocking(move || db.put(user_id_str.as_bytes(), encoded)).await??;
        Ok(())
    }

    async fn unsubscribe(&self, user_id: &UserId, packages: Vec<String>) -> Result<()> {
        if let Some(mut user) = self.get_user_data(user_id).await? {
            user.packages.retain(|pkg| !packages.contains(pkg));
            let encoded = config::serialize(&user, self.wincode_config)?;
            let user_id_str = user_id.to_string();
            let db = self.db.clone();
            if user.packages.is_empty() && !user.notification_enabled {
                spawn_blocking(move || db.delete(user_id_str.as_bytes())).await??;
            } else {
                spawn_blocking(move || db.put(user_id_str.as_bytes(), encoded)).await??;
            }
        }
        Ok(())
    }

    async fn is_notification_enabled(&self, user_id: &UserId) -> Result<bool> {
        let user = self.get_user_data_or_create(user_id).await?;
        Ok(user.notification_enabled)
    }

    async fn enable_notification(&self, user_id: &UserId) -> Result<()> {
        let mut user = self.get_user_data_or_create(user_id).await?;
        user.notification_enabled = true;
        let encoded = config::serialize(&user, self.wincode_config)?;
        let user_id_str = user_id.to_string();
        let db = self.db.clone();
        spawn_blocking(move || db.put(user_id_str.as_bytes(), encoded)).await??;
        Ok(())
    }

    async fn disable_notification(&self, user_id: &UserId) -> Result<()> {
        if let Some(mut user) = self.get_user_data(user_id).await? {
            user.notification_enabled = false;
            let encoded = config::serialize(&user, self.wincode_config)?;
            let user_id_str = user_id.to_string();
            let db = self.db.clone();
            if user.packages.is_empty() && !user.notification_enabled {
                spawn_blocking(move || db.delete(user_id_str.as_bytes())).await??;
            } else {
                spawn_blocking(move || db.put(user_id_str.as_bytes(), encoded)).await??;
            }
        }
        Ok(())
    }

    async fn notification_targets(&self) -> Result<Vec<OwnedUserId>> {
        let db = self.db.clone();
        let wincode_config = self.wincode_config;
        Ok(spawn_blocking(move || {
            let mut targets = Vec::new();
            let iter = db.iterator(rocksdb::IteratorMode::Start);
            for item in iter {
                let (key, val) = item?;
                if let Ok(user_id) = str::from_utf8(&key) {
                    if let Ok(user) =
                        config::deserialize::<User, WincodeConfig>(&val, wincode_config)
                    {
                        if user.notification_enabled {
                            targets.push(UserId::parse(user_id).unwrap());
                        }
                    }
                }
            }
            Ok::<Vec<OwnedUserId>, rocksdb::Error>(targets)
        })
        .await??)
    }

    async fn users(&self) -> Result<Vec<OwnedUserId>> {
        let db = self.db.clone();
        Ok(spawn_blocking(move || {
            let mut users = Vec::new();
            let iter = db.iterator(rocksdb::IteratorMode::Start);
            for item in iter {
                let key = item?.0;
                if let Ok(user_id) = str::from_utf8(&key) {
                    users.push(UserId::parse(user_id).unwrap());
                }
            }
            Ok::<Vec<OwnedUserId>, rocksdb::Error>(users)
        })
        .await??)
    }
}
