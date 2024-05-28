use anyhow::bail;
use web_sys::FileSystemGetFileOptions;

use super::Storage;

pub(crate) struct LocalStorage {
    pub(crate) prefix: String
}

impl Storage for LocalStorage {

    async fn save(&self, name: String) -> anyhow::Result<()> {

        let navigator = web_sys::window().unwrap().navigator();

        let handle = match wasm_bindgen_futures::JsFuture::from(navigator.storage().get_directory()).await {
            Ok(handle) => {
                web_sys::FileSystemDirectoryHandle::from(handle)
            }
            Err(e) => {
                bail!("Fail getting root directory handler")
            }
        };

        let handle = match wasm_bindgen_futures::JsFuture::from(handle.get_file_handle_with_options(&name, FileSystemGetFileOptions::new().create(true))).await {
            Ok(handle) => {
                web_sys::FileSystemFileHandle::from(handle)
            }
            Err(e) => {
                bail!("Fail getting file handler")
            }
        };



        Ok(())
    }

    async fn remove(&self, name: String) -> anyhow::Result<()> {
        todo!()
    }

    async fn exists(&self, name: String) -> anyhow::Result<bool> {
        
        Ok(false)
    }
}