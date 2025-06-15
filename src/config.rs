use matrix_sdk::ruma::{OwnedUserId, UserId};
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};
use tokio::fs;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Config {
    pub admin_ids: Arc<[OwnedUserId]>,
    pub data_dir: Option<Arc<Path>>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            admin_ids: Arc::new([]),
            data_dir: None,
        }
    }
}

impl Config {
    pub async fn load(path: Option<&Path>) -> Self {
        let cfg = toml::from_str(
            &fs::read_to_string(path.unwrap_or(Path::new("./config.toml")))
                .await
                .unwrap_or_default(),
        )
        .unwrap_or_default();
        dbg!(&cfg);
        cfg
    }

    pub fn is_admin(&self, user_id: &UserId) -> bool {
        for id in self.admin_ids.iter() {
            if id == user_id {
                return true;
            }
        }
        false
    }
}
