//! Brave Search's results page. The AI answer loads through a separate
//! undocumented request, so this engine returns results and the infobox only.

use anyhow::Result;
use reqwest::blocking::Client;
use reqwest::header::COOKIE;
use scraper::{ElementRef, Html};

use super::{fetch, first, selector, text, text_without_links};
use crate::page::{Hit, Page, Sidebar};

pub fn search(http: &Client, query: &str) -> Result<Page> {
    let req = http
        .get("https://search.brave.com/search")
        .query(&[("q", query), ("source", "web")])
        .header(COOKIE, "useLocation=0; summarizer=0");
    let body = fetch(req, "Brave")?;
    Ok(parse(&body))
}

fn parse(html: &str) -> Page {
    let doc = Html::parse_document(html);
    let results = doc
        .select(&selector(r#"div.snippet[data-type="web"]"#))
        .filter_map(|result| {
            let link = first(result, "a[href]")?;
            let snippet =
                first(result, ".generic-snippet .content").or_else(|| first(result, ".content"));
            Some(Hit {
                title: text(first(link, ".title")?),
                url: link.value().attr("href")?.to_string(),
                snippet: snippet.map(text).unwrap_or_default(),
            })
        })
        .collect();
    let sidebar = doc
        .select(&selector("#infobox-snippet"))
        .next()
        .and_then(infobox);
    Page {
        summary: None,
        sidebar,
        results,
    }
}

fn infobox(root: ElementRef) -> Option<Sidebar> {
    let title = first(root, ".infobox-header-title")?;
    // The description is an unclassed <section> just before the factsheet.
    let description = first(root, ".infobox-factsheet").and_then(|factsheet| {
        factsheet
            .prev_siblings()
            .filter_map(ElementRef::wrap)
            .find(|e| e.value().name() == "section")
    });
    let subtitle = first(root, ".infobox-header > .t-tertiary");
    // The factsheet repeats for narrow and wide layouts; read the first copy.
    let facts = first(root, ".infobox-attributes")
        .map(|attrs| {
            attrs
                .select(&selector(".infobox-attr"))
                .filter_map(|attr| {
                    Some((
                        text(first(attr, ".infobox-attr-name")?),
                        text(first(attr, ".infobox-attr-value")?),
                    ))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(Sidebar {
        title: text(title),
        text: description
            .or(subtitle)
            .map(text_without_links)
            .unwrap_or_default(),
        facts,
        url: first(title, "a")
            .and_then(|a| a.value().attr("href"))
            .map(String::from),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = include_str!("../../tests/fixtures/brave.html");

    #[test]
    fn parses_results() {
        let page = parse(PAGE);
        assert_eq!(page.results.len(), 20);
        assert_eq!(
            page.results[0].url,
            "https://en.wikipedia.org/wiki/Albert_Einstein"
        );
        assert!(page.results[0].title.contains("Albert Einstein"));
        for hit in &page.results {
            assert!(hit.url.starts_with("https://"), "{hit:?}");
            assert!(!hit.title.is_empty(), "{hit:?}");
        }
        assert!(
            page.results
                .iter()
                .filter(|h| !h.snippet.is_empty())
                .count()
                >= 15
        );
    }

    #[test]
    fn parses_infobox_as_sidebar() {
        let sidebar = parse(PAGE).sidebar.unwrap();
        assert_eq!(sidebar.title, "Albert Einstein");
        assert!(
            sidebar
                .text
                .starts_with("Albert Einstein (14 March 1879 – 18 April 1955) was a German-born")
        );
        assert!(!sidebar.text.ends_with("Wikipedia"));
        assert_eq!(
            sidebar.url.as_deref(),
            Some("https://en.wikipedia.org/wiki/Albert_Einstein")
        );
        assert_eq!(
            sidebar.facts[0],
            (
                "Born".into(),
                "14 March 1879; Ulm, Kingdom of Württemberg, German Empire".into()
            )
        );
        assert_eq!(
            sidebar
                .facts
                .iter()
                .filter(|(label, _)| label == "Born")
                .count(),
            1
        );
    }
}
