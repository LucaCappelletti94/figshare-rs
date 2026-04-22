//! Higher-level workflow helpers built on top of the low-level client.

use std::collections::BTreeSet;

use crate::client::FigshareClient;
use crate::error::FigshareError;
use crate::ids::ArticleId;
use crate::metadata::ArticleMetadata;
use crate::model::{Article, ArticleFile};
use crate::upload::{FileReplacePolicy, UploadSource, UploadSpec};

/// Result of a complete publish workflow.
#[derive(Clone, Debug, PartialEq)]
pub struct PublishedArticle {
    /// Private article payload after publication.
    pub article: Article,
    /// Published public article payload.
    pub public_article: Article,
}

impl FigshareClient {
    /// Reconciles article files using one of the provided replacement policies.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if an upload conflicts
    /// with the selected policy, if the request fails, or if Figshare returns a
    /// non-success response.
    pub(crate) async fn reconcile_files(
        &self,
        article: &Article,
        policy: FileReplacePolicy,
        uploads: Vec<UploadSpec>,
    ) -> Result<Vec<ArticleFile>, FigshareError> {
        let upload_filenames = validate_upload_filenames(&uploads)?;
        let existing = self.list_files(article.id).await?;

        match policy {
            FileReplacePolicy::KeepExistingAndAdd => {
                for upload in &uploads {
                    if existing.iter().any(|file| file.name == upload.filename) {
                        return Err(FigshareError::ConflictingDraftFile {
                            filename: upload.filename.clone(),
                        });
                    }
                }
            }
            FileReplacePolicy::ReplaceAll | FileReplacePolicy::UpsertByFilename => {}
        }

        let mut uploaded = Vec::new();
        for upload in uploads {
            let result = match upload.source {
                UploadSource::Path(path) => {
                    self.upload_path_with_filename(article.id, &upload.filename, &path)
                        .await
                }
                UploadSource::Reader {
                    reader,
                    content_length,
                } => {
                    self.upload_reader(article.id, &upload.filename, reader, content_length)
                        .await
                }
            };

            match result {
                Ok(file) => uploaded.push(file),
                Err(error) => {
                    self.cleanup_uploaded_files(article.id, &uploaded).await;
                    return Err(error);
                }
            }
        }

        match policy {
            FileReplacePolicy::ReplaceAll => {
                for file in &existing {
                    self.delete_file(article.id, file.id).await?;
                }
            }
            FileReplacePolicy::UpsertByFilename => {
                for file in existing
                    .iter()
                    .filter(|file| upload_filenames.contains(&file.name))
                {
                    self.delete_file(article.id, file.id).await?;
                }
            }
            FileReplacePolicy::KeepExistingAndAdd => {}
        }

        self.list_files(article.id).await
    }

    /// Creates a new private article, uploads the provided files, and publishes
    /// the result.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if uploading or
    /// publication fails, or if Figshare returns a non-success response.
    pub(crate) async fn create_and_publish_article(
        &self,
        metadata: &ArticleMetadata,
        uploads: Vec<UploadSpec>,
    ) -> Result<PublishedArticle, FigshareError> {
        let article = self.create_article(metadata).await?;
        if let Err(error) = self
            .reconcile_files(&article, FileReplacePolicy::ReplaceAll, uploads)
            .await
        {
            let _ = self.delete_article(article.id).await;
            return Err(error);
        }

        let public_article = match self.publish_article(article.id).await {
            Ok(public_article) => public_article,
            Err(error) => {
                let _ = self.delete_article(article.id).await;
                return Err(error);
            }
        };
        let article = self.wait_for_own_article_public(article.id).await?;

        Ok(PublishedArticle {
            article,
            public_article,
        })
    }

    /// Updates an existing private article, reconciles its files, and publishes
    /// a new public version.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub(crate) async fn publish_existing_article_with_policy(
        &self,
        article_id: ArticleId,
        metadata: &ArticleMetadata,
        policy: FileReplacePolicy,
        uploads: Vec<UploadSpec>,
    ) -> Result<PublishedArticle, FigshareError> {
        let article = self.update_article(article_id, metadata).await?;
        self.reconcile_files(&article, policy, uploads).await?;
        let public_article = self.publish_article(article_id).await?;
        let article = self.wait_for_own_article_public(article_id).await?;

        Ok(PublishedArticle {
            article,
            public_article,
        })
    }
}

impl FigshareClient {
    async fn cleanup_uploaded_files(&self, article_id: ArticleId, uploaded: &[ArticleFile]) {
        for file in uploaded {
            let _ = self.delete_file(article_id, file.id).await;
        }
    }
}

fn validate_upload_filenames(uploads: &[UploadSpec]) -> Result<BTreeSet<String>, FigshareError> {
    client_uploader_traits::collect_upload_filenames(uploads.iter()).map_err(FigshareError::from)
}

#[cfg(test)]
mod tests {
    use super::validate_upload_filenames;
    use crate::{upload::UploadSpec, FigshareError};

    #[test]
    fn duplicate_filenames_are_rejected() {
        let uploads = vec![
            UploadSpec::from_reader("artifact.bin", std::io::Cursor::new(vec![1]), 1),
            UploadSpec::from_reader("artifact.bin", std::io::Cursor::new(vec![2]), 1),
        ];
        assert!(matches!(
            validate_upload_filenames(&uploads),
            Err(FigshareError::DuplicateUploadFilename { .. })
        ));
    }

    #[test]
    fn empty_filenames_are_rejected() {
        let uploads = vec![UploadSpec::from_reader(
            "",
            std::io::Cursor::new(vec![1]),
            1,
        )];

        assert!(matches!(
            validate_upload_filenames(&uploads),
            Err(FigshareError::InvalidState(message)) if message == "upload filename cannot be empty"
        ));
    }
}
