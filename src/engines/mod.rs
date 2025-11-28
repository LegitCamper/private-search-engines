use serde::Serialize;
use sqlx::SqlitePool;
use strum::EnumIter;

use crate::{
    SearchResult,
    cache::{self, ResultRow},
};

mod brave;
mod duckduckgo;

pub use brave::Brave;
pub use duckduckgo::DuckDuckGo;

#[derive(Debug, Copy, Clone, Serialize, EnumIter, sqlx::Type)]
pub enum Engines {
    DuckDuckGo,
    Brave,
}

#[derive(Debug)]
pub enum EngineError {
    ReqwestError(reqwest::Error),
    ParseError(String),
    Timeout, // engine timeout
}

pub trait Engine {
    /// get name of engine
    fn name() -> Engines;

    /// search query with engine (must check `is_available()` first!)
    async fn search(query: &str) -> Result<Vec<ResultRow>, EngineError>;
}

#[derive(Debug)]
pub enum FetchError {
    Sqlx(sqlx::Error),
    Engine(EngineError),
}

/// Checks the cache first; if miss, fetches from the engine and stores results.
pub async fn fetch_or_cache_query<ENGINE>(
    pool: &SqlitePool,
    query: &str,
    start: usize,
    count: usize,
) -> Result<Vec<SearchResult>, FetchError>
where
    ENGINE: Engine + Send + Sync,
{
    let mut website_results = Vec::new();

    let engine_enum = ENGINE::name();
    let engine_id = cache::get_engine_id(pool, engine_enum)
        .await
        .map_err(FetchError::Sqlx)?;

    // Fetch cached results
    let cached_rows = if let Some(query_row) = cache::get_query(pool, query, engine_id)
        .await
        .map_err(FetchError::Sqlx)?
    {
        cache::get_results_for_query(pool, query_row.id)
            .await
            .map_err(FetchError::Sqlx)?
    } else {
        Vec::new()
    };

    let cached_count = cached_rows.len();
    let needed_end = start + count;

    let start = start.min(cached_count);
    let end = cached_count.min(needed_end);

    for cr in &cached_rows[start..end] {
        website_results.push(SearchResult {
            url: cr.url.clone(),
            title: cr.title.clone(),
            description: cr.description.clone(),
            engine: ENGINE::name(),
            cached: true,
        });
    }

    if cached_count < needed_end {
        let engine_results = ENGINE::search(query).await.map_err(FetchError::Engine)?;

        let fetched_at = chrono::Utc::now().naive_utc();
        let _query_id = cache::upsert_query_with_results(
            pool,
            engine_enum,
            query,
            engine_results.clone(),
            fetched_at,
        )
        .await
        .map_err(FetchError::Sqlx)?;

        for cr in &engine_results {
            website_results.push(SearchResult {
                url: cr.url.clone(),
                title: cr.title.clone(),
                description: cr.description.clone(),
                engine: ENGINE::name(),
                cached: false,
            });
        }
    }

    Ok(website_results)
}
