use rend3::util::typedefs::SsoString;

pub struct AssetLoader {
    base: SsoString,
}
impl AssetLoader {
    pub fn new_local(_base_file: &str, _base_url: &str) -> Self {
        cfg_if::cfg_if!(
            if #[cfg(not(target_arch = "wasm32"))] {
                let base = _base_file;
            } else {
                let base = _base_url;
            }
        );

        Self {
            base: SsoString::from(base),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub async fn get_asset(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let full_path = self.base.clone() + path;
        Ok(std::fs::read(&*full_path).map_err(|e| anyhow::anyhow!("Failure to load {}: {}", &full_path, e))?)
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn get_asset(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let full_path = self.base.clone() + path;
        let response = reqwest::get(&*full_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failure to load {}: {}", &full_path, e))?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Non success status requesting {}: {}",
                path,
                response.status()
            ));
        }

        Ok(response.bytes().await?.to_vec())
    }
}
