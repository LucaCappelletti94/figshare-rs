//! Streaming and file download helpers for Figshare article files.

use std::path::Path;
use std::pin::Pin;

use bytes::Bytes;
use futures_core::Stream;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use url::Url;

use crate::client::FigshareClient;
use crate::error::FigshareError;
use crate::ids::{ArticleId, Doi, FileId};
use crate::model::{Article, ArticleFile};

/// Resolved information about a completed or opened download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedDownload {
    /// Article that ultimately supplied the file.
    pub resolved_article: ArticleId,
    /// File identifier selected for download.
    pub resolved_file_id: FileId,
    /// File name selected for download.
    pub resolved_name: String,
    /// Final download URL.
    pub download_url: Url,
    /// Number of bytes written when downloading to disk.
    pub bytes_written: u64,
}

/// Streaming download handle.
pub struct DownloadStream {
    /// Resolved download metadata.
    pub resolved: ResolvedDownload,
    /// Reported content length, when present.
    pub content_length: Option<u64>,
    /// Reported content type, when present.
    pub content_type: Option<String>,
    /// Reported content disposition, when present.
    pub content_disposition: Option<String>,
    /// Byte stream for the response body.
    pub stream: Pin<Box<dyn Stream<Item = Result<Bytes, reqwest::Error>> + Send>>,
}

impl std::fmt::Debug for DownloadStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadStream")
            .field("resolved", &self.resolved)
            .field("content_length", &self.content_length)
            .field("content_type", &self.content_type)
            .field("content_disposition", &self.content_disposition)
            .finish_non_exhaustive()
    }
}

impl FigshareClient {
    /// Opens a file from a public article by exact file name.
    ///
    /// When `latest` is `true`, the latest public article version is resolved
    /// before selecting the file.
    ///
    /// # Errors
    ///
    /// Returns an error if the article or file cannot be resolved, if the
    /// request fails, or if Figshare returns a non-success response.
    pub async fn open_public_article_file_by_name(
        &self,
        article_id: ArticleId,
        name: &str,
        latest: bool,
    ) -> Result<DownloadStream, FigshareError> {
        let article = if latest {
            self.resolve_latest_public_article(article_id).await?
        } else {
            self.get_public_article(article_id).await?
        };
        let version = self.resolve_public_article_version_number(&article).await?;
        let file = self
            .find_public_article_file_by_name(article.id, version, name)
            .await?;
        self.open_download_for_file(article.id, file, false).await
    }

    /// Opens a file from a public article selected by DOI.
    ///
    /// When `latest` is `true`, the latest public article version is resolved
    /// before selecting the file.
    ///
    /// # Errors
    ///
    /// Returns an error if the article or file cannot be resolved, if the
    /// request fails, or if Figshare returns a non-success response.
    pub async fn open_article_file_by_doi(
        &self,
        doi: &Doi,
        name: &str,
        latest: bool,
    ) -> Result<DownloadStream, FigshareError> {
        let article = if latest {
            self.resolve_latest_public_article_by_doi(doi).await?
        } else {
            self.get_public_article_by_doi(doi).await?
        };
        let version = self.resolve_public_article_version_number(&article).await?;
        let file = self
            .find_public_article_file_by_name(article.id, version, name)
            .await?;
        self.open_download_for_file(article.id, file, false).await
    }

    /// Opens a file from one of the authenticated account's own articles by
    /// exact file name.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the article or file
    /// cannot be resolved, if the request fails, or if Figshare returns a
    /// non-success response.
    pub async fn open_own_article_file_by_name(
        &self,
        article_id: ArticleId,
        name: &str,
    ) -> Result<DownloadStream, FigshareError> {
        let file = self.find_own_article_file_by_name(article_id, name).await?;
        self.open_download_for_file(article_id, file, true).await
    }

    /// Downloads a file from a public article by exact file name.
    ///
    /// # Errors
    ///
    /// Returns an error if the article or file cannot be resolved, if the
    /// request fails, or if writing the destination path fails.
    pub async fn download_public_article_file_by_name_to_path(
        &self,
        article_id: ArticleId,
        name: &str,
        latest: bool,
        path: &Path,
    ) -> Result<ResolvedDownload, FigshareError> {
        let stream = self
            .open_public_article_file_by_name(article_id, name, latest)
            .await?;
        write_stream_to_path(stream, path).await
    }

    /// Downloads a file from a public article resolved by DOI.
    ///
    /// # Errors
    ///
    /// Returns an error if the article or file cannot be resolved, if the
    /// request fails, or if writing the destination path fails.
    pub async fn download_article_file_by_doi_to_path(
        &self,
        doi: &Doi,
        name: &str,
        latest: bool,
        path: &Path,
    ) -> Result<ResolvedDownload, FigshareError> {
        let stream = self.open_article_file_by_doi(doi, name, latest).await?;
        write_stream_to_path(stream, path).await
    }

    /// Downloads a file from one of the authenticated account's own articles by
    /// exact file name.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the article or file
    /// cannot be resolved, if the request fails, or if writing the destination
    /// path fails.
    pub async fn download_own_article_file_by_name_to_path(
        &self,
        article_id: ArticleId,
        name: &str,
        path: &Path,
    ) -> Result<ResolvedDownload, FigshareError> {
        let stream = self.open_own_article_file_by_name(article_id, name).await?;
        write_stream_to_path(stream, path).await
    }

    async fn resolve_public_article_version_number(
        &self,
        article: &Article,
    ) -> Result<u64, FigshareError> {
        if let Some(version) = article.version_number() {
            return Ok(version);
        }

        let versions = self.list_public_article_versions(article.id).await?;
        Ok(versions
            .iter()
            .map(|version| version.version)
            .max()
            .unwrap_or(1))
    }

    async fn find_public_article_file_by_name(
        &self,
        article_id: ArticleId,
        version: u64,
        name: &str,
    ) -> Result<ArticleFile, FigshareError> {
        self.list_public_article_version_files(article_id, version)
            .await?
            .into_iter()
            .find(|file| file.name == name)
            .ok_or_else(|| FigshareError::MissingFile {
                name: name.to_owned(),
            })
    }

    async fn find_own_article_file_by_name(
        &self,
        article_id: ArticleId,
        name: &str,
    ) -> Result<ArticleFile, FigshareError> {
        self.list_files(article_id)
            .await?
            .into_iter()
            .find(|file| file.name == name)
            .ok_or_else(|| FigshareError::MissingFile {
                name: name.to_owned(),
            })
    }

    pub(crate) async fn open_download_for_file(
        &self,
        article_id: ArticleId,
        file: ArticleFile,
        authenticated_download: bool,
    ) -> Result<DownloadStream, FigshareError> {
        let download_url = file
            .download_url
            .clone()
            .ok_or(FigshareError::MissingLink("download_url"))?;

        let response = self
            .execute_response(self.download_request_url(
                reqwest::Method::GET,
                download_url.clone(),
                authenticated_download,
            )?)
            .await?;

        let content_length = response.content_length();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let content_disposition = response
            .headers()
            .get(reqwest::header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        Ok(DownloadStream {
            resolved: ResolvedDownload {
                resolved_article: article_id,
                resolved_file_id: file.id,
                resolved_name: file.name,
                download_url,
                bytes_written: 0,
            },
            content_length,
            content_type,
            content_disposition,
            stream: Box::pin(response.bytes_stream()),
        })
    }
}

async fn write_stream_to_path(
    mut stream: DownloadStream,
    path: &Path,
) -> Result<ResolvedDownload, FigshareError> {
    let mut file = File::create(path).await?;
    let mut bytes_written = 0_u64;

    while let Some(chunk) = futures_util::StreamExt::next(&mut stream.stream).await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        bytes_written += chunk.len() as u64;
    }
    file.flush().await?;

    stream.resolved.bytes_written = bytes_written;
    Ok(stream.resolved)
}
