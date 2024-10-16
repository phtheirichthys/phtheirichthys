// #![feature(btree_extract_if)]

pub(crate) mod algorithm;
pub(crate) mod land;
pub mod phtheirichthys;
pub(crate) mod polar;
pub mod position;
pub(crate) mod race;
mod router;
mod utils;
pub mod wind;
#[cfg(feature = "wasm")]
pub mod wasm_binding;

#[cfg(test)]
mod tests;

//use wind::providers::{config::{NoaaProviderConfig, ProviderConfig}, storage::StorageConfig, Providers, ProvidersSpec};

// static PHTHEIRICHTHYS: std::sync::RwLock<Option<Phtheirichthys>> = std::sync::RwLock::new(None);
