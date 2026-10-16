mod config;
mod engines;
mod page;

use std::io::{self, IsTerminal, Read, Write};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use clap::Parser;

use crate::config::Config;
use crate::engines::{Engine, Request};

/// Query a web search engine and print the results.
///
/// Settings are read from $XDG_CONFIG_HOME/wq/config.toml; flags override them.
/// Exits 0 with output, 1 when the engine returns nothing, 2 on error.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Search engine [default: ddg]
    #[arg(short, long, value_enum)]
    engine: Option<Engine>,

    /// Maximum number of results [default: 10]
    #[arg(short = 'n', long)]
    count: Option<usize>,

    /// Print the engine's AI summary, if it offers one [default: off]
    #[arg(long, overrides_with = "no_summary")]
    summary: bool,

    /// Do not print the AI summary
    #[arg(long)]
    no_summary: bool,

    /// Print the engine's sidebar (infobox or instant answer), if it returns one [default: on]
    #[arg(long, overrides_with = "no_sidebar")]
    sidebar: bool,

    /// Do not print the sidebar
    #[arg(long)]
    no_sidebar: bool,

    /// Print the page as JSON
    #[arg(long)]
    json: bool,

    /// Search query; read from stdin when omitted. Options must come before it.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    query: Vec<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("wq: {e:#}");
            ExitCode::from(2)
        }
    }
}

/// Returns whether anything was printed.
fn run() -> Result<bool> {
    let cli = Cli::parse();
    let config = Config::load()?;
    let query = match cli.query.is_empty() {
        false => normalize(&cli.query.join(" ")),
        true => read_stdin()?,
    };
    if query.is_empty() {
        bail!("empty query");
    }

    let engine = cli.engine.or(config.engine).unwrap_or(Engine::Ddg);
    let count = cli.count.or(config.count).unwrap_or(10);
    let summary = flag(cli.summary, cli.no_summary)
        .or(config.summary)
        .unwrap_or(false);
    let sidebar = flag(cli.sidebar, cli.no_sidebar)
        .or(config.sidebar)
        .unwrap_or(true);

    let mut page = engine.search(
        &Request {
            query: &query,
            count,
            summary,
        },
        &config,
    )?;
    page.results.truncate(count);
    if !summary {
        page.summary = None;
    }
    if !sidebar {
        page.sidebar = None;
    }
    if page.is_empty() {
        eprintln!("wq: no results");
        return Ok(false);
    }

    let out = match cli.json {
        true => serde_json::to_string_pretty(&page)? + "\n",
        false => page.to_text(),
    };
    match io::stdout().write_all(out.as_bytes()) {
        Err(e) if e.kind() != io::ErrorKind::BrokenPipe => Err(e).context("cannot write to stdout"),
        _ => Ok(true),
    }
}

fn read_stdin() -> Result<String> {
    let mut stdin = io::stdin();
    if stdin.is_terminal() {
        bail!("no query; pass it as arguments or on stdin");
    }
    let mut text = String::new();
    stdin
        .read_to_string(&mut text)
        .context("cannot read stdin")?;
    Ok(normalize(&text))
}

/// Joins the words of a possibly multi-line query with single spaces.
fn normalize(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Resolves a `--x` / `--no-x` pair; `None` when neither was given.
fn flag(on: bool, off: bool) -> Option<bool> {
    match (on, off) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_follows_double_dash_or_options() {
        let cli =
            Cli::try_parse_from(["wq", "-e", "bing", "--", "how", "to", "boil", "water"]).unwrap();
        assert_eq!(cli.query, ["how", "to", "boil", "water"]);
        let cli = Cli::try_parse_from(["wq", "-n", "3", "rust", "-tutorial"]).unwrap();
        assert_eq!(cli.count, Some(3));
        assert_eq!(cli.query, ["rust", "-tutorial"]);
    }

    #[test]
    fn last_toggle_wins() {
        let cli = Cli::try_parse_from(["wq", "--no-sidebar", "--sidebar", "q"]).unwrap();
        assert_eq!(flag(cli.sidebar, cli.no_sidebar), Some(true));
        let cli = Cli::try_parse_from(["wq", "--summary", "--no-summary", "q"]).unwrap();
        assert_eq!(flag(cli.summary, cli.no_summary), Some(false));
        let cli = Cli::try_parse_from(["wq", "q"]).unwrap();
        assert_eq!(flag(cli.summary, cli.no_summary), None);
    }

    #[test]
    fn normalizes_multiline_queries() {
        assert_eq!(normalize("  how to\n boil\twater\n"), "how to boil water");
    }
}
