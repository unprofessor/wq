use serde::Serialize;

/// One search response, normalized across engines.
#[derive(Debug, Default, Serialize)]
pub struct Page {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sidebar: Option<Sidebar>,
    pub results: Vec<Hit>,
}

/// Entity or instant-answer content an engine shows beside its results.
#[derive(Debug, PartialEq, Serialize)]
pub struct Sidebar {
    pub title: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct Hit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

impl Page {
    pub fn is_empty(&self) -> bool {
        self.summary.is_none() && self.sidebar.is_none() && self.results.is_empty()
    }

    /// Renders the summary, then the sidebar, then the numbered results.
    pub fn to_text(&self) -> String {
        let mut blocks = Vec::new();
        if let Some(summary) = &self.summary {
            blocks.push(format!("Summary\n{}", indent(summary, "  ")));
        }
        if let Some(sidebar) = &self.sidebar {
            let mut lines = vec![sidebar.title.clone()];
            if !sidebar.text.is_empty() {
                lines.push(indent(&sidebar.text, "  "));
            }
            for (label, value) in &sidebar.facts {
                lines.push(format!("  {label}: {value}"));
            }
            if let Some(url) = &sidebar.url {
                lines.push(format!("  {url}"));
            }
            blocks.push(lines.join("\n"));
        }
        for (i, hit) in self.results.iter().enumerate() {
            let number = format!("{}. ", i + 1);
            let pad = " ".repeat(number.len());
            let mut block = format!("{number}{}\n{pad}{}", hit.title, hit.url);
            if !hit.snippet.is_empty() {
                block.push_str(&format!("\n{pad}{}", hit.snippet));
            }
            blocks.push(block);
        }
        let mut out = blocks.join("\n\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}

fn indent(text: &str, prefix: &str) -> String {
    text.trim()
        .lines()
        .map(|line| match line.trim_end() {
            "" => String::new(),
            line => format!("{prefix}{line}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(n: usize) -> Hit {
        Hit {
            title: format!("Title {n}"),
            url: format!("https://example.com/{n}"),
            snippet: if n == 2 {
                String::new()
            } else {
                format!("Snippet {n}")
            },
        }
    }

    #[test]
    fn renders_sections_in_order() {
        let page = Page {
            summary: Some("Line one.\n\nLine two.\n".into()),
            sidebar: Some(Sidebar {
                title: "Water".into(),
                text: "A liquid.".into(),
                facts: vec![("Formula".into(), "H2O".into())],
                url: Some("https://en.wikipedia.org/wiki/Water".into()),
            }),
            results: (1..=2).map(hit).collect(),
        };
        let expected = "\
Summary
  Line one.

  Line two.

Water
  A liquid.
  Formula: H2O
  https://en.wikipedia.org/wiki/Water

1. Title 1
   https://example.com/1
   Snippet 1

2. Title 2
   https://example.com/2
";
        assert_eq!(page.to_text(), expected);
    }

    #[test]
    fn pads_continuation_lines_to_the_number_width() {
        let page = Page {
            results: (1..=10).map(hit).collect(),
            ..Page::default()
        };
        assert!(
            page.to_text()
                .ends_with("10. Title 10\n    https://example.com/10\n    Snippet 10\n")
        );
    }

    #[test]
    fn empty_page_renders_nothing() {
        assert_eq!(Page::default().to_text(), "");
    }
}
