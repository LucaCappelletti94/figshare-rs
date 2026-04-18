#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::unwrap_used
)]

use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use figshare_rs::{ArticleId, ArticleMetadata, DefinedType, FigshareClient};
use tempfile::tempdir;

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_millis()
}

async fn cleanup_draft_article(
    client: &FigshareClient,
    article_id: ArticleId,
) -> Result<(), Box<dyn Error>> {
    if let Ok(files) = client.list_files(article_id).await {
        for file in files {
            let _ = client.delete_file(article_id, file.id).await;
        }
    }
    client.delete_article(article_id).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires FIGSHARE_TOKEN and network access; mutates draft account state"]
async fn authenticated_draft_workflow_round_trip() {
    let client = FigshareClient::from_env().expect("build authenticated client");
    let dir = tempdir().expect("tempdir");
    let payload_path = dir.path().join("payload.txt");
    let payload = b"figshare-rs live smoke payload\n";
    std::fs::write(&payload_path, payload).expect("write payload");

    let suffix = unique_suffix();
    let metadata = ArticleMetadata::builder()
        .title(format!("figshare-rs draft smoke {suffix}"))
        .defined_type(DefinedType::Dataset)
        .description("Draft-only live smoke test for figshare-rs")
        .author_named("figshare-rs CI")
        .tag("figshare-rs")
        .tag(format!("draft-run-{suffix}"))
        .build()
        .expect("metadata");

    let article = client
        .create_article(&metadata)
        .await
        .expect("create article");
    let article_id = article.id;

    let result = async {
        let draft = client.get_own_article(article_id).await?;
        assert!(
            !draft.is_public_article(),
            "freshly created article should remain private"
        );

        let uploaded = client.upload_path(article_id, &payload_path).await?;
        assert_eq!(uploaded.name, "payload.txt");

        let files = client.list_files(article_id).await?;
        assert!(
            files.iter().any(|file| file.id == uploaded.id),
            "uploaded file should be listed on the draft article"
        );

        let downloaded = dir.path().join("downloaded.txt");
        let resolved = client
            .download_own_article_file_by_name_to_path(article_id, "payload.txt", &downloaded)
            .await?;
        assert!(resolved.bytes_written > 0, "download should write bytes");
        assert_eq!(std::fs::read(&downloaded)?, payload);

        Ok::<(), Box<dyn Error>>(())
    }
    .await;

    let cleanup = cleanup_draft_article(&client, article_id).await;

    if let Err(error) = cleanup {
        panic!("failed to clean up live smoke draft article {article_id}: {error}");
    }
    if let Err(error) = result {
        panic!("authenticated draft workflow failed: {error}");
    }
}
