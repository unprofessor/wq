//! A SearXNG instance's JSON API. The instance must enable `json` under
//! `search.formats` in its settings.yml; most public instances do not.

use anyhow::{Context, Result, bail};
use reqwest::StatusCode;
use reqwest::blocking::Client;
use reqwest::header::ACCEPT;
use serde::Deserialize;

use crate::config;
use crate::page::{Hit, Page, Sidebar};

pub fn search(http: &Client, query: &str, config: &config::Searxng) -> Result<Page> {
    let base = config
        .url
        .as_deref()
        .context("the searxng engine needs `url` under [searxng] in the config file")?;
    let resp = http
        .get(format!("{}/search", base.trim_end_matches('/')))
        .query(&[("q", query), ("format", "json")])
        .header(ACCEPT, "application/json")
        .send()
        .context("SearXNG request failed")?;
    match resp.status() {
        StatusCode::FORBIDDEN => {
            bail!(
                "{base} refused format=json (HTTP 403); its settings.yml must list `json` under `search.formats`"
            )
        }
        StatusCode::TOO_MANY_REQUESTS => bail!("{base} is rate-limiting requests (HTTP 429)"),
        status if !status.is_success() => bail!("{base} returned HTTP {status}"),
        _ => {}
    }
    parse(&resp.text().context("cannot read SearXNG response")?)
}

#[derive(Deserialize)]
struct Response {
    results: Vec<Result_>,
    #[serde(default)]
    answers: Vec<Answer>,
    #[serde(default)]
    infoboxes: Vec<Infobox>,
}

#[derive(Deserialize)]
struct Result_ {
    title: String,
    url: String,
    content: Option<String>,
}

/// Recent SearXNG versions emit objects; older ones emit bare strings.
#[derive(Deserialize)]
#[serde(untagged)]
enum Answer {
    Object { answer: String, url: Option<String> },
    Text(String),
}

#[derive(Deserialize)]
struct Infobox {
    infobox: String,
    id: Option<String>,
    content: Option<String>,
    #[serde(default)]
    urls: Vec<Link>,
    #[serde(default)]
    attributes: Vec<Attribute>,
}

#[derive(Deserialize)]
struct Link {
    url: String,
}

#[derive(Deserialize)]
struct Attribute {
    label: String,
    value: Option<serde_json::Value>,
}

fn parse(json: &str) -> Result<Page> {
    let resp: Response = serde_json::from_str(json).context("unexpected SearXNG response")?;
    let results = resp
        .results
        .into_iter()
        .map(|r| Hit {
            title: r.title,
            url: r.url,
            snippet: r.content.unwrap_or_default(),
        })
        .collect();
    let sidebar = match (
        resp.infoboxes.into_iter().next(),
        resp.answers.into_iter().next(),
    ) {
        (Some(infobox), _) => Some(Sidebar {
            title: infobox.infobox,
            text: infobox.content.unwrap_or_default(),
            facts: infobox
                .attributes
                .into_iter()
                .filter_map(|a| match a.value? {
                    serde_json::Value::String(value) => Some((a.label, value)),
                    _ => None,
                })
                .collect(),
            url: infobox.id.filter(|id| id.starts_with("http")).or(infobox
                .urls
                .into_iter()
                .next()
                .map(|l| l.url)),
        }),
        (None, Some(Answer::Object { answer, url })) => Some(Sidebar {
            title: "Answer".into(),
            text: answer.trim().into(),
            facts: Vec::new(),
            url,
        }),
        (None, Some(Answer::Text(answer))) => Some(Sidebar {
            title: "Answer".into(),
            text: answer.trim().into(),
            facts: Vec::new(),
            url: None,
        }),
        (None, None) => None,
    };
    Ok(Page {
        summary: None,
        sidebar,
        results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = include_str!("../../tests/fixtures/searxng.json");

    #[test]
    fn parses_results_and_infobox() {
        let page = parse(PAGE).unwrap();
        assert_eq!(page.results.len(), 10);
        assert_eq!(page.results[0].title, "Albert Einstein - Wikipedia");
        assert_eq!(
            page.results[0].url,
            "https://en.wikipedia.org/wiki/Albert_Einstein"
        );
        let sidebar = page.sidebar.unwrap();
        assert_eq!(sidebar.title, "Albert Einstein");
        assert!(
            sidebar
                .text
                .starts_with("Albert Einstein was a German-born theoretical physicist")
        );
        assert_eq!(
            sidebar.url.as_deref(),
            Some("https://en.wikipedia.org/wiki/Albert_Einstein")
        );
        assert_eq!(
            sidebar.facts[0],
            ("Date of birth".into(), "Friday, March 14, 1879".into())
        );
    }

    #[test]
    fn falls_back_to_answer() {
        let json = r#"{"results": [], "answers": [{"url": null, "engine": "answerer: avg", "answer": "[en] avg(1, 2, 3) = 2 "}], "infoboxes": []}"#;
        let sidebar = parse(json).unwrap().sidebar.unwrap();
        assert_eq!(sidebar.title, "Answer");
        assert_eq!(sidebar.text, "[en] avg(1, 2, 3) = 2");
    }

    #[test]
    fn accepts_legacy_string_answers() {
        let json = r#"{"results": [], "answers": ["42"]}"#;
        assert_eq!(parse(json).unwrap().sidebar.unwrap().text, "42");
    }
}
