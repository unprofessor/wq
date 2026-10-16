# wq

Query a web search engine from the command line and print the results.

```sh
wq -- how to boil water
echo 'how to boil water' | wq
wq -e brave -n 5 albert einstein
wq -e searxng --json rust borrow checker | jq -r '.results[].url'
```

Options go before the query. Use `--` when the query starts with `-`.

```
1. How to Boil Water: 5 Quick & Easy Ways - wikiHow
   https://www.wikihow.com/Boil-Water
   To boil water for cooking, fill a pot with water and place it on a stovetop…
```

When enabled and returned by the engine, the AI summary prints first, then
the sidebar (infobox or instant answer), then the results.

## Engines

| Engine      | Access                  | Sidebar                   | AI summary  |
|-------------|-------------------------|---------------------------|-------------|
| `ddg`       | scraped, no key         | instant answer            | –           |
| `bing`      | scraped, no key         | –                         | –           |
| `brave`     | scraped, no key         | infobox                   | –           |
| `brave-api` | API key                 | infobox                   | Answers API |
| `searxng`   | your instance's URL     | infobox or answer         | –           |

- **Scraped engines** break when the engine changes its page layout, and they
  rate-limit automated clients. DuckDuckGo answers bursts of queries with a
  CAPTCHA; `wq` reports that and exits 2. Waiting or switching engines helps.
- **`brave-api`** uses the [Brave Search API](https://api-dashboard.search.brave.com/).
  The AI summary comes from the Answers endpoint, which is a second request
  billed separately under the Answers plan, so `summary` defaults to off.
- **`searxng`** needs an instance with `json` listed under `search.formats` in
  its `settings.yml`. Most public instances disable it, so self-hosting is the
  practical route.
- **Google** is not supported: its results page requires JavaScript, and its
  whole-web JSON API closes to existing customers on 2027-01-01.

## Configuration

`wq` reads `$XDG_CONFIG_HOME/wq/config.toml` (usually `~/.config/wq/config.toml`).
Every key is optional; flags override the file.

```toml
engine = "bing"   # ddg (default), bing, brave, brave-api, searxng
count = 10        # maximum results; -n
summary = false   # AI summary; --summary / --no-summary
sidebar = true    # sidebar content; --sidebar / --no-sidebar

[searxng]
url = "http://localhost:8888"

[brave_api]
key = "..."
answers_key = "..."   # optional; defaults to key
```

## Exit status

`0` when something was printed, `1` when the engine returned nothing, `2` on
error.

## Building

```sh
cargo install --path .
```
