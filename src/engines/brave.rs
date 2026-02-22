use crate::{
    cache::{ImagesRow, ResultRow},
    engines::{
        EngineError, EngineInfo, ImageEngine, SearchEngine, new_rand_client, parse_images,
        parse_search,
    },
};
use async_trait::async_trait;

#[derive(Clone)]
pub struct Brave;

impl EngineInfo for Brave {
    fn name(&self) -> &'static str {
        "Brave"
    }
}

#[async_trait]
impl SearchEngine for Brave {
    async fn search_results(&self, query: &str) -> Result<Vec<ResultRow>, EngineError> {
        let resp = new_rand_client()
            .map_err(EngineError::ReqwestError)?
            .get(format!("https://search.brave.com/search?q={}", query))
            .send()
            .await
            .map_err(EngineError::ReqwestError)?;

        parse_search_response(&resp.text().await.map_err(EngineError::ReqwestError)?)
    }
}

pub fn parse_search_response(html: &str) -> Result<Vec<ResultRow>, EngineError> {
    Ok(parse_search(
        html,
        "#results > .snippet[data-pos]:not(.standalone)",
        ".title",
        "a",
        ".generic-snippet, .video-snippet > .snippet-description",
    ))
}

#[async_trait]
impl ImageEngine for Brave {
    async fn search_images(&self, query: &str) -> Result<Vec<ImagesRow>, EngineError> {
        let resp = new_rand_client()
            .map_err(EngineError::ReqwestError)?
            .get(format!("https://search.brave.com/images?q={}", query))
            .send()
            .await
            .map_err(EngineError::ReqwestError)?;

        parse_image_response(&resp.text().await.map_err(EngineError::ReqwestError)?)
    }
}

pub fn parse_image_response(html: &str) -> Result<Vec<ImagesRow>, EngineError> {
    Ok(parse_images(
        html,
        ".image-result",
        ".image-metadata-title",
        "img",
    ))
}

#[cfg(test)]
mod test {
    #[ignore]
    #[tokio::test]
    async fn test_brave_search_live() {
        use super::{Brave, SearchEngine};
        let brave = Brave;
        let results = brave.search_results("rust async").await.unwrap();
        assert!(!results.is_empty());

        println!("Results: ");
        for result in results {
            println!("{:?}", result);
        }
    }

    #[ignore]
    #[tokio::test]
    async fn test_brave_images_live() {
        use super::{Brave, ImageEngine};
        let brave = Brave;
        let images = brave.search_images("rust async").await.unwrap();
        assert!(!images.is_empty());

        println!("Images: ");
        for image in images {
            println!("{:?}", image);
        }
    }
}
