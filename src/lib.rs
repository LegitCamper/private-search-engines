#![allow(async_fn_in_trait)]

use serde::Serialize;
use sqlx::SqlitePool;
use std::{cmp::Ordering, collections::BTreeMap, pin::Pin, time::Duration};
use tokio::{sync::OnceCell, task::JoinSet, time::timeout};

use crate::engines::{Brave, DuckDuckGo, EngineError, EngineInfo, ImageEngine, SearchEngine};

mod cache;
pub mod engines;

const ENGINE_TIMEOUT: u64 = 3; // seconds

static SQLPOOL: OnceCell<SqlitePool> = OnceCell::const_new();

async fn get_db() -> &'static SqlitePool {
    SQLPOOL
        .get_or_init(|| async { cache::init().await.expect("Failed to init cache db") })
        .await
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    url: String,
    title: String,
    description: String,
    engines: Vec<String>,
    cached: bool,
}

impl PartialEq for SearchResult {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
    }
}

impl PartialOrd for SearchResult {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.url.cmp(&other.url))
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ImageResult {
    url: String,
    title: String,
    engines: Vec<String>,
    cached: bool,
}

impl PartialEq for ImageResult {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
    }
}

impl PartialOrd for ImageResult {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.url.cmp(&other.url))
    }
}

#[derive(Debug)]
pub enum FetchError {
    Sqlx(sqlx::Error),
    Engine(EngineError),
    AllEnginesFailed,
    Timeouts,
}

#[derive(Clone)]
pub enum SearchEngines {
    Brave,
    DuckDuckGo,
}

pub async fn search_engine_results(
    query: String,
    engines: Vec<SearchEngines>,
) -> Result<Vec<SearchResult>, FetchError> {
    let timeout_duration = Duration::from_secs(ENGINE_TIMEOUT);

    let mut set = JoinSet::new();

    for engine in engines {
        let query = query.clone();
        let engine = engine.clone();

        // Box the future to unify types
        let fut: Pin<Box<dyn Future<Output = Result<Vec<SearchResult>, FetchError>> + Send>> =
            match engine {
                SearchEngines::Brave => Box::pin(fetch_or_cache_result(Brave, query, 0, 10)),
                SearchEngines::DuckDuckGo => {
                    Box::pin(fetch_or_cache_result(DuckDuckGo, query, 0, 10))
                }
            };

        // Spawn the boxed future
        set.spawn(timeout(timeout_duration, fut));
    }

    let combined = timeout(timeout_duration, set.join_all()).await;

    let per_engine = match combined {
        Ok(res) => res,
        Err(_) => {
            return Err(FetchError::Timeouts);
        }
    };

    let mut flat: Vec<SearchResult> = Vec::new();
    let mut any_success = false;

    for engine_result in per_engine {
        match engine_result {
            Ok(rows) => {
                any_success = true;
                flat.append(&mut rows.unwrap());
            }
            Err(e) => {
                eprintln!("Engine failed: {:?}", e);
            }
        }
    }

    if !any_success {
        return Err(FetchError::AllEnginesFailed);
    }

    Ok(merge_results(flat))
}

fn merge_results(results: Vec<SearchResult>) -> Vec<SearchResult> {
    let mut map: BTreeMap<String, SearchResult> = BTreeMap::new();

    for row in results {
        let key = row.url.clone();

        map.entry(key)
            .and_modify(|existing| {
                existing.engines.extend(row.engines.clone());

                if existing.description.is_empty() {
                    existing.description = row.description.clone();
                }
                if existing.title.is_empty() {
                    existing.title = row.title.clone();
                }
            })
            .or_insert(row);
    }

    map.into_values().collect()
}

/// Checks the cache first; if miss, fetches from the engine and caches results.
pub async fn fetch_or_cache_result<E>(
    engine: E,
    query: String,
    start: usize,
    count: usize,
) -> Result<Vec<SearchResult>, FetchError>
where
    E: SearchEngine + EngineInfo + Send,
{
    let pool = get_db().await;
    let mut search_results = Vec::new();

    let engine_enum = engine.name();
    let engine_id = cache::get_engine_id(pool, engine_enum)
        .await
        .map_err(FetchError::Sqlx)?;

    // Fetch cached results
    let cached_rows = if let Some(query_row) = cache::get_query(pool, &query, engine_id)
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
        search_results.push(SearchResult {
            url: cr.url.clone(),
            title: cr.title.clone(),
            description: cr.description.clone(),
            engines: vec![engine.name().to_string()],
            cached: true,
        });
    }

    if cached_count < needed_end {
        let engine_results = engine
            .search_results(&query)
            .await
            .map_err(FetchError::Engine)?;

        let fetched_at = chrono::Utc::now().naive_utc();
        let _query_id = cache::upsert_query_with_results(
            pool,
            engine_enum,
            &query,
            engine_results.clone(),
            fetched_at,
        )
        .await
        .map_err(FetchError::Sqlx)?;

        for cr in &engine_results {
            search_results.push(SearchResult {
                url: cr.url.clone(),
                title: cr.title.clone(),
                description: cr.description.clone(),
                engines: vec![engine.name().to_string()],
                cached: false,
            });
        }
    }

    Ok(search_results)
}

#[derive(Clone)]
pub enum ImageEngines {
    Brave,
}

pub async fn search_engine_images(
    query: String,
    engines: Vec<ImageEngines>,
) -> Result<Vec<ImageResult>, FetchError> {
    let timeout_duration = Duration::from_secs(ENGINE_TIMEOUT);

    let mut set = JoinSet::new();

    for engine in engines {
        let query = query.clone();
        let engine = engine.clone();

        // Box the future to unify types
        let fut: Pin<Box<dyn Future<Output = Result<Vec<ImageResult>, FetchError>> + Send>> =
            match engine {
                ImageEngines::Brave => Box::pin(fetch_or_cache_image(Brave, query, 0, 10)),
            };

        // Spawn the boxed future
        set.spawn(timeout(timeout_duration, fut));
    }

    let combined = timeout(timeout_duration, set.join_all()).await;

    let per_engine = match combined {
        Ok(res) => res,
        Err(_) => {
            return Err(FetchError::Timeouts);
        }
    };

    let mut flat: Vec<ImageResult> = Vec::new();
    let mut any_success = false;

    for engine_result in per_engine {
        match engine_result {
            Ok(rows) => {
                any_success = true;
                flat.append(&mut rows.unwrap());
            }
            Err(e) => {
                eprintln!("Engine failed: {:?}", e);
            }
        }
    }

    if !any_success {
        return Err(FetchError::AllEnginesFailed);
    }

    Ok(merge_images(flat))
}

fn merge_images(images: Vec<ImageResult>) -> Vec<ImageResult> {
    let mut map: BTreeMap<String, ImageResult> = BTreeMap::new();

    for row in images {
        let key = row.url.clone();

        map.entry(key)
            .and_modify(|existing| {
                existing.engines.extend(row.engines.clone());

                if existing.title.is_empty() {
                    existing.title = row.title.clone();
                }
            })
            .or_insert(row);
    }

    map.into_values().collect()
}

/// Checks the cache first; if miss, fetches from the engine and caches images.
pub async fn fetch_or_cache_image<E>(
    engine: E,
    query: String,
    start: usize,
    count: usize,
) -> Result<Vec<ImageResult>, FetchError>
where
    E: ImageEngine + EngineInfo,
{
    let pool = get_db().await;
    let mut search_images = Vec::new();

    let engine_enum = engine.name();
    let engine_id = cache::get_engine_id(pool, engine_enum)
        .await
        .map_err(FetchError::Sqlx)?;

    // Fetch cached images
    let cached_rows = if let Some(query_row) = cache::get_query(pool, &query, engine_id)
        .await
        .map_err(FetchError::Sqlx)?
    {
        cache::get_images_for_query(pool, query_row.id)
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
        search_images.push(ImageResult {
            url: cr.url.clone(),
            title: cr.title.clone(),
            engines: vec![engine.name().to_string()],
            cached: true,
        });
    }

    if cached_count < needed_end {
        let engine_images = engine
            .search_images(&query)
            .await
            .map_err(FetchError::Engine)?;

        let fetched_at = chrono::Utc::now().naive_utc();
        let _query_id = cache::upsert_query_with_images(
            pool,
            engine_enum,
            &query,
            engine_images.clone(),
            fetched_at,
        )
        .await
        .map_err(FetchError::Sqlx)?;

        for cr in &engine_images {
            search_images.push(ImageResult {
                url: cr.url.clone(),
                title: cr.title.clone(),
                engines: vec![engine.name().to_string()],
                cached: false,
            });
        }
    }

    Ok(search_images)
}
