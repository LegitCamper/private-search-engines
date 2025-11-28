use reqwest::Client;
use scraper::{Html, Selector};

use crate::{
    cache::ResultRow,
    engines::{Engine, EngineError, Engines},
};

pub struct Brave;

impl Engine for Brave {
    fn name() -> Engines {
        Engines::Brave
    }

    async fn search(query: &str) -> Result<Vec<ResultRow>, EngineError> {
        let resp = Client::new()
            .get(&format!("https://search.brave.com/search?q={}", query))
            .send()
            .await
            .map_err(EngineError::ReqwestError)?;

        parse_response(&resp.text().await.map_err(EngineError::ReqwestError)?)
    }
}

pub fn parse_response(body: &str) -> Result<Vec<ResultRow>, EngineError> {
    let result = Selector::parse("#results > .snippet[data-pos]:not(.standalone)").unwrap();
    let title = Selector::parse(".title").unwrap();
    let url = Selector::parse("a").unwrap();
    let description =
        Selector::parse(".generic-snippet, .video-snippet > .snippet-description").unwrap();

    let html = Html::parse_document(body);

    let mut results = Vec::new();

    for result in html.select(&result) {
        results.push(ResultRow {
            url: result
                .select(&url)
                .next()
                .map(|t| t.text().collect::<String>())
                .unwrap_or_default(),

            title: result
                .select(&title)
                .next()
                .map(|t| t.text().collect::<String>())
                .unwrap_or_default(),

            description: result
                .select(&description)
                .next()
                .map(|t| t.text().collect::<String>())
                .unwrap_or_default(),
        })
    }

    Ok(results)
}
