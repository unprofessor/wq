mod bing;
mod brave;
mod brave_api;
mod ddg;
mod searxng;

use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use scraper::{ElementRef, Html, Node, Selector};
use serde::Deserialize;

use crate::config::Config;
use crate::page::Page;

/// Identifies wq honestly; all three scraped engines serve full result pages to it.
const USER_AGENT: &str = concat!("wq/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Copy, Debug, ValueEnum, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Engine {
    /// DuckDuckGo, scraped from its no-JavaScript page
    #[value(alias = "duckduckgo")]
    #[serde(alias = "duckduckgo")]
    Ddg,
    /// Bing, scraped
    Bing,
    /// Brave Search, scraped
    Brave,
    /// Brave Search API (needs [brave_api] key)
    BraveApi,
    /// SearXNG instance JSON API (needs [searxng] url)
    Searxng,
}

pub struct Request<'a> {
    pub query: &'a str,
    pub count: usize,
    /// Whether to spend an extra request on an AI summary, for engines that offer one.
    pub summary: bool,
}

impl Engine {
    pub fn search(self, req: &Request, config: &Config) -> Result<Page> {
        let http = client()?;
        match self {
            Engine::Ddg => ddg::search(&http, req.query),
            Engine::Bing => bing::search(&http, req.query),
            Engine::Brave => brave::search(&http, req.query),
            Engine::BraveApi => brave_api::search(&http, req, &config.brave_api),
            Engine::Searxng => searxng::search(&http, req.query, &config.searxng),
        }
    }
}

/// Headers of a Firefox page navigation. Load-bearing: DuckDuckGo answers
/// requests without them with a CAPTCHA. Do not trim this list without
/// checking DuckDuckGo live.
const BROWSER_HEADERS: &[(&str, &str)] = &[
    (
        "accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    ),
    ("accept-language", "en-US,en;q=0.9"),
    ("dnt", "1"),
    ("sec-gpc", "1"),
    ("upgrade-insecure-requests", "1"),
    ("sec-fetch-dest", "document"),
    ("sec-fetch-mode", "navigate"),
    ("sec-fetch-site", "none"),
    ("sec-fetch-user", "?1"),
    ("priority", "u=0, i"),
    ("pragma", "no-cache"),
    ("cache-control", "no-cache"),
    ("te", "trailers"),
];

fn client() -> Result<Client> {
    let mut headers = HeaderMap::new();
    for (name, value) in BROWSER_HEADERS {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }
    Ok(Client::builder()
        .user_agent(USER_AGENT)
        .default_headers(headers)
        .timeout(Duration::from_secs(30))
        .build()?)
}

/// Sends `req` and returns the body, failing on a non-2xx status. JSON error
/// bodies are included in the message; HTML ones are too long to be useful.
fn fetch(req: RequestBuilder, name: &str) -> Result<String> {
    let resp = req
        .send()
        .with_context(|| format!("{name} request failed"))?;
    let status = resp.status();
    let is_json = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("json"));
    let body = resp
        .text()
        .with_context(|| format!("cannot read {name} response"))?;
    if !status.is_success() {
        if is_json {
            bail!("{name} returned HTTP {status}: {}", body.trim());
        }
        bail!("{name} returned HTTP {status}");
    }
    Ok(body)
}

fn selector(css: &str) -> Selector {
    Selector::parse(css).expect("selectors are static and valid")
}

fn first<'a>(el: ElementRef<'a>, css: &str) -> Option<ElementRef<'a>> {
    el.select(&selector(css)).next()
}

/// Visible text of `el` with whitespace collapsed; `<br>` becomes "; ".
fn text(el: ElementRef) -> String {
    let mut out = String::new();
    push_text(&mut out, el);
    squash(&out)
}

/// Like [`text`], but skips links that are direct children of `el`, such as a
/// trailing "More at Wikipedia".
fn text_without_links(el: ElementRef) -> String {
    let mut out = String::new();
    for child in el.children() {
        match child.value() {
            Node::Text(t) => out.push_str(t),
            Node::Element(e) if e.name() != "a" => {
                push_text(&mut out, ElementRef::wrap(child).expect("element node"))
            }
            _ => {}
        }
    }
    squash(&out)
}

fn push_text(out: &mut String, el: ElementRef) {
    for node in el.descendants() {
        match node.value() {
            Node::Text(t) => out.push_str(t),
            Node::Element(e) if e.name() == "br" => {
                out.truncate(out.trim_end().len());
                out.push_str("; ");
            }
            _ => {}
        }
    }
}

/// Plain text of an HTML fragment such as `<strong>Water</strong> &amp; ice`.
fn html_to_text(fragment: &str) -> String {
    text(Html::parse_fragment(fragment).root_element())
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_collapses_whitespace_and_breaks() {
        let doc = Html::parse_fragment("<p> 14 March 1879 <br>\n Ulm,  <b>Germany</b></p>");
        assert_eq!(
            text(first(doc.root_element(), "p").unwrap()),
            "14 March 1879; Ulm, Germany"
        );
    }

    #[test]
    fn text_without_links_drops_child_links() {
        let doc = Html::parse_fragment(
            r#"<div>Water is <b>wet</b>. <a href="x">More at Wikipedia</a></div>"#,
        );
        assert_eq!(
            text_without_links(first(doc.root_element(), "div").unwrap()),
            "Water is wet."
        );
    }

    #[test]
    fn html_to_text_decodes_entities() {
        assert_eq!(
            html_to_text("<strong>Einstein</strong>&#x27;s &amp; co"),
            "Einstein's & co"
        );
    }
}
