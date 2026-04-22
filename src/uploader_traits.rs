use std::path::Path;
use std::time::Duration;

use client_uploader_traits::{
    ClientContext, CreatePublication, CreatePublicationRequest, DoiBackedRecord,
    DownloadNamedPublicFile, DraftFilePolicy, DraftFilePolicyKind, DraftResource, DraftState,
    DraftWorkflow, ListResourceFiles, LookupByDoi, MaybeAuthenticatedClient,
    MutablePublicationOutcome, NoCreateTarget, PublicationOutcome, ReadPublicResource,
    RepositoryFile, RepositoryRecord, ResolveLatestPublicResource,
    ResolveLatestPublicResourceByDoi, SearchPublicResources, UpdatePublication,
    UpdatePublicationRequest, UploadSourceKind, UploadSpecLike,
};

use crate::{
    Article, ArticleFile, ArticleId, ArticleMetadata, ArticleQuery, Doi, Endpoint, FigshareClient,
    FigshareError, FileReplacePolicy, PollOptions, PublishedArticle, ResolvedDownload,
    UploadSource, UploadSpec,
};

impl ClientContext for FigshareClient {
    type Endpoint = Endpoint;
    type PollOptions = PollOptions;
    type Error = FigshareError;

    fn endpoint(&self) -> &Self::Endpoint {
        FigshareClient::endpoint(self)
    }

    fn poll_options(&self) -> &Self::PollOptions {
        FigshareClient::poll_options(self)
    }

    fn request_timeout(&self) -> Option<Duration> {
        FigshareClient::request_timeout(self)
    }

    fn connect_timeout(&self) -> Option<Duration> {
        FigshareClient::connect_timeout(self)
    }
}

impl MaybeAuthenticatedClient for FigshareClient {
    fn has_auth(&self) -> bool {
        !self.auth.is_anonymous()
    }
}

impl UploadSpecLike for UploadSpec {
    fn filename(&self) -> &str {
        &self.filename
    }

    fn source_kind(&self) -> UploadSourceKind {
        match self.source {
            UploadSource::Path(_) => UploadSourceKind::Path,
            UploadSource::Reader { .. } => UploadSourceKind::Reader,
        }
    }

    fn content_length(&self) -> Option<u64> {
        match self.source {
            UploadSource::Path(_) => None,
            UploadSource::Reader { content_length, .. } => Some(content_length),
        }
    }
}

impl DraftFilePolicy for FileReplacePolicy {
    fn kind(&self) -> DraftFilePolicyKind {
        match self {
            Self::ReplaceAll => DraftFilePolicyKind::ReplaceAll,
            Self::UpsertByFilename => DraftFilePolicyKind::UpsertByFilename,
            Self::KeepExistingAndAdd => DraftFilePolicyKind::KeepExistingAndAdd,
        }
    }
}

impl RepositoryFile for ArticleFile {
    type Id = crate::FileId;

    fn file_id(&self) -> Option<Self::Id> {
        Some(self.id)
    }

    fn file_name(&self) -> &str {
        &self.name
    }

    fn size_bytes(&self) -> Option<u64> {
        Some(self.size)
    }

    fn checksum(&self) -> Option<&str> {
        self.computed_md5
            .as_deref()
            .or(self.supplied_md5.as_deref())
    }

    fn download_url(&self) -> Option<&url::Url> {
        self.download_url.as_ref()
    }
}

impl RepositoryRecord for Article {
    type Id = ArticleId;
    type File = ArticleFile;

    fn resource_id(&self) -> Option<Self::Id> {
        Some(self.id)
    }

    fn title(&self) -> Option<&str> {
        Some(&self.title)
    }

    fn files(&self) -> &[Self::File] {
        self.files.as_slice()
    }
}

impl DoiBackedRecord for Article {
    type Doi = Doi;

    fn doi(&self) -> Option<Self::Doi> {
        self.doi.clone()
    }
}

impl DraftResource for Article {
    type Id = ArticleId;
    type File = ArticleFile;

    fn draft_id(&self) -> Self::Id {
        self.id
    }

    fn files(&self) -> &[Self::File] {
        self.files.as_slice()
    }
}

impl DraftState for Article {
    fn is_published(&self) -> bool {
        self.is_public_article()
    }

    fn allows_metadata_updates(&self) -> bool {
        !self.is_public_article()
    }
}

impl PublicationOutcome for Article {
    type PublicResource = Article;

    fn public_resource(&self) -> &Self::PublicResource {
        self
    }
}

impl PublicationOutcome for PublishedArticle {
    type PublicResource = Article;

    fn public_resource(&self) -> &Self::PublicResource {
        &self.public_article
    }
}

impl MutablePublicationOutcome for PublishedArticle {
    type MutableResource = Article;

    fn mutable_resource(&self) -> Option<&Self::MutableResource> {
        Some(&self.article)
    }
}

impl ReadPublicResource for FigshareClient {
    type ResourceId = ArticleId;
    type Resource = Article;

    async fn get_public_resource(
        &self,
        id: &Self::ResourceId,
    ) -> Result<Self::Resource, Self::Error> {
        self.get_public_article(*id).await
    }
}

impl SearchPublicResources for FigshareClient {
    type Query = ArticleQuery;
    type SearchResults = Vec<Article>;

    async fn search_public_resources(
        &self,
        query: &Self::Query,
    ) -> Result<Self::SearchResults, Self::Error> {
        self.search_public_articles(query).await
    }
}

impl ListResourceFiles for FigshareClient {
    type ResourceId = ArticleId;
    type File = ArticleFile;

    async fn list_resource_files(
        &self,
        id: &Self::ResourceId,
    ) -> Result<Vec<Self::File>, Self::Error> {
        let article = self.get_public_article(*id).await?;
        let version = public_article_version_number(self, &article).await?;
        self.list_public_article_version_files(article.id, version)
            .await
    }
}

impl DownloadNamedPublicFile for FigshareClient {
    type ResourceId = ArticleId;
    type Download = ResolvedDownload;

    async fn download_named_public_file_to_path(
        &self,
        id: &Self::ResourceId,
        name: &str,
        path: &Path,
    ) -> Result<Self::Download, Self::Error> {
        self.download_public_article_file_by_name_to_path(*id, name, false, path)
            .await
    }
}

impl CreatePublication for FigshareClient {
    type CreateTarget = NoCreateTarget;
    type Metadata = ArticleMetadata;
    type Upload = UploadSpec;
    type Output = PublishedArticle;

    async fn create_publication(
        &self,
        request: CreatePublicationRequest<Self::CreateTarget, Self::Metadata, Self::Upload>,
    ) -> Result<Self::Output, Self::Error> {
        let CreatePublicationRequest {
            target: _,
            metadata,
            uploads,
        } = request;
        self.create_and_publish_article(&metadata, uploads).await
    }
}

impl UpdatePublication for FigshareClient {
    type ResourceId = ArticleId;
    type Metadata = ArticleMetadata;
    type FilePolicy = FileReplacePolicy;
    type Upload = UploadSpec;
    type Output = PublishedArticle;

    async fn update_publication(
        &self,
        request: UpdatePublicationRequest<
            Self::ResourceId,
            Self::Metadata,
            Self::FilePolicy,
            Self::Upload,
        >,
    ) -> Result<Self::Output, Self::Error> {
        let UpdatePublicationRequest {
            resource_id,
            metadata,
            policy,
            uploads,
        } = request;
        self.publish_existing_article_with_policy(resource_id, &metadata, policy, uploads)
            .await
    }
}

impl LookupByDoi for FigshareClient {
    type Doi = Doi;
    type Resource = Article;

    async fn get_public_resource_by_doi(
        &self,
        doi: &Self::Doi,
    ) -> Result<Self::Resource, Self::Error> {
        self.get_public_article_by_doi(doi).await
    }
}

impl ResolveLatestPublicResource for FigshareClient {
    type ResourceId = ArticleId;
    type Resource = Article;

    async fn resolve_latest_public_resource(
        &self,
        id: &Self::ResourceId,
    ) -> Result<Self::Resource, Self::Error> {
        self.resolve_latest_public_article(*id).await
    }
}

impl ResolveLatestPublicResourceByDoi for FigshareClient {
    type Doi = Doi;
    type Resource = Article;

    async fn resolve_latest_public_resource_by_doi(
        &self,
        doi: &Self::Doi,
    ) -> Result<Self::Resource, Self::Error> {
        self.resolve_latest_public_article_by_doi(doi).await
    }
}

impl DraftWorkflow for FigshareClient {
    type Draft = Article;
    type Metadata = ArticleMetadata;
    type Upload = UploadSpec;
    type FilePolicy = FileReplacePolicy;
    type UploadResult = ArticleFile;
    type Published = Article;

    async fn create_draft(&self, metadata: &Self::Metadata) -> Result<Self::Draft, Self::Error> {
        self.create_article(metadata).await
    }

    async fn update_draft_metadata(
        &self,
        draft_id: &<Self::Draft as DraftResource>::Id,
        metadata: &Self::Metadata,
    ) -> Result<Self::Draft, Self::Error> {
        self.update_article(*draft_id, metadata).await
    }

    async fn reconcile_draft_files(
        &self,
        draft: &Self::Draft,
        policy: Self::FilePolicy,
        uploads: Vec<Self::Upload>,
    ) -> Result<Vec<Self::UploadResult>, Self::Error> {
        self.reconcile_files(draft, policy, uploads).await
    }

    async fn publish_draft(
        &self,
        draft_id: &<Self::Draft as DraftResource>::Id,
    ) -> Result<Self::Published, Self::Error> {
        self.publish_article(*draft_id).await
    }
}

async fn public_article_version_number(
    client: &FigshareClient,
    article: &Article,
) -> Result<u64, FigshareError> {
    if let Some(version) = article.version_number() {
        return Ok(version);
    }

    Ok(client
        .list_public_article_versions(article.id)
        .await?
        .into_iter()
        .map(|version| version.version)
        .max()
        .unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use client_uploader_traits::{
        CoreRepositoryClient, DoiBackedRecord, DoiVersionedRepositoryClient, DraftFilePolicy,
        DraftFilePolicyKind, DraftPublishingRepositoryClient, DraftResource, DraftState,
        MutablePublicationOutcome, PublicationOutcome, RepositoryFile, RepositoryRecord,
        UploadSourceKind, UploadSpecLike,
    };
    use serde_json::json;

    use crate::{Auth, FileId};

    use super::*;

    fn assert_core_repository_client<C>()
    where
        C: CoreRepositoryClient,
    {
    }

    fn assert_doi_versioned_repository_client<C>()
    where
        C: DoiVersionedRepositoryClient,
    {
    }

    fn assert_draft_publishing_repository_client<C>()
    where
        C: DraftPublishingRepositoryClient,
    {
    }

    fn assert_publication_outcome<T>()
    where
        T: PublicationOutcome,
    {
    }

    #[test]
    fn figshare_client_satisfies_repository_client_bundles() {
        assert_core_repository_client::<FigshareClient>();
        assert_doi_versioned_repository_client::<FigshareClient>();
        assert_draft_publishing_repository_client::<FigshareClient>();
        assert_publication_outcome::<Article>();
        assert_publication_outcome::<PublishedArticle>();
    }

    #[test]
    fn client_context_and_auth_traits_expose_config() {
        let endpoint = Endpoint::Custom("http://localhost:8080/v2/".parse().unwrap());
        let poll = PollOptions {
            max_wait: Duration::from_secs(9),
            initial_delay: Duration::from_millis(50),
            max_delay: Duration::from_secs(2),
        };
        let client = FigshareClient::builder(Auth::new("secret-token"))
            .endpoint(endpoint.clone())
            .poll_options(poll.clone())
            .request_timeout(Duration::from_secs(11))
            .connect_timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let anonymous = FigshareClient::builder(Auth::anonymous()).build().unwrap();

        assert_eq!(ClientContext::endpoint(&client), &endpoint);
        assert_eq!(ClientContext::poll_options(&client), &poll);
        assert_eq!(
            ClientContext::request_timeout(&client),
            Some(Duration::from_secs(11))
        );
        assert_eq!(
            ClientContext::connect_timeout(&client),
            Some(Duration::from_secs(3))
        );
        assert!(MaybeAuthenticatedClient::has_auth(&client));
        assert!(!MaybeAuthenticatedClient::has_auth(&anonymous));
    }

    #[test]
    fn upload_spec_like_reports_source_metadata() {
        let path_upload = UploadSpec::from_path("/tmp/archive.tar.gz").unwrap();
        let reader_upload =
            UploadSpec::from_reader("artifact.bin", std::io::Cursor::new(vec![1, 2, 3]), 3);

        assert_eq!(UploadSpecLike::filename(&path_upload), "archive.tar.gz");
        assert_eq!(
            UploadSpecLike::source_kind(&path_upload),
            UploadSourceKind::Path
        );
        assert_eq!(UploadSpecLike::content_length(&path_upload), None);
        assert_eq!(UploadSpecLike::content_type(&path_upload), None);

        assert_eq!(UploadSpecLike::filename(&reader_upload), "artifact.bin");
        assert_eq!(
            UploadSpecLike::source_kind(&reader_upload),
            UploadSourceKind::Reader
        );
        assert_eq!(UploadSpecLike::content_length(&reader_upload), Some(3));
    }

    #[test]
    fn file_replace_policy_maps_to_shared_kind() {
        assert_eq!(
            DraftFilePolicy::kind(&FileReplacePolicy::ReplaceAll),
            DraftFilePolicyKind::ReplaceAll
        );
        assert_eq!(
            DraftFilePolicy::kind(&FileReplacePolicy::UpsertByFilename),
            DraftFilePolicyKind::UpsertByFilename
        );
        assert_eq!(
            DraftFilePolicy::kind(&FileReplacePolicy::KeepExistingAndAdd),
            DraftFilePolicyKind::KeepExistingAndAdd
        );
    }

    #[test]
    fn article_and_file_models_implement_shared_inspection_traits() {
        let draft: Article = serde_json::from_value(json!({
            "id": 42,
            "title": "Draft dataset",
            "doi": "10.6084/m9.figshare.42",
            "status": "draft",
            "is_public": false,
            "files": [{
                "id": 7,
                "name": "artifact.bin",
                "size": 12,
                "computed_md5": "abc123",
                "download_url": "https://example.com/file"
            }]
        }))
        .unwrap();
        let published: Article = serde_json::from_value(json!({
            "id": 42,
            "title": "Draft dataset",
            "status": "public",
            "is_public": true
        }))
        .unwrap();
        let file = &draft.files[0];

        assert_eq!(RepositoryRecord::resource_id(&draft), Some(ArticleId(42)));
        assert_eq!(RepositoryRecord::title(&draft), Some("Draft dataset"));
        assert_eq!(RepositoryRecord::files(&draft).len(), 1);
        assert_eq!(
            DoiBackedRecord::doi(&draft),
            Some(Doi::new("10.6084/m9.figshare.42").unwrap())
        );
        assert_eq!(DraftResource::draft_id(&draft), ArticleId(42));
        assert_eq!(DraftResource::files(&draft).len(), 1);
        assert!(!DraftState::is_published(&draft));
        assert!(DraftState::allows_metadata_updates(&draft));
        assert!(DraftState::is_published(&published));
        assert!(!DraftState::allows_metadata_updates(&published));

        assert_eq!(RepositoryFile::file_id(file), Some(FileId(7)));
        assert_eq!(RepositoryFile::file_name(file), "artifact.bin");
        assert_eq!(RepositoryFile::size_bytes(file), Some(12));
        assert_eq!(RepositoryFile::checksum(file), Some("abc123"));
        assert_eq!(
            RepositoryFile::download_url(file).map(url::Url::as_str),
            Some("https://example.com/file")
        );
    }

    #[test]
    fn published_article_implements_shared_outcome_traits() {
        let article: Article = serde_json::from_value(json!({
            "id": 42,
            "title": "Private article",
            "status": "draft"
        }))
        .unwrap();
        let public_article: Article = serde_json::from_value(json!({
            "id": 42,
            "title": "Public article",
            "status": "public",
            "is_public": true
        }))
        .unwrap();
        let outcome = PublishedArticle {
            article: article.clone(),
            public_article: public_article.clone(),
        };

        assert_eq!(
            PublicationOutcome::public_resource(&outcome),
            &public_article
        );
        assert_eq!(PublicationOutcome::created(&outcome), None);
        assert_eq!(
            MutablePublicationOutcome::mutable_resource(&outcome),
            Some(&article)
        );
    }

    #[test]
    fn article_implements_publication_outcome() {
        let article: Article = serde_json::from_value(json!({
            "id": 42,
            "title": "Public article",
            "status": "public",
            "is_public": true
        }))
        .unwrap();

        assert_eq!(PublicationOutcome::public_resource(&article), &article);
        assert_eq!(PublicationOutcome::created(&article), None);
    }
}
