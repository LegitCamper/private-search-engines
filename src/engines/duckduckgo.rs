use percent_encoding::percent_decode;
use reqwest::Url;
use scraper::{ElementRef, Html, Selector};

use crate::engines::{Engine, EngineError, Engines, HtmlParser, cache::ResultRow, new_rand_client};

pub struct DuckDuckGo;

impl Engine for DuckDuckGo {
    fn name() -> Engines {
        Engines::DuckDuckGo
    }

    async fn search(query: &str) -> Result<Vec<ResultRow>, EngineError> {
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
    let parser = HtmlParser::new(
        ".serp__results .result",
        ".result__a",
        ".result__a",
        ".result__snippet",
    );

    let results = parser
        .parse(html)
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

fn is_sponsored(ddg_href: &str) -> bool {
    if ddg_href.contains("?ad_domain") || ddg_href.contains("?ad_provider") {
        return true;
    }
    false
}

#[cfg(test)]
mod test {
    #[ignore]
    #[tokio::test]
    async fn test_duckduckgo_live() {
        use super::{DuckDuckGo, Engine};
        let results = DuckDuckGo::search("rust async").await.unwrap();
        assert!(!results.is_empty());

        println!("Results: ");
        for result in results {
            println!("{:?}", result);
        }
    }
}
