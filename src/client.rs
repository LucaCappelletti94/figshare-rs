//! Low-level typed Figshare client operations.
//!
//! Use this module when you want direct access to Figshare's article, file,
//! upload, and catalog endpoints without the higher-level orchestration from
//! [`crate::workflow`].

use std::cmp::min;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, Instant};

use md5::{Digest, Md5};
use reqwest::header::{ACCEPT, AUTHORIZATION, LOCATION};
use reqwest::{Method, RequestBuilder};
use secrecy::{ExposeSecret, SecretString};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tempfile::NamedTempFile;
use tokio::time::sleep;
use url::Url;

use crate::endpoint::Endpoint;
use crate::error::FigshareError;
use crate::ids::{ArticleId, Doi, FileId};
use crate::metadata::ArticleMetadata;
use crate::model::{
    Article, ArticleCategory, ArticleFile, ArticleLicense, ArticleVersion, UploadSession,
    UploadStatus,
};
use crate::poll::PollOptions;
use crate::query::ArticleQuery;

/// Token authentication for Figshare API requests.
#[derive(Clone)]
pub struct Auth {
    /// API token used for authenticated requests, or `None` for anonymous access.
    pub token: Option<SecretString>,
}

impl Auth {
    /// Standard environment variable for a Figshare API token.
    pub const TOKEN_ENV_VAR: &'static str = "FIGSHARE_TOKEN";

    /// Creates a new authentication wrapper from a raw token string.
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: Some(SecretString::from(token.into())),
        }
    }

    /// Creates an anonymous authentication wrapper for public API calls.
    ///
    /// # Examples
    ///
    /// ```
    /// use figshare_rs::Auth;
    ///
    /// let auth = Auth::anonymous();
    /// assert!(auth.is_anonymous());
    /// ```
    #[must_use]
    pub fn anonymous() -> Self {
        Self { token: None }
    }

    /// Reads a Figshare API token from [`Self::TOKEN_ENV_VAR`].
    ///
    /// # Errors
    ///
    /// Returns an error if the environment variable is missing or invalid.
    pub fn from_env() -> Result<Self, FigshareError> {
        Self::from_env_var(Self::TOKEN_ENV_VAR)
    }

    /// Reads a Figshare API token from a custom environment variable.
    ///
    /// # Errors
    ///
    /// Returns an error if the environment variable is missing or invalid.
    pub fn from_env_var(name: &str) -> Result<Self, FigshareError> {
        let token = std::env::var(name).map_err(|source| FigshareError::EnvVar {
            name: name.to_owned(),
            source,
        })?;
        Ok(Self::new(token))
    }

    /// Returns whether this authentication wrapper is anonymous.
    #[must_use]
    pub fn is_anonymous(&self) -> bool {
        self.token.is_none()
    }
}

impl From<SecretString> for Auth {
    fn from(token: SecretString) -> Self {
        Self { token: Some(token) }
    }
}

impl std::fmt::Debug for Auth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_anonymous() {
            f.debug_struct("Auth")
                .field("token", &"<anonymous>")
                .finish()
        } else {
            f.debug_struct("Auth")
                .field("token", &"<redacted>")
                .finish()
        }
    }
}

/// Builder for configuring a [`FigshareClient`].
#[derive(Clone, Debug)]
pub struct FigshareClientBuilder {
    auth: Auth,
    endpoint: Endpoint,
    poll: PollOptions,
    user_agent: Option<String>,
    request_timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
}

impl FigshareClientBuilder {
    /// Overrides the API endpoint used by the client.
    #[must_use]
    pub fn endpoint(mut self, endpoint: Endpoint) -> Self {
        self.endpoint = endpoint;
        self
    }

    /// Overrides the `User-Agent` header sent on each request.
    #[must_use]
    pub fn user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// Sets the overall HTTP request timeout used by the underlying client.
    #[must_use]
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Sets the TCP connect timeout used by the underlying client.
    #[must_use]
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// Overrides the polling policy used by upload and publish helpers.
    #[must_use]
    pub fn poll_options(mut self, poll: PollOptions) -> Self {
        self.poll = poll;
        self
    }

    /// Builds a configured [`FigshareClient`].
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying `reqwest` client cannot be built.
    pub fn build(self) -> Result<FigshareClient, FigshareError> {
        let user_agent = self
            .user_agent
            .unwrap_or_else(|| format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")));

        let mut inner = reqwest::Client::builder().user_agent(&user_agent);
        if let Some(timeout) = self.request_timeout {
            inner = inner.timeout(timeout);
        }
        if let Some(timeout) = self.connect_timeout {
            inner = inner.connect_timeout(timeout);
        }
        let inner = inner.build()?;

        Ok(FigshareClient {
            inner,
            auth: self.auth,
            endpoint: self.endpoint,
            poll: self.poll,
            request_timeout: self.request_timeout,
            connect_timeout: self.connect_timeout,
        })
    }
}

/// Typed async client for the core Figshare REST API.
#[derive(Clone, Debug)]
pub struct FigshareClient {
    pub(crate) inner: reqwest::Client,
    pub(crate) auth: Auth,
    pub(crate) endpoint: Endpoint,
    pub(crate) poll: PollOptions,
    pub(crate) request_timeout: Option<Duration>,
    pub(crate) connect_timeout: Option<Duration>,
}

impl FigshareClient {
    const MAX_PAGE_SIZE: u64 = 1_000;

    /// Starts building a new client from authentication settings.
    #[must_use]
    pub fn builder(auth: Auth) -> FigshareClientBuilder {
        FigshareClientBuilder {
            auth,
            endpoint: Endpoint::default(),
            poll: PollOptions::default(),
            user_agent: None,
            request_timeout: None,
            connect_timeout: None,
        }
    }

    /// Builds a client with default endpoint and polling options.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be initialized.
    pub fn new(auth: Auth) -> Result<Self, FigshareError> {
        Self::builder(auth).build()
    }

    /// Builds a client directly from a raw API token.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be initialized.
    pub fn with_token(token: impl Into<String>) -> Result<Self, FigshareError> {
        Self::new(Auth::new(token))
    }

    /// Builds an anonymous client for public API calls.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying HTTP client cannot be initialized.
    pub fn anonymous() -> Result<Self, FigshareError> {
        Self::new(Auth::anonymous())
    }

    /// Builds a client from [`Auth::TOKEN_ENV_VAR`].
    ///
    /// # Errors
    ///
    /// Returns an error if the environment variable is missing or invalid, or
    /// if the underlying HTTP client cannot be initialized.
    pub fn from_env() -> Result<Self, FigshareError> {
        Self::new(Auth::from_env()?)
    }

    /// Returns the configured API endpoint.
    #[must_use]
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// Returns the configured polling behavior.
    #[must_use]
    pub fn poll_options(&self) -> &PollOptions {
        &self.poll
    }

    /// Returns the configured overall HTTP request timeout.
    #[must_use]
    pub fn request_timeout(&self) -> Option<Duration> {
        self.request_timeout
    }

    /// Returns the configured TCP connect timeout.
    #[must_use]
    pub fn connect_timeout(&self) -> Option<Duration> {
        self.connect_timeout
    }

    pub(crate) fn request(
        &self,
        method: Method,
        path: &str,
        auth_required: bool,
    ) -> Result<RequestBuilder, FigshareError> {
        let url = self.endpoint.base_url()?.join(path)?;
        self.request_url(method, url, auth_required)
    }

    pub(crate) fn request_url(
        &self,
        method: Method,
        url: Url,
        auth_required: bool,
    ) -> Result<RequestBuilder, FigshareError> {
        if !self.is_trusted_api_url(&url)? {
            return Err(FigshareError::InvalidState(format!(
                "refusing API request to different origin: {url}"
            )));
        }

        let mut request = self
            .inner
            .request(method, url)
            .header(ACCEPT, "application/json");
        if auth_required {
            request = request.header(
                AUTHORIZATION,
                self.authorization_header_value("api request")?,
            );
        }

        Ok(request)
    }

    pub(crate) fn upload_request_url(
        &self,
        method: Method,
        url: Url,
    ) -> Result<RequestBuilder, FigshareError> {
        if !self.is_trusted_upload_url(&url)? {
            return Err(FigshareError::InvalidState(format!(
                "refusing upload request to different origin: {url}"
            )));
        }

        Ok(self.inner.request(method, url).header(
            AUTHORIZATION,
            self.authorization_header_value("upload request")?,
        ))
    }

    pub(crate) fn download_request_url(
        &self,
        method: Method,
        url: Url,
        auth_download: bool,
    ) -> Result<RequestBuilder, FigshareError> {
        let url = if auth_download {
            self.with_download_token(url, "private file download")?
        } else {
            url
        };
        Ok(self.inner.request(method, url))
    }

    pub(crate) async fn execute_json<T>(&self, request: RequestBuilder) -> Result<T, FigshareError>
    where
        T: DeserializeOwned,
    {
        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(FigshareError::from_response(response).await);
        }

        let bytes = response.bytes().await?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub(crate) async fn execute_unit(&self, request: RequestBuilder) -> Result<(), FigshareError> {
        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(FigshareError::from_response(response).await);
        }

        Ok(())
    }

    pub(crate) async fn execute_response(
        &self,
        request: RequestBuilder,
    ) -> Result<reqwest::Response, FigshareError> {
        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(FigshareError::from_response(response).await);
        }

        Ok(response)
    }

    async fn execute_location(&self, request: RequestBuilder) -> Result<Url, FigshareError> {
        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(FigshareError::from_response(response).await);
        }

        let response_url = response.url().clone();
        if let Some(location) = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
        {
            return parse_location(&response_url, location);
        }

        let bytes = response.bytes().await?;
        if bytes.is_empty() {
            return Err(FigshareError::InvalidState(
                "successful Figshare response did not include a location".into(),
            ));
        }

        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if let Some(location) = value.get("location").and_then(serde_json::Value::as_str) {
                return parse_location(&response_url, location);
            }
            if let Some(location) = value.as_str() {
                return parse_location(&response_url, location);
            }
        }

        let text = String::from_utf8_lossy(&bytes);
        parse_location(&response_url, text.trim())
    }

    fn is_trusted_api_url(&self, url: &Url) -> Result<bool, FigshareError> {
        Ok(self.endpoint.base_url()?.origin() == url.origin())
    }

    fn is_trusted_upload_url(&self, url: &Url) -> Result<bool, FigshareError> {
        let endpoint = self.endpoint.base_url()?;
        Ok(endpoint.origin() == url.origin()
            || url.host_str().is_some_and(is_trusted_figshare_upload_host))
    }

    fn authorization_header_value(&self, operation: &'static str) -> Result<String, FigshareError> {
        let token = self
            .auth
            .token
            .as_ref()
            .ok_or(FigshareError::MissingAuth(operation))?;
        Ok(format!("token {}", token.expose_secret()))
    }

    fn with_download_token(
        &self,
        mut url: Url,
        operation: &'static str,
    ) -> Result<Url, FigshareError> {
        let token = self
            .auth
            .token
            .as_ref()
            .ok_or(FigshareError::MissingAuth(operation))?;

        let should_append = url
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case("ndownloader.figshare.com"))
            || self.endpoint.base_url()?.origin() == url.origin();

        if should_append {
            url.query_pairs_mut()
                .append_pair("token", token.expose_secret());
        }

        Ok(url)
    }

    async fn get_public_article_by_url(&self, url: &Url) -> Result<Article, FigshareError> {
        self.execute_json(self.request_url(Method::GET, url.clone(), false)?)
            .await
    }

    async fn get_own_article_by_url(&self, url: &Url) -> Result<Article, FigshareError> {
        self.execute_json(self.request_url(Method::GET, url.clone(), true)?)
            .await
    }

    async fn get_file_by_url(&self, url: &Url) -> Result<ArticleFile, FigshareError> {
        self.execute_json(self.request_url(Method::GET, url.clone(), true)?)
            .await
    }

    /// Lists public licenses.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or Figshare returns a non-success
    /// response.
    pub async fn list_licenses(&self) -> Result<Vec<ArticleLicense>, FigshareError> {
        self.execute_json(self.request(Method::GET, "licenses", false)?)
            .await
    }

    /// Lists public categories.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or Figshare returns a non-success
    /// response.
    pub async fn list_categories(&self) -> Result<Vec<ArticleCategory>, FigshareError> {
        self.execute_json(self.request(Method::GET, "categories", false)?)
            .await
    }

    /// Lists categories available to the authenticated account.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn list_account_categories(&self) -> Result<Vec<ArticleCategory>, FigshareError> {
        self.execute_json(self.request(Method::GET, "account/categories", true)?)
            .await
    }

    /// Lists public articles.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or Figshare returns a non-success
    /// response.
    pub async fn list_public_articles(
        &self,
        query: &ArticleQuery,
    ) -> Result<Vec<Article>, FigshareError> {
        let pairs = query.as_public_list_query_pairs()?;
        self.execute_json(self.request(Method::GET, "articles", false)?.query(&pairs))
            .await
    }

    /// Searches public articles.
    ///
    /// # Errors
    ///
    /// Returns an error if the query is invalid, if the request fails, or if
    /// Figshare returns a non-success response.
    pub async fn search_public_articles(
        &self,
        query: &ArticleQuery,
    ) -> Result<Vec<Article>, FigshareError> {
        let body = query.as_public_search_body()?;
        self.execute_json(
            self.request(Method::POST, "articles/search", false)?
                .json(&body),
        )
        .await
    }

    /// Reads one public article by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or Figshare returns a non-success
    /// response.
    pub async fn get_public_article(&self, id: ArticleId) -> Result<Article, FigshareError> {
        self.execute_json(self.request(Method::GET, &format!("articles/{id}"), false)?)
            .await
    }

    /// Lists public versions for one article.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or Figshare returns a non-success
    /// response.
    pub async fn list_public_article_versions(
        &self,
        id: ArticleId,
    ) -> Result<Vec<ArticleVersion>, FigshareError> {
        self.execute_json(self.request(Method::GET, &format!("articles/{id}/versions"), false)?)
            .await
    }

    /// Reads one specific public article version.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or Figshare returns a non-success
    /// response.
    pub async fn get_public_article_version(
        &self,
        id: ArticleId,
        version: u64,
    ) -> Result<Article, FigshareError> {
        self.execute_json(self.request(
            Method::GET,
            &format!("articles/{id}/versions/{version}"),
            false,
        )?)
        .await
    }

    /// Resolves a public article by exact DOI match.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails, Figshare returns a non-success
    /// response, or no exact DOI match is found.
    pub async fn get_public_article_by_doi(&self, doi: &Doi) -> Result<Article, FigshareError> {
        let hits = self
            .list_public_articles(&ArticleQuery::builder().doi(doi.as_str()).limit(10).build())
            .await?;
        hits.into_iter()
            .find(|article| article.doi.as_ref() == Some(doi))
            .ok_or_else(|| {
                FigshareError::UnsupportedSelector(format!(
                    "no public article matched DOI {}",
                    doi.as_str()
                ))
            })
    }

    /// Resolves the latest public article version for a given article ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or Figshare returns a non-success
    /// response.
    pub async fn resolve_latest_public_article(
        &self,
        id: ArticleId,
    ) -> Result<Article, FigshareError> {
        let article = self.get_public_article(id).await?;
        let versions = self.list_public_article_versions(id).await?;
        let Some(latest) = versions.iter().max_by_key(|version| version.version) else {
            return Ok(article);
        };

        if article.version_number() == Some(latest.version) {
            return Ok(article);
        }

        self.get_public_article_version(id, latest.version).await
    }

    /// Resolves the latest public article version for a DOI.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or no matching article exists.
    pub async fn resolve_latest_public_article_by_doi(
        &self,
        doi: &Doi,
    ) -> Result<Article, FigshareError> {
        let article = self.get_public_article_by_doi(doi).await?;
        self.resolve_latest_public_article(article.id).await
    }

    /// Lists the authenticated account's own articles.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn list_own_articles(
        &self,
        query: &ArticleQuery,
    ) -> Result<Vec<Article>, FigshareError> {
        let pairs = query.as_own_list_query_pairs()?;
        self.execute_json(
            self.request(Method::GET, "account/articles", true)?
                .query(&pairs),
        )
        .await
    }

    /// Searches the authenticated account's own articles.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the query is invalid,
    /// if the request fails, or if Figshare returns a non-success response.
    pub async fn search_own_articles(
        &self,
        query: &ArticleQuery,
    ) -> Result<Vec<Article>, FigshareError> {
        let body = query.as_own_search_body()?;
        self.execute_json(
            self.request(Method::POST, "account/articles/search", true)?
                .json(&body),
        )
        .await
    }

    /// Reads one private article owned by the authenticated account.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn get_own_article(&self, id: ArticleId) -> Result<Article, FigshareError> {
        self.execute_json(self.request(Method::GET, &format!("account/articles/{id}"), true)?)
            .await
    }

    /// Creates a new private article.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn create_article(
        &self,
        metadata: &ArticleMetadata,
    ) -> Result<Article, FigshareError> {
        let location = self
            .execute_location(
                self.request(Method::POST, "account/articles", true)?
                    .json(&metadata.to_payload()),
            )
            .await?;
        self.get_own_article_by_url(&location).await
    }

    /// Updates a private article in place.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn update_article(
        &self,
        id: ArticleId,
        metadata: &ArticleMetadata,
    ) -> Result<Article, FigshareError> {
        self.execute_unit(
            self.request(Method::PUT, &format!("account/articles/{id}"), true)?
                .json(&metadata.to_payload()),
        )
        .await?;
        self.get_own_article(id).await
    }

    /// Deletes a private article.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn delete_article(&self, id: ArticleId) -> Result<(), FigshareError> {
        self.execute_unit(self.request(Method::DELETE, &format!("account/articles/{id}"), true)?)
            .await
    }

    /// Reserves a DOI for a private article.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn reserve_doi(&self, id: ArticleId) -> Result<Doi, FigshareError> {
        #[derive(Deserialize)]
        struct Payload {
            doi: Doi,
        }

        let payload: Payload = self
            .execute_json(self.request(
                Method::POST,
                &format!("account/articles/{id}/reserve_doi"),
                true,
            )?)
            .await?;
        Ok(payload.doi)
    }

    /// Publishes a private article and returns the new public version.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn publish_article(&self, id: ArticleId) -> Result<Article, FigshareError> {
        let location = self
            .execute_location(self.request(
                Method::POST,
                &format!("account/articles/{id}/publish"),
                true,
            )?)
            .await?;
        self.wait_for_public_article_by_url(&location).await
    }

    /// Lists files attached to a private article.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn list_files(&self, id: ArticleId) -> Result<Vec<ArticleFile>, FigshareError> {
        self.list_paginated_files(&format!("account/articles/{id}/files"), true)
            .await
    }

    /// Lists files attached to one public article version.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails or if Figshare returns a
    /// non-success response.
    pub async fn list_public_article_version_files(
        &self,
        article_id: ArticleId,
        version: u64,
    ) -> Result<Vec<ArticleFile>, FigshareError> {
        self.list_paginated_files(
            &format!("articles/{article_id}/versions/{version}/files"),
            false,
        )
        .await
    }

    /// Reads one file attached to a private article.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn get_file(
        &self,
        article_id: ArticleId,
        file_id: FileId,
    ) -> Result<ArticleFile, FigshareError> {
        self.execute_json(self.request(
            Method::GET,
            &format!("account/articles/{article_id}/files/{file_id}"),
            true,
        )?)
        .await
    }

    /// Deletes a file from a private article.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn delete_file(
        &self,
        article_id: ArticleId,
        file_id: FileId,
    ) -> Result<(), FigshareError> {
        self.execute_unit(self.request(
            Method::DELETE,
            &format!("account/articles/{article_id}/files/{file_id}"),
            true,
        )?)
        .await
    }

    /// Initiates a hosted file upload for a private article.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn initiate_file_upload(
        &self,
        article_id: ArticleId,
        name: &str,
        size: u64,
        md5: &str,
    ) -> Result<ArticleFile, FigshareError> {
        let payload = serde_json::json!({
            "name": name,
            "size": size,
            "md5": md5,
        });
        let location = self
            .execute_location(
                self.request(
                    Method::POST,
                    &format!("account/articles/{article_id}/files"),
                    true,
                )?
                .json(&payload),
            )
            .await?;
        self.get_file_by_url(&location).await
    }

    /// Initiates a link-only file attachment for a private article.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn initiate_link_file(
        &self,
        article_id: ArticleId,
        link: &str,
    ) -> Result<ArticleFile, FigshareError> {
        let payload = serde_json::json!({ "link": link });
        let location = self
            .execute_location(
                self.request(
                    Method::POST,
                    &format!("account/articles/{article_id}/files"),
                    true,
                )?
                .json(&payload),
            )
            .await?;
        self.get_file_by_url(&location).await
    }

    /// Reads one upload session from the upload service.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn get_upload_session(
        &self,
        upload_url: &Url,
    ) -> Result<UploadSession, FigshareError> {
        self.execute_json(self.upload_request_url(Method::GET, upload_url.clone())?)
            .await
    }

    /// Uploads one part to the upload service.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn upload_part(
        &self,
        upload_url: &Url,
        part_no: u64,
        bytes: Vec<u8>,
    ) -> Result<(), FigshareError> {
        self.execute_unit(
            self.upload_request_url(Method::PUT, upload_part_url(upload_url, part_no)?)?
                .body(bytes),
        )
        .await
    }

    /// Resets one uploaded part.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn reset_upload_part(
        &self,
        upload_url: &Url,
        part_no: u64,
    ) -> Result<(), FigshareError> {
        self.execute_unit(
            self.upload_request_url(Method::DELETE, upload_part_url(upload_url, part_no)?)?,
        )
        .await
    }

    /// Marks an uploaded file as complete.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the request fails, or
    /// if Figshare returns a non-success response.
    pub async fn complete_file_upload(
        &self,
        article_id: ArticleId,
        file_id: FileId,
    ) -> Result<(), FigshareError> {
        self.execute_unit(self.request(
            Method::POST,
            &format!("account/articles/{article_id}/files/{file_id}"),
            true,
        )?)
        .await
    }

    /// Uploads a local file path to a private article.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if the local file cannot
    /// be read, if the request fails, or if Figshare returns a non-success
    /// response.
    pub async fn upload_path(
        &self,
        article_id: ArticleId,
        path: &Path,
    ) -> Result<ArticleFile, FigshareError> {
        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| {
                FigshareError::InvalidState("path has no final file name segment".into())
            })?;
        self.upload_path_with_filename(article_id, &filename, path)
            .await
    }

    /// Uploads data from a blocking reader by staging it to a temporary file
    /// and performing a standard Figshare hosted upload.
    ///
    /// # Errors
    ///
    /// Returns an error if authentication is missing, if staging the reader
    /// fails, if the request fails, or if Figshare returns a non-success
    /// response.
    pub async fn upload_reader<R>(
        &self,
        article_id: ArticleId,
        filename: &str,
        reader: R,
        content_length: u64,
    ) -> Result<ArticleFile, FigshareError>
    where
        R: Read + Send + 'static,
    {
        let staged = tokio::task::spawn_blocking(move || stage_reader(reader, content_length))
            .await
            .map_err(|error| {
                FigshareError::InvalidState(format!("reader staging task failed: {error}"))
            })??;
        self.upload_path_with_filename(article_id, filename, staged.path())
            .await
    }

    pub(crate) async fn wait_for_public_article_by_url(
        &self,
        url: &Url,
    ) -> Result<Article, FigshareError> {
        let start = Instant::now();
        let mut delay = self.poll.initial_delay;

        loop {
            match self.get_public_article_by_url(url).await {
                Ok(article) => return Ok(article),
                Err(FigshareError::Http { status, .. })
                    if status == reqwest::StatusCode::NOT_FOUND
                        && start.elapsed() < self.poll.max_wait =>
                {
                    sleep(delay).await;
                    delay = min(delay.saturating_mul(2), self.poll.max_delay);
                }
                Err(error) => return Err(error),
            }

            if start.elapsed() >= self.poll.max_wait {
                return Err(FigshareError::Timeout("public article publication"));
            }
        }
    }

    pub(crate) async fn wait_for_own_article_public(
        &self,
        article_id: ArticleId,
    ) -> Result<Article, FigshareError> {
        let start = Instant::now();
        let mut delay = self.poll.initial_delay;

        loop {
            let article = self.get_own_article(article_id).await?;
            if article.is_public_article() {
                return Ok(article);
            }
            if start.elapsed() >= self.poll.max_wait {
                return Err(FigshareError::Timeout("private article publication"));
            }

            sleep(delay).await;
            delay = min(delay.saturating_mul(2), self.poll.max_delay);
        }
    }

    pub(crate) async fn upload_path_with_filename(
        &self,
        article_id: ArticleId,
        filename: &str,
        path: &Path,
    ) -> Result<ArticleFile, FigshareError> {
        let path = path.to_path_buf();
        let checksum_path = path.clone();
        let (md5, size) = tokio::task::spawn_blocking(move || checksum_and_size(&checksum_path))
            .await
            .map_err(|error| {
                FigshareError::InvalidState(format!("checksum task failed: {error}"))
            })??;

        let file = self
            .initiate_file_upload(article_id, filename, size, &md5)
            .await?;
        let result = async {
            let upload_url = file
                .upload_session_url()
                .cloned()
                .ok_or(FigshareError::MissingLink("upload_url"))?;
            let session = self.get_upload_session(&upload_url).await?;

            for part in &session.parts {
                let path = path.clone();
                let start_offset = part.start_offset;
                let len = part.len();
                let bytes =
                    tokio::task::spawn_blocking(move || read_path_range(&path, start_offset, len))
                        .await
                        .map_err(|error| {
                            FigshareError::InvalidState(format!(
                                "path part read task failed: {error}"
                            ))
                        })??;
                self.upload_part(&upload_url, part.part_no, bytes).await?;
            }

            self.complete_file_upload(article_id, file.id).await?;
            let final_session = self.wait_for_upload_completion(&upload_url).await?;
            if matches!(final_session.status, UploadStatus::Aborted) {
                return Err(FigshareError::InvalidState(
                    "Figshare upload was aborted".into(),
                ));
            }

            self.get_file(article_id, file.id).await
        }
        .await;

        match result {
            Ok(file) => Ok(file),
            Err(error) => {
                let _ = self.delete_file(article_id, file.id).await;
                Err(error)
            }
        }
    }

    async fn wait_for_upload_completion(
        &self,
        upload_url: &Url,
    ) -> Result<UploadSession, FigshareError> {
        let start = Instant::now();
        let mut delay = self.poll.initial_delay;

        loop {
            let session = self.get_upload_session(upload_url).await?;
            if session.is_completed() {
                return Ok(session);
            }
            if matches!(session.status, UploadStatus::Aborted) {
                return Ok(session);
            }
            if start.elapsed() >= self.poll.max_wait {
                return Err(FigshareError::Timeout("upload completion"));
            }

            sleep(delay).await;
            delay = min(delay.saturating_mul(2), self.poll.max_delay);
        }
    }

    async fn list_paginated_files(
        &self,
        path: &str,
        auth_required: bool,
    ) -> Result<Vec<ArticleFile>, FigshareError> {
        let max_page_size = usize::try_from(Self::MAX_PAGE_SIZE).map_err(|_| {
            FigshareError::InvalidState("configured file page size does not fit usize".into())
        })?;
        let mut files = Vec::new();
        let mut page = 1_u64;

        loop {
            let batch: Vec<ArticleFile> = self
                .execute_json(self.request(Method::GET, path, auth_required)?.query(&[
                    ("page", page.to_string()),
                    ("page_size", Self::MAX_PAGE_SIZE.to_string()),
                ]))
                .await?;
            let batch_len = batch.len();
            files.extend(batch);

            if batch_len < max_page_size {
                return Ok(files);
            }
            page += 1;
        }
    }
}

fn parse_location(base: &Url, location: &str) -> Result<Url, FigshareError> {
    match Url::parse(location) {
        Ok(url) => Ok(url),
        Err(url::ParseError::RelativeUrlWithoutBase) => Ok(base.join(location)?),
        Err(error) => Err(FigshareError::Url(error)),
    }
}

fn is_trusted_figshare_upload_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host == "uploads.figshare.com"
        || host
            .strip_suffix(".figshare.com")
            .is_some_and(|subdomain| subdomain.starts_with("fup-"))
}

fn upload_part_url(upload_url: &Url, part_no: u64) -> Result<Url, FigshareError> {
    let mut url = upload_url.clone();
    let mut segments = url.path_segments_mut().map_err(|()| {
        FigshareError::InvalidState("upload URL cannot accept part number segments".into())
    })?;
    segments.pop_if_empty();
    segments.push(&part_no.to_string());
    drop(segments);
    Ok(url)
}

fn checksum_and_size(path: &Path) -> Result<(String, u64), FigshareError> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Md5::new();
    let mut size = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        size += u64::try_from(read).map_err(|_| {
            FigshareError::InvalidState("read chunk length does not fit in u64".into())
        })?;
        hasher.update(&buffer[..read]);
    }

    Ok((hex::encode(hasher.finalize()), size))
}

fn read_path_range(path: &Path, offset: u64, len: u64) -> Result<Vec<u8>, FigshareError> {
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    let len = usize::try_from(len).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "requested byte range does not fit in memory on this platform",
        )
    })?;
    let mut bytes = vec![0_u8; len];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn stage_reader<R>(mut reader: R, content_length: u64) -> Result<NamedTempFile, FigshareError>
where
    R: Read,
{
    let mut tempfile = NamedTempFile::new()?;
    let written = std::io::copy(&mut reader.by_ref().take(content_length), &mut tempfile)?;
    if written != content_length {
        return Err(FigshareError::InvalidState(format!(
            "reader produced {written} bytes but {content_length} were declared"
        )));
    }
    Ok(tempfile)
}

#[cfg(test)]
mod tests {
    use std::env::VarError;
    use std::io::Cursor;
    use std::path::Path;
    use std::time::Duration;

    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use reqwest::header::{AUTHORIZATION, LOCATION};
    use reqwest::Method;
    use secrecy::ExposeSecret;
    use serde_json::json;
    use tokio::net::TcpListener;
    use tokio::time::sleep;
    use url::Url;

    use super::{checksum_and_size, parse_location, upload_part_url, Auth, FigshareClient};
    use crate::{Endpoint, FigshareError, PollOptions};

    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvVarGuard {
        name: &'static str,
        previous: Option<String>,
    }

    impl EnvVarGuard {
        fn set(name: &'static str, value: Option<&str>) -> Self {
            let previous = std::env::var(name).ok();
            match value {
                Some(value) => std::env::set_var(name, value),
                None => std::env::remove_var(name),
            }
            Self { name, previous }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var(self.name, value),
                None => std::env::remove_var(self.name),
            }
        }
    }

    #[test]
    fn auth_helpers_cover_anonymous_and_env_loading() {
        let anonymous = Auth::anonymous();
        assert!(anonymous.is_anonymous());
        assert!(format!("{anonymous:?}").contains("<anonymous>"));

        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvVarGuard::set(Auth::TOKEN_ENV_VAR, Some("figshare-token"));
        assert_eq!(
            Auth::from_env().unwrap().token.unwrap().expose_secret(),
            "figshare-token"
        );
    }

    #[test]
    fn auth_env_missing_is_reported() {
        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvVarGuard::set(Auth::TOKEN_ENV_VAR, None);
        match Auth::from_env().unwrap_err() {
            FigshareError::EnvVar { name, source } => {
                assert_eq!(name, Auth::TOKEN_ENV_VAR);
                assert!(matches!(source, VarError::NotPresent));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn auth_debug_redacts_tokens_and_custom_env_vars_are_supported() {
        const CUSTOM_ENV: &str = "FIGSHARE_RS_TEST_TOKEN";

        let auth = Auth::new("secret-token");
        assert!(!auth.is_anonymous());
        assert!(format!("{auth:?}").contains("<redacted>"));

        let _lock = ENV_LOCK.lock().unwrap();
        let _guard = EnvVarGuard::set(CUSTOM_ENV, Some("custom-token"));
        assert_eq!(
            Auth::from_env_var(CUSTOM_ENV)
                .unwrap()
                .token
                .unwrap()
                .expose_secret(),
            "custom-token"
        );
    }

    #[test]
    fn upload_urls_and_locations_are_resolved() {
        let base = Url::parse("https://api.figshare.com/v2/account/articles").unwrap();
        assert_eq!(
            parse_location(&base, "/v2/account/articles/1")
                .unwrap()
                .as_str(),
            "https://api.figshare.com/v2/account/articles/1"
        );

        let upload_url = Url::parse("https://uploads.figshare.com/upload/token").unwrap();
        assert_eq!(
            upload_part_url(&upload_url, 7).unwrap().as_str(),
            "https://uploads.figshare.com/upload/token/7"
        );
    }

    #[test]
    fn checksum_and_reader_staging_cover_local_helpers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("artifact.bin");
        std::fs::write(&path, b"hello").unwrap();

        let (md5, size) = checksum_and_size(&path).unwrap();
        assert_eq!(size, 5);
        assert_eq!(md5, "5d41402abc4b2a76b9719d911017c592");

        let staged = super::stage_reader(Cursor::new(b"world".to_vec()), 5).unwrap();
        assert_eq!(std::fs::read(staged.path()).unwrap(), b"world");
    }

    #[tokio::test]
    async fn request_timeout_is_enforced_for_http_calls() {
        #[derive(Clone)]
        struct DelayState {
            delay: Duration,
        }

        async fn delayed_article(
            State(state): State<DelayState>,
        ) -> (StatusCode, Json<serde_json::Value>) {
            sleep(state.delay).await;
            (
                StatusCode::OK,
                Json(json!({
                    "id": 1,
                    "title": "slow"
                })),
            )
        }

        let app = Router::new()
            .route("/v2/articles/1", get(delayed_article))
            .with_state(DelayState {
                delay: Duration::from_millis(50),
            });
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = FigshareClient::builder(Auth::anonymous())
            .endpoint(Endpoint::Custom(
                Url::parse(&format!("http://{addr}/v2/")).unwrap(),
            ))
            .request_timeout(Duration::from_millis(10))
            .poll_options(PollOptions {
                max_wait: Duration::from_millis(25),
                initial_delay: Duration::from_millis(1),
                max_delay: Duration::from_millis(2),
            })
            .build()
            .unwrap();

        let error = client
            .get_public_article(crate::ArticleId(1))
            .await
            .unwrap_err();
        match error {
            FigshareError::Transport(source) => assert!(source.is_timeout()),
            other => panic!("unexpected error: {other:?}"),
        }

        server.abort();
    }

    #[test]
    fn client_builder_preserves_configuration() {
        let poll = PollOptions {
            max_wait: Duration::from_secs(3),
            initial_delay: Duration::from_millis(2),
            max_delay: Duration::from_millis(4),
        };
        let endpoint = Endpoint::Custom(Url::parse("http://localhost:9999/v2/").unwrap());
        let client = FigshareClient::builder(Auth::new("token"))
            .endpoint(endpoint.clone())
            .user_agent("figshare-rs-tests/0.1")
            .request_timeout(Duration::from_secs(7))
            .connect_timeout(Duration::from_secs(2))
            .poll_options(poll.clone())
            .build()
            .unwrap();

        assert_eq!(client.endpoint(), &endpoint);
        assert_eq!(client.poll_options(), &poll);
        assert_eq!(client.request_timeout(), Some(Duration::from_secs(7)));
        assert_eq!(client.connect_timeout(), Some(Duration::from_secs(2)));
        assert!(FigshareClient::anonymous().is_ok());
        assert!(FigshareClient::with_token("token").is_ok());
    }

    #[test]
    fn private_operations_require_authentication() {
        let client = FigshareClient::anonymous().unwrap();
        let error = client
            .request(Method::GET, "account/articles/1", true)
            .unwrap_err();
        assert!(matches!(error, FigshareError::MissingAuth("api request")));
        let error = client
            .with_download_token(
                Url::parse("https://ndownloader.figshare.com/files/1").unwrap(),
                "private file download",
            )
            .unwrap_err();
        assert!(matches!(
            error,
            FigshareError::MissingAuth("private file download")
        ));
    }

    #[test]
    fn download_token_is_only_added_for_trusted_hosts() {
        let client = FigshareClient::with_token("token").unwrap();
        let downloader = client
            .with_download_token(
                Url::parse("https://ndownloader.figshare.com/files/1").unwrap(),
                "private file download",
            )
            .unwrap();
        assert_eq!(downloader.query(), Some("token=token"));

        let external = client
            .with_download_token(
                Url::parse("https://example.com/file.bin").unwrap(),
                "private file download",
            )
            .unwrap();
        assert_eq!(external.query(), None);
    }

    #[test]
    fn request_helpers_enforce_origin_policies() {
        let client = FigshareClient::builder(Auth::new("token"))
            .endpoint(Endpoint::Custom(
                Url::parse("https://api.example.test/v2/").unwrap(),
            ))
            .build()
            .unwrap();

        let api_error = client
            .request_url(
                Method::GET,
                Url::parse("https://evil.example.test/v2/articles").unwrap(),
                false,
            )
            .unwrap_err();
        assert!(
            matches!(api_error, FigshareError::InvalidState(message) if message.contains("different origin"))
        );

        let upload_request = client
            .upload_request_url(
                Method::PUT,
                Url::parse("https://uploads.figshare.com/upload/token").unwrap(),
            )
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            upload_request.url().host_str(),
            Some("uploads.figshare.com")
        );
        assert_eq!(
            upload_request.headers()[AUTHORIZATION].to_str().unwrap(),
            "token token"
        );

        let regional_upload_request = client
            .upload_request_url(
                Method::PUT,
                Url::parse("https://fup-eu-west-1.figshare.com/upload/token").unwrap(),
            )
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            regional_upload_request.url().host_str(),
            Some("fup-eu-west-1.figshare.com")
        );
        assert_eq!(
            regional_upload_request.headers()[AUTHORIZATION]
                .to_str()
                .unwrap(),
            "token token"
        );

        let upload_error = client
            .upload_request_url(
                Method::PUT,
                Url::parse("https://evil.example.test/upload/token").unwrap(),
            )
            .unwrap_err();
        assert!(
            matches!(upload_error, FigshareError::InvalidState(message) if message.contains("different origin"))
        );

        let public_download = client
            .download_request_url(
                Method::GET,
                Url::parse("https://downloads.example.test/file.bin").unwrap(),
                false,
            )
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(public_download.url().query(), None);
    }

    #[tokio::test]
    async fn execute_location_supports_multiple_success_shapes() {
        let app = Router::new()
            .route(
                "/v2/location/header",
                post(|| async {
                    (
                        StatusCode::CREATED,
                        [(LOCATION, "/v2/account/articles/1")],
                        Json(json!({ "ignored": true })),
                    )
                }),
            )
            .route(
                "/v2/location/object",
                post(|| async {
                    (
                        StatusCode::CREATED,
                        Json(json!({ "location": "/v2/account/articles/2" })),
                    )
                }),
            )
            .route(
                "/v2/location/string",
                post(|| async { (StatusCode::CREATED, Json(json!("/v2/account/articles/3"))) }),
            )
            .route(
                "/v2/location/text",
                post(|| async { (StatusCode::CREATED, "/v2/account/articles/4") }),
            )
            .route("/v2/location/empty", post(|| async { StatusCode::CREATED }));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = FigshareClient::builder(Auth::anonymous())
            .endpoint(Endpoint::Custom(
                Url::parse(&format!("http://{addr}/v2/")).unwrap(),
            ))
            .build()
            .unwrap();

        let header = client
            .execute_location(
                client
                    .request(Method::POST, "location/header", false)
                    .unwrap(),
            )
            .await
            .unwrap();
        let object = client
            .execute_location(
                client
                    .request(Method::POST, "location/object", false)
                    .unwrap(),
            )
            .await
            .unwrap();
        let string = client
            .execute_location(
                client
                    .request(Method::POST, "location/string", false)
                    .unwrap(),
            )
            .await
            .unwrap();
        let text = client
            .execute_location(
                client
                    .request(Method::POST, "location/text", false)
                    .unwrap(),
            )
            .await
            .unwrap();
        let empty = client
            .execute_location(
                client
                    .request(Method::POST, "location/empty", false)
                    .unwrap(),
            )
            .await
            .unwrap_err();

        assert_eq!(header.path(), "/v2/account/articles/1");
        assert_eq!(object.path(), "/v2/account/articles/2");
        assert_eq!(string.path(), "/v2/account/articles/3");
        assert_eq!(text.path(), "/v2/account/articles/4");
        assert!(
            matches!(empty, FigshareError::InvalidState(message) if message.contains("did not include a location"))
        );

        server.abort();
    }

    #[tokio::test]
    async fn list_paginated_files_fetches_multiple_pages() {
        async fn files_route(
            State(()): State<()>,
            axum::extract::Query(query): axum::extract::Query<
                std::collections::HashMap<String, String>,
            >,
        ) -> Json<Vec<serde_json::Value>> {
            let page = query
                .get("page")
                .and_then(|value| value.parse::<u64>().ok())
                .unwrap_or(1);
            let count = if page == 1 { 1_000 } else { 1 };
            let start = if page == 1 { 1 } else { 1_001 };
            Json(
                (start..start + count)
                    .map(|id| json!({ "id": id, "name": format!("file-{id}.bin"), "size": 1 }))
                    .collect(),
            )
        }

        let app = Router::new()
            .route("/v2/account/articles/1/files", get(files_route))
            .with_state(());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = FigshareClient::builder(Auth::new("token"))
            .endpoint(Endpoint::Custom(
                Url::parse(&format!("http://{addr}/v2/")).unwrap(),
            ))
            .build()
            .unwrap();

        let files = client
            .list_paginated_files("account/articles/1/files", true)
            .await
            .unwrap();
        assert_eq!(files.len(), 1_001);
        assert_eq!(files.first().unwrap().id.0, 1);
        assert_eq!(files.last().unwrap().id.0, 1_001);

        server.abort();
    }

    #[tokio::test]
    async fn upload_path_rejects_missing_filename() {
        let client = FigshareClient::anonymous().unwrap();
        let error = client
            .upload_path(crate::ArticleId(1), Path::new("/"))
            .await
            .unwrap_err();
        assert!(
            matches!(error, FigshareError::InvalidState(message) if message.contains("path has no final file name segment"))
        );
    }
}
