//! Bing's results page. Without JavaScript, Bing serves organic results only:
//! no sidebar and no AI answer.

use anyhow::Result;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use reqwest::Url;
use reqwest::blocking::Client;
use scraper::Html;

use super::{fetch, first, selector, text};
use crate::page::{Hit, Page};

pub fn search(http: &Client, query: &str) -> Result<Page> {
    let body = fetch(
        http.get("https://www.bing.com/search")
            .query(&[("q", query)]),
        "Bing",
    )?;
    Ok(parse(&body))
}

fn parse(html: &str) -> Page {
    let doc = Html::parse_document(html);
    let results = doc
        .select(&selector("#b_results > li.b_algo"))
        .filter_map(|result| {
            let link = first(result, "h2 a")?;
            let snippet = first(result, ".b_caption p").or_else(|| first(result, "p"));
            Some(Hit {
                title: text(link),
                url: decode_href(link.value().attr("href")?),
                snippet: snippet.map(text).unwrap_or_default(),
            })
        })
        .collect();
    Page {
        summary: None,
        sidebar: None,
        results,
    }
}

/// Unwraps Bing's `https://www.bing.com/ck/a?...&u=a1<base64url target>` redirect.
fn decode_href(href: &str) -> String {
    let target = Url::parse(href)
        .ok()
        .filter(|u| u.domain() == Some("www.bing.com") && u.path() == "/ck/a")
        .and_then(|u| {
            u.query_pairs()
                .find(|(k, _)| k == "u")
                .map(|(_, v)| v.into_owned())
        })
        .and_then(|u| {
            URL_SAFE_NO_PAD
                .decode(u.strip_prefix("a1")?.trim_end_matches('='))
                .ok()
        })
        .and_then(|bytes| String::from_utf8(bytes).ok());
    target.unwrap_or_else(|| href.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = include_str!("../../tests/fixtures/bing.html");

    #[test]
    fn parses_results() {
        let page = parse(PAGE);
        assert_eq!(page.results.len(), 10);
        let hit = &page.results[0];
        assert_eq!(
            hit.title,
            "Boiling Point of Water - What Temperature Does Water Boil?"
        );
        assert_eq!(
            hit.url,
            "https://sciencenotes.org/boiling-point-of-water-what-temperature-does-water-boil/"
        );
        assert!(
            hit.snippet
                .contains("Learn about the boiling point of water.")
        );
        assert_eq!(
            page.results[1].url,
            "https://en.wikipedia.org/wiki/Boiling_point"
        );
        assert!(page.results.iter().all(|h| !h.url.contains("bing.com/ck/")));
    }

    #[test]
    fn leaves_direct_links_alone() {
        assert_eq!(
            decode_href("https://example.com/a"),
            "https://example.com/a"
        );
    }
}
