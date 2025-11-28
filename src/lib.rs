#![allow(async_fn_in_trait)]

use serde::Serialize;
pub use sqlx;

mod cache;
pub mod engines;

pub use crate::engines::Engines;
pub use cache::init as init_cache;

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    url: String,
    title: String,
    description: String,
    engine: Engines,
    cached: bool,
}
