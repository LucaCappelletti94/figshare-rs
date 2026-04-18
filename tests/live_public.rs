#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::unwrap_used
)]

use figshare_rs::{Article, ArticleFile, ArticleQuery, DefinedType, Doi, FigshareClient};
use futures_util::StreamExt;
use tempfile::tempdir;

const MAX_PUBLIC_DOWNLOAD_BYTES: u64 = 1_000_000;

struct PublicDownloadTarget {
    fetched: Article,
    latest: Article,
    version_article: Article,
    file: ArticleFile,
    doi: Doi,
}

async fn select_public_download_target(client: &FigshareClient) -> PublicDownloadTarget {
    let listed = client
        .list_public_articles(
            &ArticleQuery::builder()
                .item_type(DefinedType::Dataset)
                .limit(25)
                .build(),
        )
        .await
        .expect("list public articles");
    assert!(
        !listed.is_empty(),
        "dataset list should return public results"
    );

    let searched = client
        .search_public_articles(
            &ArticleQuery::builder()
                .item_type(DefinedType::Dataset)
                .limit(25)
                .build(),
        )
        .await
        .expect("search public articles");
    assert!(
        !searched.is_empty(),
        "dataset search should return public results"
    );

    let mut seen = std::collections::BTreeSet::new();
    for article in listed.into_iter().chain(searched) {
        if !seen.insert(article.id.0) {
            continue;
        }

        let fetched = client
            .get_public_article(article.id)
            .await
            .expect("get public article");
        let versions = client
            .list_public_article_versions(article.id)
            .await
            .expect("list public article versions");
        let Some(latest_version) = versions.iter().map(|version| version.version).max() else {
            continue;
        };
        let latest = client
            .resolve_latest_public_article(article.id)
            .await
            .expect("resolve latest public article");
        let version_article = client
            .get_public_article_version(article.id, latest_version)
            .await
            .expect("get public article version");
        let files = client
            .list_public_article_version_files(article.id, latest_version)
            .await
            .expect("list public article version files");

        let doi = version_article
            .doi
            .clone()
            .or_else(|| latest.doi.clone())
            .or_else(|| fetched.doi.clone());
        let file = files.into_iter().find(|file| {
            file.size > 0
                && file.size <= MAX_PUBLIC_DOWNLOAD_BYTES
                && !file.name.is_empty()
                && file.download_url.is_some()
                && !file.is_link_only.unwrap_or(false)
        });

        if let (Some(doi), Some(file)) = (doi, file) {
            return PublicDownloadTarget {
                fetched,
                latest,
                version_article,
                file,
                doi,
            };
        }
    }

    panic!(
        "could not find a public dataset article with a DOI and downloadable file under {} bytes",
        MAX_PUBLIC_DOWNLOAD_BYTES
    );
}

#[tokio::test]
#[ignore = "requires network access to the public Figshare API"]
async fn daily_public_api_surface() {
    let client = FigshareClient::anonymous().expect("build anonymous client");
    let dir = tempdir().expect("tempdir");

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

    let target = select_public_download_target(&client).await;
    assert_eq!(
        target.fetched.id, target.latest.id,
        "latest resolution should preserve article id"
    );
    assert_eq!(
        target.version_article.id, target.fetched.id,
        "versioned read should preserve article id"
    );
    assert!(
        !target.latest.title.is_empty(),
        "latest article should have a title"
    );

    let by_doi = client
        .get_public_article_by_doi(&target.doi)
        .await
        .expect("resolve public article by doi");
    assert_eq!(
        by_doi.id, target.fetched.id,
        "doi lookup should resolve the selected article"
    );

    let latest_by_doi = client
        .resolve_latest_public_article_by_doi(&target.doi)
        .await
        .expect("resolve latest public article by doi");
    assert_eq!(
        latest_by_doi.id, target.latest.id,
        "latest doi resolution should preserve article id"
    );
    assert!(
        !latest_by_doi.title.is_empty(),
        "latest doi resolution should return a populated article"
    );

    let mut opened_by_id = client
        .open_public_article_file_by_name(target.fetched.id, &target.file.name, true)
        .await
        .expect("open public article file by id");
    let first_chunk = opened_by_id
        .stream
        .next()
        .await
        .expect("first chunk from public file by id")
        .expect("public file by id chunk");
    assert!(
        !first_chunk.is_empty(),
        "streamed public file by id should produce bytes"
    );

    let mut opened_by_doi = client
        .open_article_file_by_doi(&target.doi, &target.file.name, true)
        .await
        .expect("open public article file by doi");
    let first_chunk = opened_by_doi
        .stream
        .next()
        .await
        .expect("first chunk from public file by doi")
        .expect("public file by doi chunk");
    assert!(
        !first_chunk.is_empty(),
        "streamed public file by doi should produce bytes"
    );

    let public_download_path = dir.path().join("public-by-id.bin");
    let public_download = client
        .download_public_article_file_by_name_to_path(
            target.fetched.id,
            &target.file.name,
            true,
            &public_download_path,
        )
        .await
        .expect("download public article file by id");
    assert!(
        public_download.bytes_written > 0,
        "public id-based download should write bytes"
    );
    assert_eq!(
        public_download.resolved_file_id, target.file.id,
        "public id-based download should resolve the selected file"
    );

    let doi_download_path = dir.path().join("public-by-doi.bin");
    let doi_download = client
        .download_article_file_by_doi_to_path(
            &target.doi,
            &target.file.name,
            true,
            &doi_download_path,
        )
        .await
        .expect("download public article file by doi");
    assert!(
        doi_download.bytes_written > 0,
        "public doi-based download should write bytes"
    );
    assert_eq!(
        doi_download.resolved_file_id, target.file.id,
        "public doi-based download should resolve the selected file"
    );

    let downloaded_by_id = std::fs::read(&public_download_path).expect("read public id download");
    let downloaded_by_doi = std::fs::read(&doi_download_path).expect("read public doi download");
    assert_eq!(
        downloaded_by_id, downloaded_by_doi,
        "public downloads by id and doi should match"
    );
    assert!(
        !downloaded_by_id.is_empty(),
        "downloaded public file should not be empty"
    );
}
