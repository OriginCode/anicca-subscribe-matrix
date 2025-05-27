use eyre::Result;
use reqwest::Client;
use serde::Deserialize;
use std::path::Path;
use tokio::fs;

const PKGSUPDATE_JSON_URL: &str =
    "https://raw.githubusercontent.com/AOSC-Dev/anicca/main/pkgsupdate.json";
const PKGSUPDATE_JSON_FILENAME: &str = "anicca.json";

#[derive(Deserialize, Debug, Clone)]
pub struct Package {
    pub name: String,
    pub before: String,
    pub after: String,
    pub path: String,
    pub warnings: Vec<String>,
}

#[derive(Deserialize, Debug)]
pub struct Anicca(Vec<Package>);

impl Anicca {
    pub async fn fetch_json(data_dir: &Path) -> Result<()> {
        let client = Client::default();
        let content = client
            .get(PKGSUPDATE_JSON_URL)
            .send()
            .await?
            .bytes()
            .await?;

        fs::write(data_dir.join(PKGSUPDATE_JSON_FILENAME), content).await?;

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

    pub async fn get_updates(&self, packages: &[String]) -> Result<Vec<Package>> {
        let updates = self
            .0
            .iter()
            .filter(|pkg| packages.contains(&pkg.name))
            .cloned()
            .collect::<Vec<Package>>();

        Ok(updates)
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
