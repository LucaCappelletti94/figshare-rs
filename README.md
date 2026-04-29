# figshare-rs

[![CI](https://github.com/LucaCappelletti94/figshare-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/LucaCappelletti94/figshare-rs/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/LucaCappelletti94/figshare-rs/graph/badge.svg)](https://codecov.io/gh/LucaCappelletti94/figshare-rs)
[![crates.io](https://img.shields.io/crates/v/figshare-rs.svg)](https://crates.io/crates/figshare-rs)
[![docs.rs](https://img.shields.io/docsrs/figshare-rs)](https://docs.rs/figshare-rs)
[![License](https://img.shields.io/crates/l/figshare-rs.svg)](https://github.com/LucaCappelletti94/figshare-rs/blob/main/LICENSE)

Async Rust client for core [Figshare](https://figshare.com/) workflows.

It provides a typed API built around [`FigshareClient`](https://docs.rs/figshare-rs/latest/figshare_rs/client/struct.FigshareClient.html) for public article reads, exact DOI lookup, latest-version resolution, private article updates, hosted uploads, publication, and public or private file downloads, with typed models, metadata and query builders, and higher-level workflow helpers. Use [`FigshareClient::anonymous`](https://docs.rs/figshare-rs/latest/figshare_rs/client/struct.FigshareClient.html#method.anonymous) for public reads and `FIGSHARE_TOKEN` for authenticated account workflows.

The shared cross-client traits from [`client-uploader-traits`](https://github.com/LucaCappelletti94/client-uploader-traits) are re-exported as `figshare_rs::client_uploader_traits`. Import `figshare_rs::client_uploader_traits::prelude::*` when you want to write generic code against the aligned `client-rs` trait surface.

> [!WARNING]
> Figshare has asked us to turn off the daily live API tester for this project. The live smoke suite was working up to April 29, 2026, but because it is no longer run automatically every day, we cannot guarantee that this crate will maintain API parity with Figshare.

> [!WARNING]
> For regular free `figshare.com` accounts, Figshare currently offers a `20GB` total storage quota and a `20GB` maximum individual file size, not `20GB` per document:
> <https://help.figshare.com/article/figshare-account-limits>
> <https://info.figshare.com/user-guide/file-size-limits-and-storage/>
>
> I created this crate thinking that the `20GB` limit was per document rather than total account storage. If you are looking for a general-purpose personal archival workflow, I suggest using [zenodo-rs](https://github.com/LucaCappelletti94/zenodo-rs) instead.
>
> This crate still exists because Figshare is used by institutions and publishers, and the API client can still be useful in those environments.

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
