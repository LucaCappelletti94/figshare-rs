#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::needless_raw_string_hashes,
    clippy::panic,
    clippy::too_many_lines,
    clippy::unwrap_used
)]

mod mock_support;

use std::io::Cursor;

use axum::http::{Method, StatusCode};
use figshare_rs::client_uploader_traits::prelude::*;
use figshare_rs::{
    ArticleId, ArticleMetadata, ArticleOrder, ArticleQuery, DefinedType, Doi, FigshareError,
    FileReplacePolicy, UploadSpec,
};
use serde_json::json;
use tempfile::tempdir;

use crate::mock_support::{MockFigshareServer, QueuedResponse};

fn public_article_json(server: &MockFigshareServer, id: u64, version: u64) -> serde_json::Value {
    json!({
        "id": id,
        "title": format!("Article {id}"),
        "doi": format!("10.6084/m9.figshare.{id}"),
        "defined_type": 3,
        "is_public": true,
        "version": version,
        "url_public_api": server.api_url(&format!("articles/{id}")),
        "files": [{
            "id": id * 10,
            "name": "artifact.bin",
            "size": 5,
            "download_url": server.raw_url(&format!("files/{id}/artifact.bin"))
        }]
    })
}

fn private_article_json(
    server: &MockFigshareServer,
    id: u64,
    is_public: bool,
) -> serde_json::Value {
    json!({
        "id": id,
        "title": format!("Private {id}"),
        "defined_type": "dataset",
        "status": if is_public { "public" } else { "draft" },
        "is_public": is_public,
        "url_private_api": server.api_url(&format!("account/articles/{id}")),
        "files": []
    })
}

fn private_file_json(
    server: &MockFigshareServer,
    article_id: u64,
    file_id: u64,
) -> serde_json::Value {
    json!({
        "id": file_id,
        "name": "artifact.bin",
        "size": 6,
        "status": "created",
        "is_link_only": false,
        "download_url": server.raw_url(&format!("files/{article_id}/artifact.bin")),
        "upload_url": server.raw_url(&format!("upload/token-{file_id}")),
        "upload_token": format!("token-{file_id}"),
        "supplied_md5": "e80b5017098950fc58aad83c8c14978e"
    })
}

fn public_file_json(
    server: &MockFigshareServer,
    article_id: u64,
    file_id: u64,
) -> serde_json::Value {
    json!({
        "id": file_id,
        "name": "artifact.bin",
        "size": 5,
        "download_url": server.raw_url(&format!("files/{article_id}/artifact.bin"))
    })
}

fn article_metadata() -> ArticleMetadata {
    ArticleMetadata::builder()
        .title("Example dataset")
        .defined_type(DefinedType::Dataset)
        .description("Example description")
        .author_named("Doe, Jane")
        .tag("data")
        .license_id(1)
        .build()
        .expect("valid metadata")
}

#[tokio::test]
async fn public_article_methods_use_expected_routes_and_payloads() {
    let server = MockFigshareServer::start().await;
    let client = server.anonymous_client();

    server.enqueue_json(
        Method::GET,
        "/v2/articles",
        StatusCode::OK,
        json!([{
            "id": 1,
            "title": "One",
            "defined_type": 3
        }]),
    );
    server.enqueue_json(
        Method::POST,
        "/v2/articles/search",
        StatusCode::OK,
        json!([public_article_json(&server, 2, 1)]),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/articles/2",
        StatusCode::OK,
        public_article_json(&server, 2, 1),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/articles/2/versions",
        StatusCode::OK,
        json!([
            { "version": 1, "url": server.api_url("articles/2/versions/1") },
            { "version": 3, "url": server.api_url("articles/2/versions/3") }
        ]),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/articles/2/versions/3",
        StatusCode::OK,
        public_article_json(&server, 2, 3),
    );

    let listed = client
        .list_public_articles(
            &ArticleQuery::builder()
                .order(ArticleOrder::PublishedDate)
                .page(2)
                .page_size(25)
                .item_type(DefinedType::Dataset)
                .build(),
        )
        .await
        .expect("list public");
    assert_eq!(listed[0].id, ArticleId(1));

    let searched = client
        .search_public_resources(
            &ArticleQuery::builder()
                .item_type(DefinedType::Dataset)
                .limit(10)
                .build(),
        )
        .await
        .expect("search public");
    assert_eq!(searched[0].id, ArticleId(2));

    let latest = client
        .resolve_latest_public_resource(&ArticleId(2))
        .await
        .expect("latest article");
    assert_eq!(latest.version_number(), Some(3));

    let requests = server.requests();
    assert!(requests[0]
        .query
        .as_deref()
        .unwrap_or_default()
        .contains("item_type=3"));
    let search_body: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(search_body["item_type"], 3);
    assert_eq!(search_body["limit"], 10);
}

#[tokio::test]
async fn doi_lookup_uses_exact_public_search() {
    let server = MockFigshareServer::start().await;
    let client = server.anonymous_client();

    server.enqueue_json(
        Method::GET,
        "/v2/articles",
        StatusCode::OK,
        json!([
            {
                "id": 10,
                "title": "Wrong",
                "doi": "10.6084/m9.figshare.other"
            },
            public_article_json(&server, 11, 1)
        ]),
    );

    let doi = Doi::new("10.6084/m9.figshare.11").unwrap();
    let article = client
        .get_public_resource_by_doi(&doi)
        .await
        .expect("article by doi");
    assert_eq!(article.id, ArticleId(11));

    let requests = server.requests();
    assert!(requests[0]
        .query
        .as_deref()
        .unwrap_or_default()
        .contains("doi=10.6084%2Fm9.figshare.11"));
}

#[tokio::test]
async fn private_article_methods_follow_location_and_send_auth_header() {
    let server = MockFigshareServer::start().await;
    let client = server.client();

    server.enqueue_json(
        Method::POST,
        "/v2/account/articles",
        StatusCode::CREATED,
        json!({ "location": server.api_url("account/articles/5") }),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/5",
        StatusCode::OK,
        private_article_json(&server, 5, false),
    );
    server.enqueue_text(
        Method::PUT,
        "/v2/account/articles/5",
        StatusCode::RESET_CONTENT,
        "",
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/5",
        StatusCode::OK,
        private_article_json(&server, 5, false),
    );
    server.enqueue_json(
        Method::POST,
        "/v2/account/articles/5/reserve_doi",
        StatusCode::OK,
        json!({ "doi": "10.6084/m9.figshare.5" }),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/categories",
        StatusCode::OK,
        json!([{
            "id": 3,
            "title": "Allowed category"
        }]),
    );
    server.enqueue_text(
        Method::DELETE,
        "/v2/account/articles/5",
        StatusCode::NO_CONTENT,
        "",
    );

    let article = client
        .create_draft(&article_metadata())
        .await
        .expect("create article");
    assert_eq!(article.id, ArticleId(5));
    client
        .update_draft_metadata(&ArticleId(5), &article_metadata())
        .await
        .expect("update article");
    let doi = client.reserve_doi(ArticleId(5)).await.expect("reserve doi");
    assert_eq!(doi.as_str(), "10.6084/m9.figshare.5");
    let categories = client
        .list_account_categories()
        .await
        .expect("list account categories");
    assert_eq!(categories[0].id, figshare_rs::CategoryId(3));
    client
        .delete_article(ArticleId(5))
        .await
        .expect("delete article");

    let requests = server.requests();
    assert_eq!(
        requests[0].headers.get("authorization").map(String::as_str),
        Some("token test-token")
    );
    let create_body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(create_body["title"], "Example dataset");
    assert_eq!(create_body["defined_type"], "dataset");
}

#[tokio::test]
async fn publish_waits_for_visibility_changes() {
    let server = MockFigshareServer::start().await;
    let client = server.client();

    server.enqueue_json(
        Method::POST,
        "/v2/account/articles/6/publish",
        StatusCode::CREATED,
        json!({ "location": server.api_url("articles/6") }),
    );
    server.enqueue_text(
        Method::GET,
        "/v2/articles/6",
        StatusCode::NOT_FOUND,
        "missing",
    );
    server.enqueue_json(
        Method::GET,
        "/v2/articles/6",
        StatusCode::OK,
        public_article_json(&server, 6, 1),
    );
    let public = client
        .publish_draft(&ArticleId(6))
        .await
        .expect("publish article");
    assert!(public.is_public_article());
}

#[tokio::test]
async fn file_management_endpoints_cover_listing_reading_deleting_and_link_files() {
    let server = MockFigshareServer::start().await;
    let client = server.client();

    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/8/files",
        StatusCode::OK,
        json!([private_file_json(&server, 8, 80)]),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/8/files/80",
        StatusCode::OK,
        private_file_json(&server, 8, 80),
    );
    server.enqueue_text(
        Method::DELETE,
        "/v2/account/articles/8/files/80",
        StatusCode::NO_CONTENT,
        "",
    );
    server.enqueue_json(
        Method::POST,
        "/v2/account/articles/8/files",
        StatusCode::CREATED,
        json!({ "location": server.api_url("account/articles/8/files/81") }),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/8/files/81",
        StatusCode::OK,
        json!({
            "id": 81,
            "name": "link.bin",
            "size": 0,
            "is_link_only": true,
            "download_url": "https://example.com/external.bin"
        }),
    );

    let files = client.list_files(ArticleId(8)).await.expect("list files");
    assert_eq!(files[0].id.0, 80);
    client
        .get_file(ArticleId(8), figshare_rs::FileId(80))
        .await
        .expect("get file");
    client
        .delete_file(ArticleId(8), figshare_rs::FileId(80))
        .await
        .expect("delete file");
    let linked = client
        .initiate_link_file(ArticleId(8), "https://example.com/external.bin")
        .await
        .expect("link file");
    assert_eq!(linked.id.0, 81);

    let requests = server.requests();
    let link_body: serde_json::Value = serde_json::from_slice(&requests[3].body).unwrap();
    assert_eq!(link_body["link"], "https://example.com/external.bin");
}

#[tokio::test]
async fn upload_path_and_reader_use_upload_service_routes() {
    let server = MockFigshareServer::start().await;
    let client = server.client();
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("artifact.bin");
    std::fs::write(&path, b"abcdef").expect("write temp file");

    server.enqueue_json(
        Method::POST,
        "/v2/account/articles/9/files",
        StatusCode::CREATED,
        json!({ "location": server.api_url("account/articles/9/files/90") }),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/9/files/90",
        StatusCode::OK,
        private_file_json(&server, 9, 90),
    );
    server.enqueue_json(
        Method::GET,
        "/upload/token-90",
        StatusCode::OK,
        json!({
            "token": "token-90",
            "name": "artifact.bin",
            "size": 6,
            "md5": "e80b5017098950fc58aad83c8c14978e",
            "status": "PENDING",
            "parts": [
                { "partNo": 1, "startOffset": 0, "endOffset": 2, "status": "PENDING", "locked": false },
                { "partNo": 2, "startOffset": 3, "endOffset": 5, "status": "PENDING", "locked": false }
            ]
        }),
    );
    server.enqueue_text(Method::PUT, "/upload/token-90/1", StatusCode::OK, "");
    server.enqueue_text(Method::PUT, "/upload/token-90/2", StatusCode::OK, "");
    server.enqueue_text(
        Method::POST,
        "/v2/account/articles/9/files/90",
        StatusCode::ACCEPTED,
        "",
    );
    server.enqueue_json(
        Method::GET,
        "/upload/token-90",
        StatusCode::OK,
        json!({
            "token": "token-90",
            "name": "artifact.bin",
            "size": 6,
            "status": "COMPLETED",
            "parts": []
        }),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/9/files/90",
        StatusCode::OK,
        private_file_json(&server, 9, 90),
    );

    let uploaded = client
        .upload_path(ArticleId(9), &path)
        .await
        .expect("upload path");
    assert_eq!(uploaded.id.0, 90);

    server.enqueue_json(
        Method::POST,
        "/v2/account/articles/9/files",
        StatusCode::CREATED,
        json!({ "location": server.api_url("account/articles/9/files/91") }),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/9/files/91",
        StatusCode::OK,
        json!({
            "id": 91,
            "name": "reader.bin",
            "size": 5,
            "status": "created",
            "is_link_only": false,
            "upload_url": server.raw_url("upload/token-91")
        }),
    );
    server.enqueue_json(
        Method::GET,
        "/upload/token-91",
        StatusCode::OK,
        json!({
            "token": "token-91",
            "name": "reader.bin",
            "size": 5,
            "status": "PENDING",
            "parts": [
                { "partNo": 1, "startOffset": 0, "endOffset": 4, "status": "PENDING", "locked": false }
            ]
        }),
    );
    server.enqueue_text(Method::PUT, "/upload/token-91/1", StatusCode::OK, "");
    server.enqueue_text(
        Method::POST,
        "/v2/account/articles/9/files/91",
        StatusCode::ACCEPTED,
        "",
    );
    server.enqueue_json(
        Method::GET,
        "/upload/token-91",
        StatusCode::OK,
        json!({
            "token": "token-91",
            "name": "reader.bin",
            "size": 5,
            "status": "COMPLETED",
            "parts": []
        }),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/9/files/91",
        StatusCode::OK,
        json!({
            "id": 91,
            "name": "reader.bin",
            "size": 5,
            "status": "available",
            "is_link_only": false
        }),
    );

    let uploaded_reader = client
        .upload_reader(
            ArticleId(9),
            "reader.bin",
            Cursor::new(b"12345".to_vec()),
            5,
        )
        .await
        .expect("upload reader");
    assert_eq!(uploaded_reader.id.0, 91);

    let requests = server.requests();
    let part_bodies: Vec<_> = requests
        .iter()
        .filter(|request| request.path.starts_with("/upload/token-"))
        .filter(|request| request.method == Method::PUT)
        .map(|request| request.body.clone())
        .collect();
    assert_eq!(
        part_bodies,
        vec![b"abc".to_vec(), b"def".to_vec(), b"12345".to_vec()]
    );
}

#[tokio::test]
async fn download_helpers_use_dedicated_file_list_endpoints() {
    let server = MockFigshareServer::start().await;
    let public_client = server.anonymous_client();
    let private_client = server.client();
    let dir = tempdir().expect("tempdir");

    server.enqueue_json(
        Method::GET,
        "/v2/articles/12",
        StatusCode::OK,
        json!({
            "id": 12,
            "title": "Article 12",
            "doi": "10.6084/m9.figshare.12",
            "defined_type": 3,
            "is_public": true,
            "version": 1,
            "url_public_api": server.api_url("articles/12"),
            "files": []
        }),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/articles/12/versions/1/files",
        StatusCode::OK,
        json!([public_file_json(&server, 12, 120)]),
    );
    server.enqueue(
        Method::GET,
        "/files/12/artifact.bin",
        QueuedResponse::bytes(StatusCode::OK, vec![], b"hello".to_vec()),
    );

    let public_path = dir.path().join("public.bin");
    let public_download = public_client
        .download_public_article_file_by_name_to_path(
            ArticleId(12),
            "artifact.bin",
            false,
            &public_path,
        )
        .await
        .expect("public download");
    assert_eq!(public_download.bytes_written, 5);
    assert_eq!(std::fs::read(&public_path).unwrap(), b"hello");

    server.enqueue_json(
        Method::GET,
        "/v2/articles",
        StatusCode::OK,
        json!([{
            "id": 13,
            "title": "Article 13",
            "doi": "10.6084/m9.figshare.13",
            "defined_type": 3,
            "is_public": true,
            "version": 1,
            "url_public_api": server.api_url("articles/13"),
            "files": []
        }]),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/articles/13/versions/1/files",
        StatusCode::OK,
        json!([public_file_json(&server, 13, 130)]),
    );
    server.enqueue(
        Method::GET,
        "/files/13/artifact.bin",
        QueuedResponse::bytes(StatusCode::OK, vec![], b"world".to_vec()),
    );

    let doi_path = dir.path().join("doi.bin");
    let doi = Doi::new("10.6084/m9.figshare.13").unwrap();
    private_client
        .download_article_file_by_doi_to_path(&doi, "artifact.bin", false, &doi_path)
        .await
        .expect("doi download");
    assert_eq!(std::fs::read(&doi_path).unwrap(), b"world");

    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/14/files",
        StatusCode::OK,
        json!([{
            "id": 140,
            "name": "artifact.bin",
            "size": 5,
            "download_url": server.raw_url("files/140")
        }]),
    );
    server.enqueue(
        Method::GET,
        "/files/140",
        QueuedResponse::bytes(StatusCode::OK, vec![], b"owner".to_vec()),
    );

    let own_path = dir.path().join("own.bin");
    private_client
        .download_own_article_file_by_name_to_path(ArticleId(14), "artifact.bin", &own_path)
        .await
        .expect("own download");
    assert_eq!(std::fs::read(&own_path).unwrap(), b"owner");

    let requests = server.requests();
    let private_download = requests
        .iter()
        .find(|request| request.path == "/files/140")
        .expect("private download request");
    assert_eq!(private_download.query.as_deref(), Some("token=test-token"));
    assert!(requests
        .iter()
        .any(|request| request.path == "/v2/articles/12/versions/1/files"));
    assert!(requests
        .iter()
        .any(|request| request.path == "/v2/articles/13/versions/1/files"));
    assert!(requests
        .iter()
        .any(|request| request.path == "/v2/account/articles/14/files"));
}

#[tokio::test]
async fn workflow_helpers_cover_reconcile_and_publish() {
    let server = MockFigshareServer::start().await;
    let client = server.client();
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("artifact.bin");
    std::fs::write(&path, b"abcdef").expect("write temp file");

    server.enqueue_text(
        Method::PUT,
        "/v2/account/articles/20",
        StatusCode::RESET_CONTENT,
        "",
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/20",
        StatusCode::OK,
        private_article_json(&server, 20, false),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/20/files",
        StatusCode::OK,
        json!([]),
    );
    server.enqueue_json(
        Method::POST,
        "/v2/account/articles/20/files",
        StatusCode::CREATED,
        json!({ "location": server.api_url("account/articles/20/files/200") }),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/20/files/200",
        StatusCode::OK,
        private_file_json(&server, 20, 200),
    );
    server.enqueue_json(
        Method::GET,
        "/upload/token-200",
        StatusCode::OK,
        json!({
            "token": "token-200",
            "name": "artifact.bin",
            "size": 6,
            "status": "PENDING",
            "parts": [
                { "partNo": 1, "startOffset": 0, "endOffset": 5, "status": "PENDING", "locked": false }
            ]
        }),
    );
    server.enqueue_text(Method::PUT, "/upload/token-200/1", StatusCode::OK, "");
    server.enqueue_text(
        Method::POST,
        "/v2/account/articles/20/files/200",
        StatusCode::ACCEPTED,
        "",
    );
    server.enqueue_json(
        Method::GET,
        "/upload/token-200",
        StatusCode::OK,
        json!({
            "token": "token-200",
            "name": "artifact.bin",
            "size": 6,
            "status": "COMPLETED",
            "parts": []
        }),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/20/files/200",
        StatusCode::OK,
        private_file_json(&server, 20, 200),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/20/files",
        StatusCode::OK,
        json!([private_file_json(&server, 20, 200)]),
    );
    server.enqueue_json(
        Method::POST,
        "/v2/account/articles/20/publish",
        StatusCode::CREATED,
        json!({ "location": server.api_url("articles/20") }),
    );
    server.enqueue_text(
        Method::GET,
        "/v2/articles/20",
        StatusCode::NOT_FOUND,
        "missing",
    );
    server.enqueue_json(
        Method::GET,
        "/v2/articles/20",
        StatusCode::OK,
        public_article_json(&server, 20, 1),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/20",
        StatusCode::OK,
        private_article_json(&server, 20, false),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/20",
        StatusCode::OK,
        private_article_json(&server, 20, true),
    );

    let published = client
        .update_publication(UpdatePublicationRequest::new(
            ArticleId(20),
            article_metadata(),
            FileReplacePolicy::ReplaceAll,
            vec![UploadSpec::from_path(&path).unwrap()],
        ))
        .await
        .expect("publish workflow");
    assert!(published.article.is_public_article());
    assert_eq!(published.public_article.id, ArticleId(20));
}

#[tokio::test]
async fn reconcile_files_rejects_conflicts_for_keep_existing_policy() {
    let server = MockFigshareServer::start().await;
    let client = server.client();
    let article = figshare_rs::Article {
        id: ArticleId(21),
        title: "Example".into(),
        ..serde_json::from_value(json!({
            "id": 21,
            "title": "Example"
        }))
        .unwrap()
    };
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/21/files",
        StatusCode::OK,
        json!([{
            "id": 210,
            "name": "artifact.bin",
            "size": 1
        }]),
    );

    let error = client
        .reconcile_draft_files(
            &article,
            FileReplacePolicy::KeepExistingAndAdd,
            vec![UploadSpec::from_reader(
                "artifact.bin",
                Cursor::new(vec![1]),
                1,
            )],
        )
        .await
        .expect_err("conflict expected");
    assert!(matches!(
        error,
        FigshareError::ConflictingDraftFile { filename } if filename == "artifact.bin"
    ));
}

#[tokio::test]
async fn reconcile_files_does_not_delete_existing_files_before_successful_replacement() {
    let server = MockFigshareServer::start().await;
    let client = server.client();
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("artifact.bin");
    std::fs::write(&path, b"abcdef").expect("write temp file");
    let article = figshare_rs::Article {
        id: ArticleId(30),
        title: "Example".into(),
        ..serde_json::from_value(json!({
            "id": 30,
            "title": "Example"
        }))
        .unwrap()
    };

    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/30/files",
        StatusCode::OK,
        json!([{
            "id": 300,
            "name": "existing.bin",
            "size": 1
        }]),
    );
    server.enqueue_json(
        Method::POST,
        "/v2/account/articles/30/files",
        StatusCode::CREATED,
        json!({ "location": server.api_url("account/articles/30/files/301") }),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/30/files/301",
        StatusCode::OK,
        private_file_json(&server, 30, 301),
    );
    server.enqueue_json(
        Method::GET,
        "/upload/token-301",
        StatusCode::OK,
        json!({
            "token": "token-301",
            "name": "artifact.bin",
            "size": 6,
            "status": "PENDING",
            "parts": [
                { "partNo": 1, "startOffset": 0, "endOffset": 5, "status": "PENDING", "locked": false }
            ]
        }),
    );
    server.enqueue_text(
        Method::PUT,
        "/upload/token-301/1",
        StatusCode::INTERNAL_SERVER_ERROR,
        "upload failed",
    );
    server.enqueue_text(
        Method::DELETE,
        "/v2/account/articles/30/files/301",
        StatusCode::NO_CONTENT,
        "",
    );

    let error = client
        .reconcile_draft_files(
            &article,
            FileReplacePolicy::ReplaceAll,
            vec![UploadSpec::from_path(&path).unwrap()],
        )
        .await
        .expect_err("upload failure expected");
    assert!(matches!(error, FigshareError::Http { .. }));

    let requests = server.requests();
    assert!(!requests.iter().any(|request| {
        request.method == Method::DELETE && request.path == "/v2/account/articles/30/files/300"
    }));
    assert!(requests.iter().any(|request| {
        request.method == Method::DELETE && request.path == "/v2/account/articles/30/files/301"
    }));
}

#[tokio::test]
async fn create_and_publish_article_deletes_new_draft_after_upload_failure() {
    let server = MockFigshareServer::start().await;
    let client = server.client();
    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("artifact.bin");
    std::fs::write(&path, b"abcdef").expect("write temp file");

    server.enqueue_json(
        Method::POST,
        "/v2/account/articles",
        StatusCode::CREATED,
        json!({ "location": server.api_url("account/articles/40") }),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/40",
        StatusCode::OK,
        private_article_json(&server, 40, false),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/40/files",
        StatusCode::OK,
        json!([]),
    );
    server.enqueue_json(
        Method::POST,
        "/v2/account/articles/40/files",
        StatusCode::CREATED,
        json!({ "location": server.api_url("account/articles/40/files/400") }),
    );
    server.enqueue_json(
        Method::GET,
        "/v2/account/articles/40/files/400",
        StatusCode::OK,
        private_file_json(&server, 40, 400),
    );
    server.enqueue_json(
        Method::GET,
        "/upload/token-400",
        StatusCode::OK,
        json!({
            "token": "token-400",
            "name": "artifact.bin",
            "size": 6,
            "status": "PENDING",
            "parts": [
                { "partNo": 1, "startOffset": 0, "endOffset": 5, "status": "PENDING", "locked": false }
            ]
        }),
    );
    server.enqueue_text(
        Method::PUT,
        "/upload/token-400/1",
        StatusCode::INTERNAL_SERVER_ERROR,
        "upload failed",
    );
    server.enqueue_text(
        Method::DELETE,
        "/v2/account/articles/40/files/400",
        StatusCode::NO_CONTENT,
        "",
    );
    server.enqueue_text(
        Method::DELETE,
        "/v2/account/articles/40",
        StatusCode::NO_CONTENT,
        "",
    );

    let error = client
        .create_publication(CreatePublicationRequest::untargeted(
            article_metadata(),
            vec![UploadSpec::from_path(&path).unwrap()],
        ))
        .await
        .expect_err("upload failure expected");
    assert!(matches!(error, FigshareError::Http { .. }));

    let requests = server.requests();
    assert!(requests.iter().any(|request| {
        request.method == Method::DELETE && request.path == "/v2/account/articles/40"
    }));
}
