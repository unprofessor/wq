//! The Brave Search API. Results and the infobox come from the web search
//! endpoint; the AI summary comes from the Answers endpoint, which is billed
//! separately and needs the Answers plan.

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use reqwest::header::ACCEPT;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{fetch, html_to_text};
use crate::config;
use crate::engines::Request;
use crate::page::{Hit, Page, Sidebar};

const SEARCH_URL: &str = "https://api.search.brave.com/res/v1/web/search";
const ANSWERS_URL: &str = "https://api.search.brave.com/res/v1/chat/completions";
/// The web search endpoint returns at most this many results per request.
const MAX_COUNT: usize = 20;

pub fn search(http: &Client, req: &Request, config: &config::BraveApi) -> Result<Page> {
    let key = config
        .key
        .as_deref()
        .context("the brave-api engine needs `key` under [brave_api] in the config file")?;
    let answers_key = config.answers_key.as_deref().unwrap_or(key);
    std::thread::scope(|scope| {
        let summary = req
            .summary
            .then(|| scope.spawn(|| answer(http, answers_key, req.query)));
        let mut page = web(http, key, req.query, req.count)?;
        if let Some(summary) = summary {
            match summary.join().expect("answers thread does not panic") {
                Ok(text) => page.summary = Some(text),
                Err(e) => eprintln!("wq: no summary: {e:#}"),
            }
        }
        Ok(page)
    })
}

fn web(http: &Client, key: &str, query: &str, count: usize) -> Result<Page> {
    let count = count.clamp(1, MAX_COUNT).to_string();
    let req = http
        .get(SEARCH_URL)
        .query(&[("q", query), ("count", &count)])
        .header(ACCEPT, "application/json")
        .header("X-Subscription-Token", key);
    parse(&fetch(req, "Brave Search API")?)
}

fn answer(http: &Client, key: &str, query: &str) -> Result<String> {
    let body = json!({
        "model": "brave",
        "messages": [{"role": "user", "content": query}],
        "stream": false,
    });
    let req = http
        .post(ANSWERS_URL)
        .header(ACCEPT, "application/json")
        .header("X-Subscription-Token", key)
        .json(&body);
    let completion: Completion = serde_json::from_str(&fetch(req, "Brave Answers API")?)
        .context("unexpected Brave Answers API response")?;
    completion
        .choices
        .into_iter()
        .next()
        .map(|c| c.message.content.trim().to_string())
        .filter(|text| !text.is_empty())
        .context("Brave Answers API returned an empty answer")
}

#[derive(Deserialize)]
struct Completion {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
}

#[derive(Deserialize)]
struct Message {
    content: String,
}

#[derive(Deserialize)]
struct Response {
    web: Option<Web>,
    infobox: Option<Infobox>,
}

#[derive(Deserialize)]
struct Web {
    results: Vec<WebResult>,
}

#[derive(Deserialize)]
struct WebResult {
    title: String,
    url: String,
    #[serde(default)]
    description: String,
}

#[derive(Deserialize)]
struct Infobox {
    results: OneOrMany<Entity>,
}

/// Brave documents `infobox.results` as a list, but its own client types it as a
/// single object; accept both.
#[derive(Deserialize)]
#[serde(untagged)]
enum OneOrMany<T> {
    Many(Vec<T>),
    One(T),
}

#[derive(Deserialize)]
struct Entity {
    title: String,
    url: Option<String>,
    description: Option<String>,
    long_desc: Option<String>,
    #[serde(default)]
    attributes: Vec<Vec<Value>>,
}

fn parse(json: &str) -> Result<Page> {
    let resp: Response =
        serde_json::from_str(json).context("unexpected Brave Search API response")?;
    let results = resp
        .web
        .map(|web| web.results)
        .unwrap_or_default()
        .into_iter()
        .map(|r| Hit {
            title: html_to_text(&r.title),
            url: r.url,
            snippet: html_to_text(&r.description),
        })
        .collect();
    let entity = resp.infobox.and_then(|infobox| match infobox.results {
        OneOrMany::Many(entities) => entities.into_iter().next(),
        OneOrMany::One(entity) => Some(entity),
    });
    let sidebar = entity.map(|e| Sidebar {
        title: html_to_text(&e.title),
        text: e
            .long_desc
            .or(e.description)
            .map(|d| html_to_text(&d))
            .unwrap_or_default(),
        facts: e
            .attributes
            .into_iter()
            .filter_map(|pair| match pair.as_slice() {
                [Value::String(label), Value::String(value)] => {
                    Some((label.clone(), value.clone()))
                }
                _ => None,
            })
            .collect(),
        url: e.url,
    });
    Ok(Page {
        summary: None,
        sidebar,
        results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthesized from the documented WebSearchApiResponse schema, not captured live.
    const PAGE: &str = include_str!("../../tests/fixtures/brave_api.json");

    #[test]
    fn parses_results_and_infobox() {
        let page = parse(PAGE).unwrap();
        assert_eq!(page.results.len(), 2);
        assert_eq!(page.results[0].title, "Albert Einstein - Wikipedia");
        assert_eq!(
            page.results[0].url,
            "https://en.wikipedia.org/wiki/Albert_Einstein"
        );
        assert!(
            page.results[0]
                .snippet
                .starts_with("Albert Einstein was a German-born")
        );
        assert_eq!(page.results[1].title, "Einstein's Theory of Relativity");
        let sidebar = page.sidebar.unwrap();
        assert_eq!(sidebar.title, "Albert Einstein");
        assert!(
            sidebar
                .text
                .starts_with("Albert Einstein (14 March 1879 – 18 April 1955)")
        );
        assert_eq!(
            sidebar.facts,
            vec![
                ("Born".into(), "14 March 1879".into()),
                ("Died".into(), "18 April 1955".into())
            ]
        );
        assert_eq!(
            sidebar.url.as_deref(),
            Some("https://en.wikipedia.org/wiki/Albert_Einstein")
        );
    }

    #[test]
    fn accepts_single_infobox_object() {
        let json = r#"{"infobox": {"type": "graph", "results": {"title": "Water", "description": "A liquid"}}}"#;
        let page = parse(json).unwrap();
        assert!(page.results.is_empty());
        assert_eq!(page.sidebar.unwrap().text, "A liquid");
    }

    #[test]
    fn parses_answer() {
        let json = r#"{"id": "chatcmpl-1", "object": "chat.completion", "choices": [{"index": 0, "finish_reason": "stop", "message": {"role": "assistant", "content": " Water boils at 100 °C at sea level.\n"}}]}"#;
        let completion: Completion = serde_json::from_str(json).unwrap();
        assert_eq!(
            completion.choices[0].message.content.trim(),
            "Water boils at 100 °C at sea level."
        );
    }
}
