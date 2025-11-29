use rand::seq::IndexedRandom;
use reqwest::Client;
use scraper::{Html, Selector};
use serde::Serialize;
use strum::EnumIter;

use crate::cache::{self, ResultRow};

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
    fn name() -> Engines;
    async fn search(query: &str) -> Result<Vec<ResultRow>, EngineError>;
}

fn new_rand_client() -> Result<Client, reqwest::Error> {
    static USER_AGENTS: &[&str] = &[
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        "Mozilla/5.0 (X11; Linux x86_64; rv:118.0) Gecko/20100101 Firefox/118.0",
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 13_4) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/118.0.5993.72 Safari/537.36",
    ];

    let user_agent = USER_AGENTS.choose(&mut rand::rng()).unwrap();

    Client::builder().user_agent(*user_agent).build()
}

pub struct HtmlParser {
    results_selector: Selector,
    title_selector: Selector,
    href_selector: Selector,
    description_selector: Selector,
}

impl HtmlParser {
    pub fn new(
        results_selector: &'static str,
        title_selector: &'static str,
        href_selector: &'static str,
        description_selector: &'static str,
    ) -> Self {
        const PARSE_ERROR: &'static str = "Couldnt parse selector string";
        Self {
            results_selector: Selector::parse(results_selector).expect(PARSE_ERROR),
            title_selector: Selector::parse(title_selector).expect(PARSE_ERROR),
            href_selector: Selector::parse(href_selector).expect(PARSE_ERROR),
            description_selector: Selector::parse(description_selector).expect(PARSE_ERROR),
        }
    }

    pub fn parse(&self, html: &str) -> Vec<ResultRow> {
        let html = Html::parse_document(html);

        let mut results = Vec::new();

        for result in html.select(&self.results_selector) {
            results.push(ResultRow {
                url: result
                    .select(&self.href_selector)
                    .next()
                    .and_then(|u| u.value().attr("href"))
                    .unwrap_or_default()
                    .to_string(),

                title: result
                    .select(&self.title_selector)
                    .next()
                    .map(|t| t.text().collect::<String>())
                    .unwrap_or_default(),

                description: result
                    .select(&self.description_selector)
                    .next()
                    .map(|d| d.text().collect::<String>())
                    .unwrap_or_default(),
            })
        }

        results
    }
}
