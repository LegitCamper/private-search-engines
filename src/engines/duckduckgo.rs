use async_trait::async_trait;
use percent_encoding::percent_decode;
use reqwest::Url;

use crate::engines::{
    EngineError, EngineInfo, SearchEngine, cache::ResultRow, new_rand_client, parse_search,
};

#[derive(Clone)]
pub struct DuckDuckGo;

impl EngineInfo for DuckDuckGo {
    fn name(&self) -> &'static str {
        "DuckDuckGo"
    }
}

#[async_trait]
impl SearchEngine for DuckDuckGo {
    async fn search_results(&self, query: &str) -> Result<Vec<ResultRow>, EngineError> {
        let resp = new_rand_client()
            .map_err(EngineError::ReqwestError)?
            .get(&format!("https://html.duckduckgo.com/html?q={}", query))
            .send()
            .await
            .map_err(EngineError::ReqwestError)?;

        parse_response(&resp.text().await.map_err(EngineError::ReqwestError)?)
    }
}

pub fn parse_response(html: &str) -> Result<Vec<ResultRow>, EngineError> {
    let results = parse_search(
        html,
        ".serp__results .result",
        ".result__a",
        ".result__a",
        ".result__snippet",
    )
    .into_iter()
    .filter(|r| !is_sponsored(&r.url))
    .map(|mut r| {
        r.url = extract_ddg_url(&r.url).unwrap();
        r
    })
    .collect();

    Ok(results)
}

fn extract_ddg_url(ddg_href: &str) -> Option<String> {
    // Decode the DDG redirect link
    let url = Url::parse("https://duckduckgo.com")
        .ok()?
        .join(ddg_href)
        .ok()?;
    for (k, v) in url.query_pairs() {
        if k == "uddg" {
            return Some(percent_decode(v.as_bytes()).decode_utf8().ok()?.to_string());
        }
    }
    Some(ddg_href.to_string()) // fallback to raw href
}

fn is_sponsored(href: &str) -> bool {
    href.contains("duckduckgo.com/l/?")
        || href.contains("duckduckgo.com/y.js")
        || href.contains("duckduckgo.com/?uddg=")
}

#[cfg(test)]
mod test {
    #[ignore]
    #[tokio::test]
    async fn test_duckduckgo_live() {
        use super::{DuckDuckGo, SearchEngine};
        let ddg = DuckDuckGo;
        let results = ddg.search_results("rust async").await.unwrap();
        assert!(!results.is_empty());

        println!("Results: ");
        for result in results {
            println!("{:?}", result);
        }
    }
}
