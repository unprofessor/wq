//! DuckDuckGo's no-JavaScript results page.

use anyhow::{Context, Result, bail};
use reqwest::StatusCode;
use reqwest::Url;
use reqwest::blocking::Client;
use scraper::Html;

use super::{first, selector, text, text_without_links};
use crate::page::{Hit, Page, Sidebar};

pub fn search(http: &Client, query: &str) -> Result<Page> {
    let resp = http
        .get("https://html.duckduckgo.com/html/")
        .query(&[("q", query)])
        .send()
        .context("DuckDuckGo request failed")?;
    let status = resp.status();
    let body = resp.text().context("cannot read DuckDuckGo response")?;
    if is_bot_check(status, &body) {
        bail!(
            "DuckDuckGo answered with a bot check (CAPTCHA); wait a few minutes or use another engine, e.g. `-e bing`"
        );
    }
    if !status.is_success() {
        bail!("DuckDuckGo returned HTTP {status}");
    }
    Ok(parse(&body))
}

/// DuckDuckGo signals a suspected bot with HTTP 202 and a challenge form.
fn is_bot_check(status: StatusCode, body: &str) -> bool {
    status == StatusCode::ACCEPTED || body.contains("challenge-form")
}

fn parse(html: &str) -> Page {
    let doc = Html::parse_document(html);
    let results = doc
        .select(&selector("#links .web-result:not(.result--ad)"))
        .filter_map(|result| {
            let link = first(result, "a.result__a")?;
            Some(Hit {
                title: text(link),
                url: decode_href(link.value().attr("href")?),
                snippet: first(result, ".result__snippet")
                    .map(text)
                    .unwrap_or_default(),
            })
        })
        .collect();
    let sidebar = doc.select(&selector(".zci")).next().and_then(|zci| {
        let heading = first(zci, ".zci__heading")?;
        Some(Sidebar {
            title: text(heading),
            text: first(zci, ".zci__result")
                .map(text_without_links)
                .unwrap_or_default(),
            facts: Vec::new(),
            url: first(heading, "a")
                .and_then(|a| a.value().attr("href"))
                .map(String::from),
        })
    });
    Page {
        summary: None,
        sidebar,
        results,
    }
}

/// Unwraps DuckDuckGo's `//duckduckgo.com/l/?uddg=<target>` redirect.
fn decode_href(href: &str) -> String {
    let redirect = Url::parse(&format!("https:{href}"))
        .ok()
        .filter(|u| u.path() == "/l/");
    redirect
        .and_then(|u| {
            u.query_pairs()
                .find(|(k, _)| k == "uddg")
                .map(|(_, v)| v.into_owned())
        })
        .unwrap_or_else(|| href.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = include_str!("../../tests/fixtures/ddg.html");
    const BOT_CHECK: &str = include_str!("../../tests/fixtures/ddg_anomaly.html");

    #[test]
    fn parses_results() {
        let page = parse(PAGE);
        assert_eq!(page.results.len(), 10);
        let hit = &page.results[0];
        assert_eq!(
            hit.title,
            "How to Boil Water: 5 Quick & Easy Ways - wikiHow"
        );
        assert_eq!(hit.url, "https://www.wikihow.com/Boil-Water");
        assert!(!hit.snippet.is_empty());
        assert!(page.results.iter().all(|h| h.url.starts_with("https://")));
    }

    #[test]
    fn parses_instant_answer_as_sidebar() {
        let sidebar = parse(PAGE).sidebar.unwrap();
        assert_eq!(sidebar.title, "How to Boil Water");
        assert!(
            sidebar
                .text
                .starts_with("How to Boil Water is an American television program.")
        );
        assert!(!sidebar.text.contains("More at"));
        assert_eq!(
            sidebar.url.as_deref(),
            Some("https://en.wikipedia.org/wiki/How_to_Boil_Water")
        );
    }

    #[test]
    fn detects_bot_check() {
        assert!(is_bot_check(StatusCode::OK, BOT_CHECK));
        assert!(is_bot_check(StatusCode::ACCEPTED, ""));
        assert!(!is_bot_check(StatusCode::OK, PAGE));
    }

    #[test]
    fn leaves_direct_links_alone() {
        assert_eq!(
            decode_href("https://example.com/a"),
            "https://example.com/a"
        );
    }
}
