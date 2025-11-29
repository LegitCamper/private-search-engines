use crate::{
    cache::ResultRow,
    engines::{Engine, EngineError, Engines, HtmlParser, new_rand_client},
};

pub struct Brave;

impl Engine for Brave {
    fn name() -> Engines {
        Engines::Brave
    }

    async fn search(query: &str) -> Result<Vec<ResultRow>, EngineError> {
        let resp = new_rand_client()
            .map_err(EngineError::ReqwestError)?
            .get(&format!("https://search.brave.com/search?q={}", query))
            .send()
            .await
            .map_err(EngineError::ReqwestError)?;

        parse_response(&resp.text().await.map_err(EngineError::ReqwestError)?)
    }
}

pub fn parse_response(html: &str) -> Result<Vec<ResultRow>, EngineError> {
    let parser = HtmlParser::new(
        "#results > .snippet[data-pos]:not(.standalone)",
        ".title",
        "a",
        ".generic-snippet, .video-snippet > .snippet-description",
    );

    let results = parser.parse(html);

    Ok(results)
}

#[cfg(test)]
mod test {
    #[ignore]
    #[tokio::test]
    async fn test_brave_live() {
        use super::{Brave, Engine};
        let results = Brave::search("rust async").await.unwrap();
        assert!(!results.is_empty());

        println!("Results: ");
        for result in results {
            println!("{:?}", result);
        }
    }
}
