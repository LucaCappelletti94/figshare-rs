#![allow(
    missing_docs,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::unwrap_used
)]

use std::error::Error;
use std::io::Cursor;
use std::time::{SystemTime, UNIX_EPOCH};

use figshare_rs::{
    ArticleId, ArticleMetadata, ArticleQuery, DefinedType, FigshareClient, FigshareError,
    FileReplacePolicy, UploadPart, UploadSpec, UploadStatus,
};
use futures_util::StreamExt;
use md5::{Digest, Md5};
use reqwest::StatusCode;
use tempfile::tempdir;
use tokio::time::{sleep, Duration, Instant};

const SEARCH_WAIT: Duration = Duration::from_secs(20);
const UPLOAD_WAIT: Duration = Duration::from_secs(30);
const POLL_DELAY: Duration = Duration::from_secs(2);
const UNPUBLISH_REASON: &str = "CI cleanup after live smoke publication";

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_millis()
}

fn live_metadata(
    title: impl Into<String>,
    description: impl Into<String>,
    tag: impl Into<String>,
) -> Result<ArticleMetadata, figshare_rs::ArticleMetadataBuildError> {
    ArticleMetadata::builder()
        .title(title)
        .defined_type(DefinedType::Dataset)
        .description(description)
        .author_named("figshare-rs CI")
        .tag("figshare-rs")
        .tag(tag)
        .build()
}

fn md5_hex(bytes: &[u8]) -> String {
    let mut digest = Md5::new();
    digest.update(bytes);
    hex::encode(digest.finalize())
}

fn part_bytes(bytes: &[u8], part: &UploadPart) -> Vec<u8> {
    let start = usize::try_from(part.start_offset).expect("part start offset should fit in usize");
    let end = usize::try_from(part.end_offset + 1).expect("part end offset should fit in usize");
    bytes[start..end].to_vec()
}

async fn cleanup_article(
    client: &FigshareClient,
    article_id: ArticleId,
) -> Result<(), Box<dyn Error>> {
    let article = match client.get_own_article(article_id).await {
        Ok(article) => article,
        Err(FigshareError::Http { status, .. }) if status == StatusCode::NOT_FOUND => {
            return Ok(());
        }
        Err(error) => return Err(Box::new(error)),
    };

    if article.is_public_article() {
        client
            .unpublish_article(article_id, UNPUBLISH_REASON)
            .await?;
    }

    if let Ok(files) = client.list_files(article_id).await {
        for file in files {
            let _ = client.delete_file(article_id, file.id).await;
        }
    }

    match client.delete_article(article_id).await {
        Ok(()) => Ok(()),
        Err(FigshareError::Http { status, .. }) if status == StatusCode::NOT_FOUND => Ok(()),
        Err(error) => Err(Box::new(error)),
    }
}

async fn wait_for_upload_completion(
    client: &FigshareClient,
    upload_url: &url::Url,
) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + UPLOAD_WAIT;

    loop {
        let session = client.get_upload_session(upload_url).await?;
        if matches!(session.status, UploadStatus::Completed) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err("timed out waiting for upload session completion".into());
        }
        sleep(POLL_DELAY).await;
    }
}

async fn cleanup_articles(
    client: &FigshareClient,
    article_ids: &[ArticleId],
) -> Result<(), Box<dyn Error>> {
    for &article_id in article_ids.iter().rev() {
        cleanup_article(client, article_id).await?;
    }
    Ok(())
}

async fn wait_for_own_search_hit(
    client: &FigshareClient,
    search_for: &str,
    article_id: ArticleId,
) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + SEARCH_WAIT;

    loop {
        let hits = client
            .search_own_articles(
                &ArticleQuery::builder()
                    .search_for(search_for)
                    .limit(10)
                    .build(),
            )
            .await?;
        if hits.iter().any(|article| article.id == article_id) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "timed out waiting for search_own_articles to find draft article {article_id}"
            )
            .into());
        }
        sleep(POLL_DELAY).await;
    }
}

#[tokio::test]
#[ignore = "requires FIGSHARE_TOKEN and network access; mutates authenticated account state"]
async fn authenticated_account_surface_round_trip() {
    let client = FigshareClient::from_env().expect("build authenticated client");
    let dir = tempdir().expect("tempdir");
    let payload_path = dir.path().join("payload.txt");
    let payload = b"figshare-rs live smoke payload\n";
    let manual_payload = b"figshare-rs manual upload payload\n";
    std::fs::write(&payload_path, payload).expect("write payload");

    let suffix = unique_suffix();
    let initial_title = format!("figshare-rs draft smoke {suffix}");
    let initial_metadata = live_metadata(
        initial_title.clone(),
        "Draft-to-public live smoke test for figshare-rs",
        format!("draft-run-{suffix}"),
    )
    .expect("metadata");

    let article = client
        .create_article(&initial_metadata)
        .await
        .expect("create article");
    let article_id = article.id;

    let result = async {
        let own_articles = client
            .list_own_articles(&ArticleQuery::builder().limit(25).build())
            .await?;
        assert!(
            own_articles.iter().any(|article| article.id == article_id),
            "new draft should appear in own article listing"
        );

        wait_for_own_search_hit(&client, &initial_title, article_id).await?;

        let draft = client.get_own_article(article_id).await?;
        assert!(
            !draft.is_public_article(),
            "freshly created article should remain private"
        );
        assert_eq!(draft.title, initial_title, "draft title should round-trip");

        let updated_title = format!("figshare-rs updated draft smoke {suffix}");
        let updated_metadata = live_metadata(
            updated_title.clone(),
            "Updated live smoke test for figshare-rs",
            format!("draft-run-{suffix}"),
        )?;
        let updated = client.update_article(article_id, &updated_metadata).await?;
        assert_eq!(
            updated.title, updated_title,
            "update_article should persist"
        );
        assert_eq!(
            updated.description.as_deref(),
            Some("Updated live smoke test for figshare-rs"),
            "updated description should round-trip"
        );

        let link_file = client
            .initiate_link_file(article_id, "https://figshare.com/")
            .await?;

        let uploaded = client.upload_path(article_id, &payload_path).await?;
        assert_eq!(uploaded.name, "payload.txt");

        let uploaded_reader = client
            .upload_reader(
                article_id,
                "reader.txt",
                std::io::Cursor::new(payload.to_vec()),
                payload.len() as u64,
            )
            .await?;
        assert_eq!(uploaded_reader.name, "reader.txt");

        let fetched_uploaded = client.get_file(article_id, uploaded.id).await?;
        assert_eq!(fetched_uploaded.name, "payload.txt");

        let files = client.list_files(article_id).await?;
        assert!(
            files.iter().any(|file| file.id == uploaded.id),
            "uploaded file should be listed on the draft article"
        );
        assert!(
            files.iter().any(|file| file.id == uploaded_reader.id),
            "reader upload should be listed on the draft article"
        );
        assert!(
            files.iter().any(|file| file.id == link_file.id),
            "link-only file should be listed on the draft article"
        );

        let mut opened = client
            .open_own_article_file_by_name(article_id, "reader.txt")
            .await?;
        let first_chunk = opened
            .stream
            .next()
            .await
            .expect("first chunk from private download stream")?;
        assert!(
            !first_chunk.is_empty(),
            "private streamed download should produce bytes"
        );

        let downloaded = dir.path().join("downloaded.txt");
        let resolved = client
            .download_own_article_file_by_name_to_path(article_id, "payload.txt", &downloaded)
            .await?;
        assert!(resolved.bytes_written > 0, "download should write bytes");
        assert_eq!(std::fs::read(&downloaded)?, payload);

        client.delete_file(article_id, link_file.id).await?;
        let files = client.list_files(article_id).await?;
        assert!(
            files.iter().all(|file| file.id != link_file.id),
            "deleted link-only file should no longer be listed"
        );

        let reserved_doi = client.reserve_doi(article_id).await?;
        assert!(
            reserved_doi.as_str().starts_with("10."),
            "reserved DOI should look like a DOI"
        );

        let manual_file = client
            .initiate_file_upload(
                article_id,
                "manual.txt",
                manual_payload.len() as u64,
                &md5_hex(manual_payload),
            )
            .await?;
        let upload_url = manual_file
            .upload_session_url()
            .cloned()
            .expect("manual upload session url");
        let session = client.get_upload_session(&upload_url).await?;
        assert!(
            !session.parts.is_empty(),
            "manual upload session should expose at least one part"
        );

        let mut reset_once = false;
        for part in &session.parts {
            let part_bytes = part_bytes(manual_payload, part);
            client
                .upload_part(&upload_url, part.part_no, part_bytes.clone())
                .await?;
            if !reset_once {
                client.reset_upload_part(&upload_url, part.part_no).await?;
                client
                    .upload_part(&upload_url, part.part_no, part_bytes)
                    .await?;
                reset_once = true;
            }
        }
        assert!(
            reset_once,
            "manual upload should have reset at least one uploaded part"
        );

        client
            .complete_file_upload(article_id, manual_file.id)
            .await?;
        wait_for_upload_completion(&client, &upload_url).await?;

        let fetched_manual = client.get_file(article_id, manual_file.id).await?;
        assert_eq!(fetched_manual.name, "manual.txt");

        let published = client.publish_article(article_id).await?;
        assert!(
            published.is_public_article(),
            "publish_article should publish"
        );
        assert_eq!(published.id, article_id);
        if let Some(doi) = &published.doi {
            assert_eq!(
                doi.as_str(),
                reserved_doi.as_str(),
                "published article should preserve the reserved DOI"
            );
        }

        let unpublished = client
            .unpublish_article(article_id, UNPUBLISH_REASON)
            .await?;
        assert!(
            !unpublished.is_public_article(),
            "unpublish_article should restore a private draft"
        );

        Ok::<(), Box<dyn Error>>(())
    }
    .await;

    let cleanup = cleanup_article(&client, article_id).await;

    if let Err(error) = cleanup {
        panic!("failed to clean up live smoke article {article_id}: {error}");
    }
    if let Err(error) = result {
        panic!("authenticated account surface workflow failed: {error}");
    }
}

#[tokio::test]
#[ignore = "requires FIGSHARE_TOKEN and network access; mutates authenticated account state"]
async fn authenticated_publish_workflow_helpers_round_trip() {
    let client = FigshareClient::from_env().expect("build authenticated client");
    let suffix = unique_suffix();
    let mut cleanup_ids = Vec::new();

    let result = async {
        let direct_article = client
            .create_article(&live_metadata(
                format!("figshare-rs reconcile smoke {suffix}"),
                "Direct reconcile_files live smoke test",
                format!("workflow-direct-{suffix}"),
            )?)
            .await?;
        cleanup_ids.push(direct_article.id);

        let reconciled_files = client
            .reconcile_files(
                &direct_article,
                FileReplacePolicy::KeepExistingAndAdd,
                vec![UploadSpec::from_reader(
                    "workflow.txt",
                    Cursor::new(b"first workflow payload".to_vec()),
                    22,
                )],
            )
            .await?;
        assert!(
            reconciled_files
                .iter()
                .any(|file| file.name == "workflow.txt"),
            "reconcile_files should upload the requested file"
        );

        let published_existing = client
            .publish_existing_article_with_policy(
                direct_article.id,
                &live_metadata(
                    format!("figshare-rs publish-existing smoke {suffix}"),
                    "publish_existing_article_with_policy live smoke test",
                    format!("workflow-update-{suffix}"),
                )?,
                FileReplacePolicy::UpsertByFilename,
                vec![UploadSpec::from_reader(
                    "workflow.txt",
                    Cursor::new(b"second workflow payload".to_vec()),
                    23,
                )],
            )
            .await?;
        assert!(
            published_existing.article.is_public_article(),
            "publish_existing_article_with_policy should publish the private article"
        );
        assert_eq!(
            published_existing.article.id, direct_article.id,
            "publish_existing_article_with_policy should preserve the article id"
        );
        assert_eq!(
            published_existing.public_article.id, direct_article.id,
            "publish_existing_article_with_policy should return the public article"
        );

        let created = client
            .create_and_publish_article(
                &live_metadata(
                    format!("figshare-rs create-publish smoke {}", suffix + 1),
                    "create_and_publish_article live smoke test",
                    format!("workflow-create-{}", suffix + 1),
                )?,
                vec![UploadSpec::from_reader(
                    "created.txt",
                    Cursor::new(b"create and publish payload".to_vec()),
                    26,
                )],
            )
            .await?;
        cleanup_ids.push(created.article.id);
        assert!(
            created.article.is_public_article(),
            "create_and_publish_article should publish the private article"
        );
        assert_eq!(
            created.article.id, created.public_article.id,
            "create_and_publish_article should preserve the article id"
        );

        Ok::<(), Box<dyn Error>>(())
    }
    .await;

    let cleanup = cleanup_articles(&client, &cleanup_ids).await;

    if let Err(error) = cleanup {
        panic!("failed to clean up published workflow smoke articles: {error}");
    }
    if let Err(error) = result {
        panic!("authenticated workflow helpers smoke failed: {error}");
    }
}
