use std::borrow::Cow;

use rend3::util::typedefs::SsoString;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AssetError {
    #[error("Could not read {path} from disk")]
    #[cfg(not(target_arch = "wasm32"))]
    FileError {
        path: SsoString,
        #[source]
        error: std::io::Error,
    },
    #[error("Could not read {path} from the network")]
    #[cfg(target_arch = "wasm32")]
    NetworkError {
        path: SsoString,
        #[source]
        error: reqwest::Error,
    },
    #[error("Reading {path} from the network returned non-success status code {status}")]
    #[cfg(target_arch = "wasm32")]
    NetworkStatusError {
        path: SsoString,
        status: reqwest::StatusCode,
    },
}

pub enum AssetPath<'a> {
    Internal(&'a str),
    External(&'a str),
}
impl<'a> AssetPath<'a> {
    fn get_path(self, base: &str) -> Cow<'a, str> {
        match self {
            Self::Internal(p) => Cow::Owned(base.to_owned() + p),
            Self::External(p) => Cow::Borrowed(p),
        }
    }
}

pub struct AssetLoader {
    base: SsoString,
}
impl AssetLoader {
    pub fn new_local(_base_file: &str, _base_asset: &str, _base_url: &str) -> Self {
        cfg_if::cfg_if!(
            if #[cfg(target_arch = "wasm32")] {
                let base = _base_url;
            } else if #[cfg(target_os = "android")] {
                let base = _base_asset;
            } else {
                let base = _base_file;
            }
        );

        Self {
            base: SsoString::from(base),
        }
    }

    pub fn get_asset_path<'a>(&self, path: AssetPath<'a>) -> Cow<'a, str> {
        path.get_path(&self.base)
    }

    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    pub async fn get_asset(&self, path: AssetPath<'_>) -> Result<Vec<u8>, AssetError> {
        let full_path = path.get_path(&self.base);
        Ok(std::fs::read(&*full_path).map_err(|error| AssetError::FileError {
            path: SsoString::from(full_path),
            error,
        })?)
    }

    #[cfg(target_os = "android")]
    pub async fn get_asset(&self, path: AssetPath<'_>) -> Result<Vec<u8>, AssetError> {
        use std::ffi::CString;

        let manager = ndk_glue::native_activity().asset_manager();

        let full_path = path.get_path(&self.base);
        manager
            .open(&CString::new(&*full_path).unwrap())
            .ok_or_else(|| AssetError::FileError {
                path: SsoString::from(&*full_path),
                error: std::io::Error::new(std::io::ErrorKind::NotFound, "could not find file in asset manager"),
            })
            .and_then(|mut file| {
                file.get_buffer()
                    .map(|b| b.to_vec())
                    .map_err(|error| AssetError::FileError {
                        path: SsoString::from(full_path),
                        error,
                    })
            })
    }

    #[cfg(target_arch = "wasm32")]
    pub async fn get_asset(&self, path: AssetPath<'_>) -> Result<Vec<u8>, AssetError> {
        let full_path = path.get_path(&self.base);
        let response = reqwest::get(&*full_path)
            .await
            .map_err(|error| AssetError::NetworkError {
                path: SsoString::from(&*full_path),
                error,
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(AssetError::NetworkStatusError {
                path: SsoString::from(&*full_path),
                status,
            });
        }

        Ok(response
            .bytes()
            .await
            .map_err(|error| AssetError::NetworkError {
                path: SsoString::from(&*full_path),
                error,
            })?
            .to_vec())
    }
}
