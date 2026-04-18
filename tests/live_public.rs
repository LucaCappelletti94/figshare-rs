#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::unwrap_used
)]

use figshare_rs::{ArticleQuery, DefinedType, FigshareClient};

#[tokio::test]
#[ignore = "requires network access to the public Figshare API"]
async fn daily_public_api_surface() {
    let client = FigshareClient::anonymous().expect("build anonymous client");

    let licenses = client.list_licenses().await.expect("list licenses");
    assert!(
        !licenses.is_empty(),
        "public license catalog should not be empty"
    );

    let categories = client.list_categories().await.expect("list categories");
    assert!(
        !categories.is_empty(),
        "public category catalog should not be empty"
    );

    let hits = client
        .search_public_articles(
            &ArticleQuery::builder()
                .item_type(DefinedType::Dataset)
                .limit(3)
                .build(),
        )
        .await
        .expect("search public articles");
    assert!(
        !hits.is_empty(),
        "dataset search should return public results"
    );

    let latest = client
        .resolve_latest_public_article(hits[0].id)
        .await
        .expect("resolve latest public article");
    assert!(
        !latest.title.is_empty(),
        "latest article should have a title"
    );
}
