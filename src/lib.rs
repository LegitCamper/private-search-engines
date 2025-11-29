#![allow(async_fn_in_trait)]

pub use crate::engines::Engines;
use crate::engines::{Brave, DuckDuckGo, Engine, EngineError};
use serde::Serialize;
use sqlx::SqlitePool;
use std::{cmp::Ordering, collections::BTreeMap, time::Duration};
use tokio::{sync::OnceCell, task::JoinSet, time::timeout};

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
    engines: Vec<Engines>,
    cached: bool,
}

impl PartialEq for SearchResult {
    fn eq(&self, other: &Self) -> bool {
        self.url == other.url
    }
}

impl Eq for SearchResult {}

impl PartialOrd for SearchResult {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.url.cmp(&other.url))
    }
}

impl Ord for SearchResult {
    fn cmp(&self, other: &Self) -> Ordering {
        self.url.cmp(&other.url)
    }
}

#[derive(Debug)]
pub enum FetchError {
    Sqlx(sqlx::Error),
    Engine(EngineError),
    AllEnginesFailed,
    Timeouts,
}

pub async fn search_all(
    query: String,
    engines: Vec<Engines>,
) -> Result<Vec<SearchResult>, FetchError> {
    let timeout_duration = Duration::from_secs(ENGINE_TIMEOUT);

    let mut set = JoinSet::new();

    engines.iter().for_each(|e| {
        match e {
            Engines::DuckDuckGo => {
                set.spawn(fetch_or_cache_query::<DuckDuckGo>(query.clone(), 0, 10))
            }
            Engines::Brave => set.spawn(fetch_or_cache_query::<Brave>(query.clone(), 0, 10)),
        };
    });

    let combined = timeout(timeout_duration, set.join_all()).await;

    let per_engine = match combined {
        Ok(res) => res,
        Err(_) => {
            return Err(FetchError::Timeouts);
        }
    };

    // Flatten good results, track if any succeeded
    let mut flat: Vec<SearchResult> = Vec::new();
    let mut any_success = false;

    for engine_result in per_engine {
        match engine_result {
            Ok(mut rows) => {
                any_success = true;
                flat.append(&mut rows); // flatten Vec<Vec<SearchResult>>
            }
            Err(e) => {
                eprintln!("Engine failed: {:?}", e);
            }
        }
    }

    if !any_success {
        return Err(FetchError::AllEnginesFailed);
    }

    let merged = merge_results(flat);

    Ok(merged)
}

fn merge_results(results: Vec<SearchResult>) -> Vec<SearchResult> {
    let mut map: BTreeMap<String, SearchResult> = BTreeMap::new();

    for row in results {
        let key = row.url.clone();

        map.entry(key)
            .and_modify(|existing| {
                // Merge engines
                existing.engines.extend(row.engines.clone());

                // Optional: smarter merging
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
pub async fn fetch_or_cache_query<ENGINE>(
    query: String,
    start: usize,
    count: usize,
) -> Result<Vec<SearchResult>, FetchError>
where
    ENGINE: Engine + Send + Sync,
{
    let pool = get_db().await;
    let mut search_results = Vec::new();

    let engine_enum = ENGINE::name();
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
            engines: vec![ENGINE::name()],
            cached: true,
        });
    }

    if cached_count < needed_end {
        let engine_results = ENGINE::search(&query).await.map_err(FetchError::Engine)?;

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
                engines: vec![ENGINE::name()],
                cached: false,
            });
        }
    }

    Ok(search_results)
}
