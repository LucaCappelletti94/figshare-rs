# figshare-rs

[![CI](https://github.com/LucaCappelletti94/figshare-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/LucaCappelletti94/figshare-rs/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/LucaCappelletti94/figshare-rs/graph/badge.svg)](https://codecov.io/gh/LucaCappelletti94/figshare-rs)
[![crates.io](https://img.shields.io/crates/v/figshare-rs.svg)](https://crates.io/crates/figshare-rs)
[![docs.rs](https://img.shields.io/docsrs/figshare-rs)](https://docs.rs/figshare-rs)
[![License](https://img.shields.io/crates/l/figshare-rs.svg)](https://github.com/LucaCappelletti94/figshare-rs/blob/main/LICENSE)

Async Rust client for core [Figshare](https://figshare.com/) workflows.

It covers public article reads, exact DOI lookup, latest-version resolution,
private article updates, hosted uploads, publication, and public/private file
downloads through a typed API built around
[`FigshareClient`](https://docs.rs/figshare-rs/latest/figshare_rs/client/struct.FigshareClient.html).

The crate provides:

- typed request and response models with forward-compatible `extra` fields
- metadata and query builders
- high-level workflow helpers for upload and publish flows
- mock-heavy tests plus scheduled live smoke checks

## Install

```toml
[dependencies]
figshare-rs = "0.1"
```

Optional features:

- `native-tls`: use `reqwest` with `native-tls` instead of the default `rustls-tls`

If your application needs a Tokio runtime, `rt` plus `macros` is enough:

```toml
tokio = { version = "1", features = ["rt", "macros"] }
```

## Examples

```rust
use figshare_rs::{
    ArticleOrder, ArticleQuery, Auth, DefinedType, Endpoint, FigshareClient, OrderDirection,
};

let client = FigshareClient::builder(Auth::anonymous())
    .endpoint(Endpoint::Custom("http://localhost:8080/v2/".parse()?))
    .build()?;
let query = ArticleQuery::builder()
    .item_type(DefinedType::Dataset)
    .order(ArticleOrder::PublishedDate)
    .order_direction(OrderDirection::Desc)
    .limit(3)
    .build();

assert_eq!(client.endpoint().base_url()?.as_str(), "http://localhost:8080/v2/");
assert_eq!(query.item_type, Some(DefinedType::Dataset));
assert_eq!(query.limit, Some(3));
assert_eq!(query.order, Some(ArticleOrder::PublishedDate));
# Ok::<(), Box<dyn std::error::Error>>(())
```

Build publication inputs locally:

```rust
use figshare_rs::{ArticleMetadata, DefinedType, UploadSource, UploadSpec};

let metadata = ArticleMetadata::builder()
    .title("Example dataset")
    .defined_type(DefinedType::Dataset)
    .description("Example upload from Rust")
    .author_named("Doe, Jane")
    .tag("example")
    .build()?;
let upload = UploadSpec::from_reader(
    "artifact.tar.gz",
    std::io::Cursor::new(vec![1_u8, 2, 3, 4]),
    4,
);

assert_eq!(metadata.title, "Example dataset");
assert_eq!(metadata.defined_type, DefinedType::Dataset);
assert_eq!(metadata.tags, vec!["example".to_owned()]);
match upload.source {
    UploadSource::Reader { content_length, .. } => assert_eq!(content_length, 4),
    UploadSource::Path(_) => unreachable!("expected reader-backed upload"),
}
# Ok::<(), figshare_rs::ArticleMetadataBuildError>(())
```

## Authentication

- `FIGSHARE_TOKEN` is the standard env var for authenticated account workflows.
- Use [`FigshareClient::anonymous`](https://docs.rs/figshare-rs/latest/figshare_rs/client/struct.FigshareClient.html#method.anonymous) for public reads.
- Hosted upload calls use the same token against both the main API and the upload service.
- Private file downloads add the token to the Figshare downloader URL when required by the API.

## CI

- `ci.yml` runs formatting, docs, clippy, tests, semver, audit, packaging, and coverage.
- `live-daily.yml` runs the public live smoke test every day.
- Authenticated live smoke is draft-only, requires `FIGSHARE_TOKEN`, and is manual opt-in.
- The live CI setup is documented in [docs/live-api-ci.md](docs/live-api-ci.md).
