use eyre::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

const PKGSUPDATE_JSON_URL: &str =
    "https://raw.githubusercontent.com/AOSC-Dev/anicca/main/pkgsupdate.json";
const PKGSUPDATE_JSON_FILENAME_DIFF: &str = "anicca_diff.json";
const PKGSUPDATE_JSON_FILENAME: &str = "anicca.json";

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub before: String,
    pub after: String,
    pub path: String,
    pub warnings: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Anicca(Vec<Package>);

impl Anicca {
    pub async fn fetch_json(data_dir: &Path) -> Result<()> {
        let file_path = data_dir.join(PKGSUPDATE_JSON_FILENAME);
        let diff_path = data_dir.join(PKGSUPDATE_JSON_FILENAME_DIFF);

        let client = Client::default();
        let content = client
            .get(PKGSUPDATE_JSON_URL)
            .send()
            .await?
            .bytes()
            .await?;

        if file_path.exists() {
            let current_data = serde_json::from_slice::<Anicca>(&content)?;
            let past_data = serde_json::from_slice::<Anicca>(&fs::read(&file_path).await?)?;
            let diff = current_data.diff(&past_data);
            fs::write(&diff_path, serde_json::to_string(&diff.0)?).await?;
            fs::write(&file_path, content).await?;
        } else {
            fs::write(&file_path, content).await?;
            fs::copy(&file_path, &diff_path).await?;
        }

        Ok(())
    }

    pub async fn get_local_json(data_dir: &Path) -> Result<Self> {
        let file_path = data_dir.join(PKGSUPDATE_JSON_FILENAME);
        if !file_path.exists() {
            Self::fetch_json(data_dir).await?;
        }
        let content = fs::read(file_path).await?;
        Ok(serde_json::from_slice(&content)?)
    }

    pub async fn get_diff(data_dir: &Path) -> Result<Self> {
        let file_path = data_dir.join(PKGSUPDATE_JSON_FILENAME_DIFF);
        if !file_path.exists() {
            Self::fetch_json(data_dir).await?;
        }
        let content = fs::read(file_path).await?;
        Ok(serde_json::from_slice(&content)?)
    }

    pub fn get_updates(data: &Self, packages: &[String]) -> Result<Vec<Package>> {
        let updates = data
            .0
            .iter()
            .filter(|pkg| packages.contains(&pkg.name))
            .cloned()
            .collect::<Vec<Package>>();

        Ok(updates)
    }

    pub fn get_subscription_updates(&self, packages: &[String]) -> Result<Vec<Package>> {
        Self::get_updates(self, packages)
    }

    fn diff(&self, past_data: &Self) -> Self {
        Self(
            self.0
                .iter()
                .filter(|pkg| !past_data.0.contains(pkg))
                .cloned()
                .collect::<Vec<Package>>(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Anicca;
    use eyre::Result;
    use std::path::Path;

    #[tokio::test]
    async fn test_fetch() -> Result<()> {
        Anicca::fetch_json(Path::new(".")).await?;
        Ok(())
    }

    #[tokio::test]
    async fn test_read() -> Result<()> {
        Anicca::get_local_json(Path::new(".")).await?;
        Ok(())
    }
}
