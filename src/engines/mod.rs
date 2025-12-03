use async_trait::async_trait;
use rand::seq::IndexedRandom;
use reqwest::Client;
use scraper::{Html, Selector};

use crate::cache::{self, ImagesRow, ResultRow};

mod brave;
mod duckduckgo;

pub use brave::Brave;
pub use duckduckgo::DuckDuckGo;

#[derive(Debug)]
pub enum EngineError {
    ReqwestError(reqwest::Error),
    ParseError(String),
    Timeout, // engine timeout
}

#[async_trait]
pub trait EngineInfo: Clone + Send {
    fn name(&self) -> &'static str;
}

#[async_trait]
pub trait SearchEngine: EngineInfo + Clone + Send {
    async fn search_results(&self, query: &str) -> Result<Vec<ResultRow>, EngineError>;
}

#[async_trait]
pub trait ImageEngine: EngineInfo + Clone + Send {
    async fn search_images(&self, query: &str) -> Result<Vec<ImagesRow>, EngineError>;
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

const PARSE_ERROR: &'static str = "Couldnt parse selector string";

pub fn parse_search(
    html: &str,
    results_selector: &'static str,
    title_selector: &'static str,
    href_selector: &'static str,
    description_selector: &'static str,
) -> Vec<ResultRow> {
    let html = Html::parse_document(html);

    let results_selector = Selector::parse(results_selector).expect(PARSE_ERROR);
    let title_selector = Selector::parse(title_selector).expect(PARSE_ERROR);
    let href_selector = Selector::parse(href_selector).expect(PARSE_ERROR);
    let description_selector = Selector::parse(description_selector).expect(PARSE_ERROR);

    let mut results = Vec::new();

    for result in html.select(&results_selector) {
        results.push(ResultRow {
            url: result
                .select(&href_selector)
                .next()
                .and_then(|u| u.value().attr("href"))
                .unwrap_or_default()
                .to_string(),

            title: result
                .select(&title_selector)
                .next()
                .map(|t| t.text().collect::<String>())
                .unwrap_or_default(),

            description: result
                .select(&description_selector)
                .next()
                .map(|d| d.text().collect::<String>())
                .unwrap_or_default(),
        })
    }

    results
}

pub fn parse_images(
    html: &str,
    images_selector: &'static str,
    title_selector: &'static str,
    img_selector: &'static str,
) -> Vec<ImagesRow> {
    let html = Html::parse_document(html);

    let images_selector = Selector::parse(images_selector).expect(PARSE_ERROR);
    let title_selector = Selector::parse(title_selector).expect(PARSE_ERROR);
    let img_selector = Selector::parse(img_selector).expect(PARSE_ERROR);

    let mut images = Vec::new();

    for result in html.select(&images_selector) {
        images.push(ImagesRow {
            url: result
                .select(&img_selector)
                .next()
                .and_then(|u| u.value().attr("src"))
                .unwrap_or_default()
                .to_string(),

            title: result
                .select(&title_selector)
                .next()
                .map(|t| t.text().collect::<String>())
                .unwrap_or_default(),
        })
    }

    images
}
