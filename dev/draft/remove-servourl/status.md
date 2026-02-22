# Remove ServoUrl Investigation

## Problem

ServoUrl wraps `Arc<url::Url>`. HPPR URLs use UTF-8 coordinates with `{` `}` `#` in JSONqa. The code percent-encodes these to survive `Url::parse`, then decodes them back for display. The canonical form should be UTF-8.

## ServoUrl Usage by Component

| Component | References | Files |
|---|---|---|
| components/script | 475 | ~80 |
| components/net | 168 | ~25 |
| components/shared | 116 | ~20 |
| components/url | 40 | 4 |
| components/storage | 24 | 4 |
| components/constellation | 22 | 4 |
| ports/servoshell | 20 | 4 |
| components/servo | 12 | 4 |
| components/devtools | 12 | 5 |
| components/layout | 11 | 4 |
| components/fonts | 9 | 3 |
| components/webdriver_server | 2 | 1 |
| ports/havishell | 2 | 1 |
| **Total** | **~932** | **~133 non-test files** |

## Method Usage Map

Every ServoUrl method, ranked by call count across non-test code, with what callers actually need:

| Method | Calls | What callers get | HPPR impact |
|---|---|---|---|
| `as_str()` | ~200 | String representation for logging, comparison, display, IPC | Works if wrapper stores string. Most common use. |
| `origin()` | ~180 | `ImmutableOrigin` for same-origin checks | Already has HPPR-specific logic. Works. |
| `parse(s)` | ~82 | Constructor from string | Entry point — this is where encoding damage happens. |
| `scheme()` | ~80 | "hppr", "http", "about", etc. | Trivial string op. |
| `path()` | ~55 | URL path component | HPPR: returns encoded `//group/app/loc`. Callers treat as opaque mostly. |
| `host()` / `host_str()` | ~50 | Host for cookies, CORS, HSTS, websockets | HPPR URLs have no host. Returns `None`. Callers already guard. |
| `port()` | ~30 | Port number | HPPR: returns `None`. Callers already guard. |
| `as_mut_url()` | ~25 | Mutable `&mut Url` for `set_scheme`, `set_fragment`, etc. | Only in HTTP paths (fetch, HSTS, websocket). |
| `into_string()` | ~20 | Owned string, consuming self | Trivial. |
| `as_url()` | ~18 | `&Url` for `url::quirks::*`, `Position` indexing, `UrlExtraData` | Hard coupling to rust-url. See below. |
| `into_url()` | ~15 | Owned `Url` for hyper/tungstenite/stylo | Hard coupling. All in HTTP/websocket paths. |
| `get_arc()` | ~22 | `Arc<Url>` for `UrlExtraData` (stylo CSS) | Hard coupling. ~22 call sites in CSS/style code. |
| `fragment()` | ~15 | Fragment identifier | HPPR uses JSONqa `{#:text}` instead. |
| `join(s)` | ~10 | Relative URL resolution | Already has HPPR override (`join_hppr`). |
| `domain()` | ~15 | Registrable domain for cookies | HTTP-only. HPPR returns `None`. |
| `username()` / `password()` | ~13 each | HTTP auth credentials | HTTP-only. HPPR returns empty/None. |
| `matches_about_blank()` | ~10 | Check for `about:blank` | Scheme check. Works for any type. |
| `is_potentially_trustworthy()` | ~12 | Security check | Has HPPR-aware origin(). Works. |
| `query()` | ~8 | Query string | HPPR doesn't use. Returns `None`. |
| `cannot_be_a_base()` | ~8 | `data:`, `javascript:` etc. | HPPR returns `false`. Correct. |
| `path_segments()` | ~6 | Iterator over path segments | HTTP code only. |
| `to_file_path()` | ~5 | `file://` to filesystem path | File scheme only. |
| `is_secure_scheme()` | ~5 | HTTPS/WSS check | Returns false for HPPR. Correct. |
| `parse_with_base()` | ~12 | Relative resolution against base | Uses `Url::options().base_url()`. |
| `is_local_scheme()` | ~2 | about/blob/data check | Returns false for HPPR. Correct. |
| `is_special_scheme()` | ~2 | HTTP/HTTPS/file/etc. | Returns false for HPPR. Correct. |
| `is_equal_excluding_fragments()` | ~2 | Compare URLs ignoring fragment | Cookie code only. |
| `hppr_display_url()` | ~5 | Decode `%7B`→`{` for display | Exists solely because of the encoding problem. |
| `hosturc()` | 0 | Parse as `HAVIAddress` | Unused outside url crate! |
| `debug_compact()` | ~3 | Short display for thread names | HTTP-focused. |
| `from_file_path()` | ~2 | Filesystem path to `file://` URL | File scheme only. |
| `set_username/password/ip_host` | ~2 | Mutate URL | HTTP proxy code only. |
| `set_fragment()` | ~3 | Set fragment | HTTP code. |
| `port_or_known_default()` | ~2 | Port with 80/443 default | HTTP/websocket only. |

## Hard Couplings to rust-url's `Url`

These are the non-mechanical dependencies — places that need an actual `url::Url` object.

### 1. `get_arc()` → `UrlExtraData` (stylo/CSS) — 22 sites

```
UrlExtraData(url.get_arc())
```

Stylo's `UrlExtraData` wraps `Arc<url::Url>`. Call sites:
- `components/script/stylesheet_loader.rs` (2)
- `components/script/dom/css/` (8)
- `components/script/dom/html/htmlbodyelement.rs` (1)
- `components/script/dom/html/htmlelement.rs` (1)
- `components/script/dom/html/htmlstyleelement.rs` (1)
- `components/script/dom/element.rs` (3)
- `components/script/dom/node.rs` (2)
- `components/script/dom/media/medialist.rs` (3)
- `components/layout/query.rs` (1)

**What stylo actually does with it:**

Inspected the stylo source (`style/stylesheets/mod.rs` at rev 9a413b4). `UrlExtraData` for the `servo` feature is `pub struct UrlExtraData(pub Arc<::url::Url>)` with exactly two methods:

```rust
pub fn chrome_rules_enabled(&self) -> bool { self.0.scheme() == "chrome" }
pub fn as_str(&self) -> &str { self.0.as_str() }
```

Stylo never decomposes the URL — no `host()`, `port()`, `path()` calls. It checks if the scheme is `"chrome"` (never true for HPPR) and gets the string representation. The `Arc<Url>` is stored alongside parsed CSS values as provenance metadata.

**Implication:** `UrlExtraData` could wrap `Arc<String>` instead of `Arc<Url>`. The `Url` type adds nothing. For `BrowserUrl`, a `to_arc_url()` method that constructs a `Url` on demand works, or better: upstream a `UrlExtraData::from_str()` to stylo (it only needs `.scheme()` and `.as_str()`).

### 2. `as_url()` → `url::quirks::*` (WHATWG URL API) — 13 getters + 9 setters in urlhelper.rs

`urlhelper.rs` implements the WHATWG URL API (`URL.hostname`, `URL.pathname`, etc.) via `url::quirks::*`. Called from:
- `dom/url.rs` — the JS `URL()` constructor and its properties
- `dom/location.rs` — `window.location.*` properties
- `dom/html/htmlhyperlinkelementutils.rs` — `<a>.hostname`, `<a>.pathname`, etc.

**What actually flows through these:**

The JS `URL()` constructor (`dom/url.rs:55`) calls `Url::parse()` directly. HPPR URLs with `{` in JSONqa would fail `Url::parse` and throw a `TypeError` — web content can never construct a `URL` object from an HPPR string.

`window.location` and `<a>.href` do carry the document's URL, which can be HPPR. But the current behavior is already wrong: `location.hostname` returns the group name, `location.port` returns empty, `location.pathname` returns `/app/location`. This is nonsensical from a web-standard perspective. No web content running on HPPR pages uses these properties to decompose HPPR coordinates.

**Implication:** For `BrowserUrl::Hppr`, `url::quirks` calls need a fallback. Options: (a) construct a throwaway `Url` for the quirks call (preserves current broken behavior), (b) return HPPR-native values, (c) return empty strings. Nothing depends on this working correctly for HPPR.

### 3. `as_url()` → `Position` indexing — 7 sites

```rust
// document.rs:2164 — hashchange comparison
old_url.as_url()[Position::BeforeFragment..] != new_url.as_url()[Position::BeforeFragment..]

// window.rs:3404 — same-document navigation check
load_data.url.as_url()[..Position::AfterQuery] == doc.url().as_url()[..Position::AfterQuery]

// response.rs:473 — strip fragment for Response.url
&url[..Position::AfterQuery]

// xmlhttprequest.rs:1034 — XHR response URL
metadata.final_url[..Position::AfterQuery].clone_into(...)

// protocols/mod.rs:209 — strip scheme for data URL
&url[Position::AfterScheme..][1..]

// mediafragmentparser.rs:194,201 — media fragment parsing
&url[Position::AfterPath..]
```

**What these actually need:**

- `BeforeFragment`/`AfterQuery` comparisons: "compare URLs ignoring fragment". For `BrowserUrl::Hppr`, JSONqa `{#:text}` is the fragment equivalent. A dedicated method `without_fragment()` or `is_same_document()` replaces all of these.
- `AfterScheme`: strips scheme prefix. Trivial string operation.
- `AfterPath`: extracts query/fragment. HPPR has no query string; the JSONqa suffix serves a different purpose.

**Implication:** Replaceable with dedicated methods on `BrowserUrl`. `Position`-based indexing is an implementation detail of rust-url, not a semantic need.

### 4. `into_url()` → external APIs — 15 sites

Traced every call site to its actual consumer:

**a) HTTP request construction (http_loader.rs:673)**
```rust
url.clone().into_url().as_ref().replace('|', "%7C").replace('{', "%7B").replace('}', "%7D")
```
Converts to `Url`, immediately converts to `&str`, then percent-encodes special chars. **Does not use `Url` at all** — `.as_str()` suffices.

**b) CSP (fetch/methods.rs:262, security/csp.rs:118, htmlbaseelement.rs:90)**
The `content-security-policy` crate's `Request` struct has `url: Url`. The crate checks `url.scheme()`, `url.host()`, `url.port()`, `url.path()` for directive matching. HPPR URLs never reach CSP checks (no HTTP content policies apply).

**c) WebResourceRequest (request_interceptor.rs:36)**
`WebResourceRequest.url: Url` — public embedder API for request interception. HPPR requests use a different protocol handler and don't hit this interceptor.

**d) Websocket (websocket_loader.rs:324)**
`into_url()` for `set_host()` and TCP connect. WebSockets are HTTP-only.

**e) Servo public API (servo.rs:337,386,407)**
Converts for `AuthenticationRequest.url: Url`, `NavigationRequest.url: Url`, `ProtocolHandlerRegistration.url: Url`. These are Servo's public embedding API (`components/servo/webview_delegate.rs`).

**f) Webview back/forward list (webview.rs:674)**
Populates `back_forward_list: Vec<Url>` — public API.

**g) WebView.load (servoshell/window.rs:307)**
`WebView::load` takes `Url`.

**h) Context menu (document_embedder_controls.rs:291,312)**
`ContextMenuElementInformation` has `link_url: Option<Url>`, `image_url: Option<Url>`. Public API.

**i) Webdriver (webdriver_server/lib.rs:721)**
`WebDriverCommandMsg::LoadUrl(_, Url, _)` uses raw `Url`.

**j) SVG (svgsvgelement.rs:223)**
`doc.url().into_url().into()` → creates `UrlExtraData`. Same as coupling #1 — only needs `.scheme()` and `.as_str()`.

**k) User stylesheet (servoshell/desktop/app.rs:125)**
`UserStyleSheet::new(contents, url.clone().into_url())` — stylo. Same as coupling #1.

**Summary:**

| Consumer | Actually needs `Url`? | Notes |
|---|---|---|
| http_loader.rs:673 | **No** | Immediately converts to string |
| CSP crate | Yes (external API) | HTTP-only path |
| WebResourceRequest | Yes (public API) | HTTP-only path |
| Websocket | Yes (host/port ops) | HTTP-only |
| Servo public API | Yes (public API) | **The real boundary** — 5 sites |
| Webdriver | Yes (command type) | Uses raw `Url` |
| SVG/stylesheet | **No** | Only needs string |

**The Servo "public API" is ours to change.** `WebView::load()`, `WebView::url()`, `NavigationRequest`, `AuthenticationRequest`, `ContextMenuElementInformation`, `back_forward_list` — all currently expose `url::Url` to embedders. Since we own this API and are abandoning servoshell compatibility, these change to `BrowserUrl` directly. No conversion needed.

Sites that currently use raw `Url` in the Servo/embedder API:
- `WebView::load(Url)` → `WebView::load(BrowserUrl)`
- `WebView::url() -> Option<Url>` → `-> Option<BrowserUrl>`
- `WebViewDelegate::notify_url_changed(_, Url)` → `(_, BrowserUrl)`
- `NavigationRequest.url: Url` → `BrowserUrl`
- `AuthenticationRequest.url: Url` → `BrowserUrl`
- `ContextMenuElementInformation.{link,image}_url: Option<Url>` → `Option<BrowserUrl>`
- `WebResourceRequest.url: Url` / `WebResourceResponse.url: Url` → `BrowserUrl`
- `back_forward_list: Vec<Url>` → `Vec<BrowserUrl>`
- `EmbedderToConstellationMessage::LoadUrl(_, ServoUrl)` → already `ServoUrl`, becomes `BrowserUrl`
- `WebDriverCommandMsg::LoadUrl(_, Url, _)` → `BrowserUrl`

Havishell simplifies:
- `app.rs:945` — `url::Url::parse(url_str)` → `BrowserUrl::parse(url_str)` (no more mangling `{`/`}`)
- `app.rs:878` — `decode_hppr_display(new_url.as_str())` → `new_url.as_str()` (already UTF-8)
- `app.rs:1693` — entire `decode_hppr_display()` function deleted
- `notify_url_changed` receives `BrowserUrl`, `.as_str()` gives clean UTF-8

### 5. `as_mut_url()` — 25 sites

All HTTP-specific:
- `fetch/methods.rs:258,328,414` — `set_scheme("wss")` for websocket upgrade
- `http_loader.rs:271` — redirect handling
- `net/resource_thread.rs:909` — `set_scheme` for HSTS
- `hsts.rs:237` — `set_scheme("https")`
- `dom/urlhelper.rs:51+` — URL API setters (`set_hash`, `set_host`, etc.)
- Various DOM: `set_fragment` for navigation

None apply to HPPR URLs.

### 6. Devtools — already broken for HPPR

`components/devtools/actors/network_event.rs:585-595` has HPPR-specific workaround code:
```rust
if url.scheme() == "hppr" || url.scheme().starts_with("hppr-") {
    let host = url.host_str().unwrap_or("");       // group
    let app = url.path().trim_start_matches('/').split('/').next().unwrap_or("");
    // ... format as "group/app"
}
```
This manually reconstructs HPPR coordinate semantics from the HTTP URL decomposition. With `BrowserUrl::Hppr`, this becomes `address.group()` and `address.app()`.

## Categorization of All 133 Non-Test Files

### Pure mechanical (type signature, use statement, pass-through): ~105 files

These files mention `ServoUrl` only in:
- `use servo_url::ServoUrl;`
- Struct fields: `pub url: ServoUrl`
- Function parameters: `fn foo(url: &ServoUrl)`
- Return types: `-> ServoUrl`
- `ServoUrl::parse("about:blank")` (constant URLs)
- `.as_str()`, `.scheme()`, `.origin()` (simple accessors)

A rename from `ServoUrl` to `BrowserUrl` in these files is pure find-and-replace.

### Structural but HTTP-only: ~18 files

These call HTTP-specific methods (`host()`, `port()`, `domain()`, `username()`, `password()`, `as_mut_url()`, `into_url()`, `to_file_path()`) but only in code paths that handle HTTP/HTTPS/WSS/file URLs:

- `components/net/http_loader.rs` — HTTP request/response handling
- `components/net/fetch/methods.rs` — Fetch algorithm
- `components/net/websocket_loader.rs` — WebSocket
- `components/net/cookie_storage.rs` — Cookie jar
- `components/net/cookie.rs` — Cookie parsing
- `components/net/hsts.rs` — HSTS upgrade
- `components/net/connector.rs` — TLS/connection
- `components/net/protocols/file.rs` — file:// handler
- `components/net/local_directory_listing.rs` — directory listing
- `components/script/dom/xmlhttprequest.rs` — XHR
- `components/script/dom/request.rs` — Request API
- `components/script/dom/response.rs` — Response API
- `components/script/dom/history.rs` — history API
- `components/script/dom/location.rs` — Location API
- `components/script/dom/html/htmliframeelement.rs` — iframe
- `components/script/dom/html/htmlhyperlinkelementutils.rs` — <a> element
- `components/devtools/actors/network_event.rs` — devtools
- `components/webdriver_server/lib.rs` — webdriver

For a `BrowserUrl` enum, these need match arms or `.as_web_url()` accessors. HPPR URLs never reach these code paths, so the match arm is `unreachable!()` or an early return.

### CSS/stylo coupling: ~12 files

Call `get_arc()` for `UrlExtraData`. Need `Arc<Url>` specifically:

All listed under "Hard Couplings §1" above. For `BrowserUrl`, these could call `.to_arc_url()` which constructs an `Arc<Url>` on demand for HPPR (the coordinate is a valid opaque URL).

### WHATWG URL API: 2 files

- `components/script/dom/urlhelper.rs` — uses `url::quirks::*`
- `components/script/dom/url.rs` — `URL` constructor

These need `Url` for the web-standard URL API. For `BrowserUrl`, the `URL()` constructor would continue to produce only web URLs (HPPR strings would fail `Url::parse` and throw). `urlhelper.rs` methods are only called on URLs created via `URL()`.

### Position indexing: 5 files

- `components/net/protocols/mod.rs`
- `components/script/dom/media/mediafragmentparser.rs`
- `components/script/dom/document.rs`
- `components/script/dom/xmlhttprequest.rs`
- `components/script/dom/response.rs`

All HTTP/web code paths. `document.rs` uses it for fragment-comparison, but already uses `as_url()`.

### HPPR-aware: ~5 files

- `components/url/lib.rs` — the wrapper itself
- `components/url/hppr.rs` — HPPR URL types
- `ports/havishell/src/protocols/mod.rs` — protocol dispatch
- `ports/havishell/src/app.rs` — address bar
- `ports/servoshell/parser.rs` — URL bar parsing

## The Boundary

```
Address bar input (UTF-8 string)
  → BrowserUrl::parse()
    → if hppr/havi scheme: BrowserUrl::Hppr(HAVIAddress) — stores UTF-8 natively
    → if web scheme: BrowserUrl::Web(Arc<Url>) — stores percent-encoded
  → passes through constellation, pipeline, message types as BrowserUrl
  → at net layer:
    → HPPR protocol handler: match BrowserUrl::Hppr(addr) → native HPPR fetch
    → HTTP handler: match BrowserUrl::Web(url) → existing http_loader
  → at script layer:
    → document.url is BrowserUrl, .as_str() works for both
    → URL() constructor only creates Web variant
    → get_arc() for CSS: .to_arc_url() method, converts HPPR to opaque Url on demand
```

## Approach: Full Replacement

Replace `ServoUrl` with:

```rust
#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, MallocSizeOf)]
pub enum BrowserUrl {
    /// Web URL (HTTP, HTTPS, file, about, data, blob, javascript)
    Web(#[conditional_malloc_size_of] Arc<Url>),
    /// HPPR coordinate URL — stored as canonical UTF-8
    Hppr(Arc<HpprUrlData>),
}

#[derive(Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, MallocSizeOf)]
pub struct HpprUrlData {
    /// Full URL string: "hppr://group/app/location{jsonqa}"
    raw: String,
    /// Parsed address (scheme + endpoint + URC)
    address: HAVIAddress,
    /// JSONqa suffix if present
    jsonqa: String,
}
```

### Methods on BrowserUrl

| Method | Implementation |
|---|---|
| `as_str()` | Web: `url.as_str()`, Hppr: `&raw` |
| `scheme()` | Web: `url.scheme()`, Hppr: `address.scheme().prefix().trim_end_matches(':')` |
| `origin()` | Existing logic, already dispatches |
| `parse(s)` | Detect scheme, route to `Url::parse` or `HAVIAddress::parse` |
| `join(s)` | Web: `url.join()`, Hppr: existing `join_hppr` logic |
| `path()` | Web: `url.path()`, Hppr: `format!("//{}", urc)` or similar |
| `host()` | Web: `url.host()`, Hppr: `None` |
| `port()` | Web: `url.port()`, Hppr: `None` |
| `fragment()` | Web: `url.fragment()`, Hppr: parse from jsonqa `{#:...}` |
| `as_web_url()` | `Option<&Url>` — returns `None` for Hppr |
| `to_arc_url()` | For UrlExtraData. Hppr: construct `Arc<Url>` from `raw` on demand |
| `is_hppr()` | `matches!(self, BrowserUrl::Hppr(_))` |
| `matches_about_blank()` | Web: existing, Hppr: `false` |
| All `set_*` | Web: delegate, Hppr: not applicable (modify address directly) |

### Change Scope by File Category

| Category | Files | Change type |
|---|---|---|
| Pure mechanical | ~105 | `s/ServoUrl/BrowserUrl/g` — automated |
| HTTP-only structural | ~18 | Add `.as_web_url().unwrap()` or match arm at 1-3 sites per file |
| CSS/stylo coupling | ~12 | Replace `.get_arc()` with `.to_arc_url()` |
| WHATWG URL API | 2 | Add match arm, web-only path |
| Position indexing | 5 | Already use `.as_url()`, change to `.as_web_url().unwrap()` |
| HPPR-aware | ~5 | Simplify — no more encode/decode dance |
| Test files | ~16 | Mechanical rename |

## Risk Assessment

| Risk | Severity | Analysis |
|---|---|---|
| Compile errors from match exhaustiveness | Low | Compiler catches all. No runtime risk. |
| `to_arc_url()` for CSS on HPPR pages | Low | Constructs valid opaque URL. CSS resolution still uses `join()` which handles HPPR. |
| `host()`/`port()` returning None for HPPR | None | Already returns None — `url::Url` has no host for opaque URLs like `hppr:` |
| Serde format change | Medium | `BrowserUrl` serialization must be backward compatible or coordinated with IPC restart. Tag enum with `#[serde(untagged)]` or string-based. |
| `PartialOrd`/`Hash` stability | Low | Derive from string representation. Same as current. |
| External crates expecting `Url` | Low | Only at boundaries already identified (hyper, tungstenite, stylo). All HTTP-only. |
| `url::quirks` for HPPR | None | `URL()` JS constructor never creates HPPR URLs. No web content uses `new URL("hppr://...")`. |

## What Simplifies

- **No more `encode_jsonqa_for_url` / `percent_decode_jsonqa`** — the entire encode/decode dance disappears
- **No more `hppr_display_url()`** — `as_str()` returns the canonical UTF-8 form directly
- **No more `hosturc()`** — HPPR variant already has the parsed `HAVIAddress`
- **`join()` simplifies** — no need to strip/re-encode JSONqa through `Url::parse`
- **`origin()` simplifies** — direct match on variant instead of re-parsing the string
- **Address bar display** — no `decode_hppr_display()` needed

## Recommendation

**Full replacement.** No hard couplings remain:

1. ~105 of 133 files are pure mechanical rename (`ServoUrl` → `BrowserUrl`)
2. ~18 files have HTTP-specific method calls — all in HTTP-only code paths, get `.as_web_url().unwrap()` or match arm
3. Stylo `UrlExtraData`: only needs `.scheme()` and `.as_str()`. Construct `Arc<Url>` on demand via `to_url_extra_data()` — 22 sites, same pattern
4. `url::quirks` (WHATWG URL API): already broken for HPPR; `URL()` constructor never creates HPPR URLs. No change needed.
5. `into_url()`: half the sites don't need `Url` at all. The rest are the Servo API boundary — which we own and change to `BrowserUrl`.
6. `Position` indexing: 7 sites, replaced with semantic methods
7. Servo "public API" (`WebView`, `NavigationRequest`, etc.): not a boundary. We own it, servoshell compatibility abandoned. Changes to `BrowserUrl` throughout.
8. Constellation messages (`LoadUrl`, `NewWebView`, `HistoryChanged`): already use `ServoUrl`, become `BrowserUrl`.

## What Simplifies

- `encode_jsonqa_for_url` / `percent_decode_jsonqa` — deleted
- `hppr_display_url()` — deleted, `as_str()` returns canonical UTF-8
- `hosturc()` — deleted, HPPR variant has `HAVIAddress` directly
- `decode_hppr_display()` in havishell — deleted
- `join_hppr()` — simplified, no encode/decode through `Url::parse`
- havishell `navigate()` — `BrowserUrl::parse()` instead of `url::Url::parse()`, no more mangling
- havishell `notify_url_changed` — receives `BrowserUrl`, `.as_str()` is clean
- devtools `network_event.rs` HPPR workaround — replaced with `address.group()` / `address.app()`
- `WebView::load` / `WebView::url` — native `BrowserUrl`, no conversion

## Scope

**Medium.** ~133 files, ~2-3 days. Mostly automated find-and-replace with ~35 files needing manual attention. Zero runtime risk from missed cases (compiler catches all).

## Execution Plan

1. Define `BrowserUrl` enum in `components/url/lib.rs` with all existing `ServoUrl` methods
2. Implement `Serialize`/`Deserialize` as string
3. Global rename `ServoUrl` → `BrowserUrl` in all files
4. Change Servo API types (`WebView`, `NavigationRequest`, `AuthenticationRequest`, `WebResourceRequest/Response`, `ContextMenuElementInformation`, `WebDriverCommandMsg`, `back_forward_list`) from `Url` to `BrowserUrl`
5. Fix compile errors — compiler guides every remaining change
6. Replace `get_arc()` with `to_url_extra_data()` (22 sites, same pattern)
7. Replace `Position` indexing with semantic methods (7 sites)
8. Delete: `encode_jsonqa_for_url`, `percent_decode_jsonqa`, `hppr_display_url`, `hosturc`, `decode_hppr_display`
9. Clean up devtools HPPR workaround to use native `HAVIAddress` methods
