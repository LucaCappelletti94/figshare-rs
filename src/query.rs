//! Typed search and list query builders for Figshare articles.

use serde_json::{Map, Value};

use crate::error::FigshareError;
use crate::metadata::DefinedType;

/// Supported sort fields for article list and search endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArticleOrder {
    /// Sort by creation date.
    CreatedDate,
    /// Sort by publication date.
    PublishedDate,
    /// Sort by modification date.
    ModifiedDate,
    /// Sort by views.
    Views,
    /// Sort by shares.
    Shares,
    /// Sort by downloads.
    Downloads,
    /// Sort by citations.
    Cites,
}

impl ArticleOrder {
    #[must_use]
    pub(crate) fn as_api_str(self) -> &'static str {
        match self {
            Self::CreatedDate => "created_date",
            Self::PublishedDate => "published_date",
            Self::ModifiedDate => "modified_date",
            Self::Views => "views",
            Self::Shares => "shares",
            Self::Downloads => "downloads",
            Self::Cites => "cites",
        }
    }
}

/// Sort direction for list and search endpoints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderDirection {
    /// Ascending order.
    Asc,
    /// Descending order.
    Desc,
}

impl OrderDirection {
    #[must_use]
    pub(crate) fn as_api_str(self) -> &'static str {
        match self {
            Self::Asc => "asc",
            Self::Desc => "desc",
        }
    }
}

/// Shared query options for public and authenticated article list/search calls.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ArticleQuery {
    /// Search string used by `POST .../search` endpoints.
    pub search_for: Option<String>,
    /// Filter results published since the given ISO-8601 string.
    pub published_since: Option<String>,
    /// Filter results modified since the given ISO-8601 string.
    pub modified_since: Option<String>,
    /// Restrict results to the given institution.
    pub institution: Option<u64>,
    /// Restrict results to the given group.
    pub group: Option<u64>,
    /// Restrict results to the given item type.
    pub item_type: Option<DefinedType>,
    /// Restrict results to the given resource DOI.
    pub resource_doi: Option<String>,
    /// Restrict results to the given DOI.
    pub doi: Option<String>,
    /// Restrict results to the given handle.
    pub handle: Option<String>,
    /// Restrict results to the given project.
    pub project_id: Option<u64>,
    /// Legacy resource title filter retained for backward-compatibility checks.
    pub resource_title: Option<String>,
    /// Optional sort field.
    pub order: Option<ArticleOrder>,
    /// Optional sort direction.
    pub order_direction: Option<OrderDirection>,
    /// Page number based pagination.
    pub page: Option<u64>,
    /// Page size based pagination.
    pub page_size: Option<u64>,
    /// Offset based pagination.
    pub offset: Option<u64>,
    /// Limit based pagination.
    pub limit: Option<u64>,
    /// Extra raw key-value pairs forwarded as-is.
    pub custom: Vec<(String, String)>,
}

impl ArticleQuery {
    /// Starts building a query.
    ///
    /// # Examples
    ///
    /// ```
    /// use figshare_rs::{ArticleOrder, ArticleQuery, DefinedType, OrderDirection};
    ///
    /// let query = ArticleQuery::builder()
    ///     .item_type(DefinedType::Dataset)
    ///     .doi("10.6084/m9.figshare.123")
    ///     .order(ArticleOrder::PublishedDate)
    ///     .order_direction(OrderDirection::Desc)
    ///     .limit(10)
    ///     .build();
    ///
    /// assert_eq!(query.item_type, Some(DefinedType::Dataset));
    /// assert_eq!(query.doi.as_deref(), Some("10.6084/m9.figshare.123"));
    /// assert_eq!(query.limit, Some(10));
    /// ```
    #[must_use]
    pub fn builder() -> ArticleQueryBuilder {
        ArticleQueryBuilder::default()
    }

    pub(crate) fn as_public_list_query_pairs(
        &self,
    ) -> Result<Vec<(String, String)>, FigshareError> {
        self.validate_pagination()?;
        Self::ensure_unsupported_fields(
            "list_public_articles",
            [
                ("search_for", self.search_for.is_some()),
                ("project_id", self.project_id.is_some()),
                ("resource_title", self.resource_title.is_some()),
            ],
        )?;

        let mut pairs = Vec::new();
        self.push_common_pairs(&mut pairs);
        if let Some(item_type) = &self.item_type {
            if let Some(id) = item_type.api_id() {
                pairs.push(("item_type".into(), id.to_string()));
            }
        }
        if let Some(resource_doi) = &self.resource_doi {
            pairs.push(("resource_doi".into(), resource_doi.clone()));
        }
        if let Some(doi) = &self.doi {
            pairs.push(("doi".into(), doi.clone()));
        }
        if let Some(handle) = &self.handle {
            pairs.push(("handle".into(), handle.clone()));
        }
        if let Some(order) = self.order {
            pairs.push(("order".into(), order.as_api_str().into()));
        }
        if let Some(order_direction) = self.order_direction {
            pairs.push((
                "order_direction".into(),
                order_direction.as_api_str().into(),
            ));
        }
        self.push_pagination_pairs(&mut pairs);
        pairs.extend(self.custom.iter().cloned());
        Ok(pairs)
    }

    pub(crate) fn as_own_list_query_pairs(&self) -> Result<Vec<(String, String)>, FigshareError> {
        self.validate_pagination()?;
        Self::ensure_unsupported_fields(
            "list_own_articles",
            [
                ("search_for", self.search_for.is_some()),
                ("published_since", self.published_since.is_some()),
                ("modified_since", self.modified_since.is_some()),
                ("institution", self.institution.is_some()),
                ("group", self.group.is_some()),
                ("item_type", self.item_type.is_some()),
                ("resource_doi", self.resource_doi.is_some()),
                ("resource_title", self.resource_title.is_some()),
                ("order", self.order.is_some()),
                ("order_direction", self.order_direction.is_some()),
                ("doi", self.doi.is_some()),
                ("handle", self.handle.is_some()),
                ("project_id", self.project_id.is_some()),
            ],
        )?;

        let mut pairs = Vec::new();
        self.push_pagination_pairs(&mut pairs);
        pairs.extend(self.custom.iter().cloned());
        Ok(pairs)
    }

    pub(crate) fn as_public_search_body(&self) -> Result<Value, FigshareError> {
        self.validate_pagination()?;
        Self::ensure_unsupported_fields(
            "search_public_articles",
            [("resource_title", self.resource_title.is_some())],
        )?;

        let mut object = Map::new();
        self.insert_common_search_fields(&mut object);
        if let Some(item_type) = &self.item_type {
            if let Some(id) = item_type.api_id() {
                object.insert("item_type".into(), Value::from(id));
            }
        }
        if let Some(resource_doi) = &self.resource_doi {
            object.insert("resource_doi".into(), Value::String(resource_doi.clone()));
        }
        if let Some(doi) = &self.doi {
            object.insert("doi".into(), Value::String(doi.clone()));
        }
        if let Some(handle) = &self.handle {
            object.insert("handle".into(), Value::String(handle.clone()));
        }
        if let Some(project_id) = self.project_id {
            object.insert("project_id".into(), Value::from(project_id));
        }
        if let Some(order) = self.order {
            object.insert("order".into(), Value::String(order.as_api_str().into()));
        }
        if let Some(order_direction) = self.order_direction {
            object.insert(
                "order_direction".into(),
                Value::String(order_direction.as_api_str().into()),
            );
        }
        self.insert_pagination_fields(&mut object);
        for (key, value) in &self.custom {
            object.insert(key.clone(), Value::String(value.clone()));
        }
        Ok(Value::Object(object))
    }

    pub(crate) fn as_own_search_body(&self) -> Result<Value, FigshareError> {
        self.validate_pagination()?;
        Self::ensure_unsupported_fields(
            "search_own_articles",
            [("resource_title", self.resource_title.is_some())],
        )?;

        let mut object = Map::new();
        self.insert_common_search_fields(&mut object);
        if let Some(item_type) = &self.item_type {
            if let Some(id) = item_type.api_id() {
                object.insert("item_type".into(), Value::from(id));
            }
        }
        if let Some(resource_doi) = &self.resource_doi {
            object.insert("resource_doi".into(), Value::String(resource_doi.clone()));
        }
        if let Some(doi) = &self.doi {
            object.insert("doi".into(), Value::String(doi.clone()));
        }
        if let Some(handle) = &self.handle {
            object.insert("handle".into(), Value::String(handle.clone()));
        }
        if let Some(project_id) = self.project_id {
            object.insert("project_id".into(), Value::from(project_id));
        }
        if let Some(order) = self.order {
            object.insert("order".into(), Value::String(order.as_api_str().into()));
        }
        if let Some(order_direction) = self.order_direction {
            object.insert(
                "order_direction".into(),
                Value::String(order_direction.as_api_str().into()),
            );
        }
        self.insert_pagination_fields(&mut object);
        for (key, value) in &self.custom {
            object.insert(key.clone(), Value::String(value.clone()));
        }
        Ok(Value::Object(object))
    }

    fn validate_pagination(&self) -> Result<(), FigshareError> {
        let uses_page = self.page.is_some() || self.page_size.is_some();
        let uses_offset = self.limit.is_some() || self.offset.is_some();
        if uses_page && uses_offset {
            return Err(FigshareError::InvalidState(
                "cannot mix page/page_size with limit/offset pagination".into(),
            ));
        }
        Ok(())
    }

    fn push_common_pairs(&self, pairs: &mut Vec<(String, String)>) {
        if let Some(published_since) = &self.published_since {
            pairs.push(("published_since".into(), published_since.clone()));
        }
        if let Some(modified_since) = &self.modified_since {
            pairs.push(("modified_since".into(), modified_since.clone()));
        }
        if let Some(institution) = self.institution {
            pairs.push(("institution".into(), institution.to_string()));
        }
        if let Some(group) = self.group {
            pairs.push(("group".into(), group.to_string()));
        }
    }

    fn push_pagination_pairs(&self, pairs: &mut Vec<(String, String)>) {
        if let Some(page) = self.page {
            pairs.push(("page".into(), page.to_string()));
        }
        if let Some(page_size) = self.page_size {
            pairs.push(("page_size".into(), page_size.to_string()));
        }
        if let Some(offset) = self.offset {
            pairs.push(("offset".into(), offset.to_string()));
        }
        if let Some(limit) = self.limit {
            pairs.push(("limit".into(), limit.to_string()));
        }
    }

    fn insert_common_search_fields(&self, object: &mut Map<String, Value>) {
        if let Some(search_for) = &self.search_for {
            object.insert("search_for".into(), Value::String(search_for.clone()));
        }
        if let Some(published_since) = &self.published_since {
            object.insert(
                "published_since".into(),
                Value::String(published_since.clone()),
            );
        }
        if let Some(modified_since) = &self.modified_since {
            object.insert(
                "modified_since".into(),
                Value::String(modified_since.clone()),
            );
        }
        if let Some(institution) = self.institution {
            object.insert("institution".into(), Value::from(institution));
        }
        if let Some(group) = self.group {
            object.insert("group".into(), Value::from(group));
        }
    }

    fn insert_pagination_fields(&self, object: &mut Map<String, Value>) {
        if let Some(page) = self.page {
            object.insert("page".into(), Value::from(page));
        }
        if let Some(page_size) = self.page_size {
            object.insert("page_size".into(), Value::from(page_size));
        }
        if let Some(offset) = self.offset {
            object.insert("offset".into(), Value::from(offset));
        }
        if let Some(limit) = self.limit {
            object.insert("limit".into(), Value::from(limit));
        }
    }

    fn ensure_unsupported_fields<const N: usize>(
        endpoint: &str,
        fields: [(&'static str, bool); N],
    ) -> Result<(), FigshareError> {
        let unsupported = fields
            .into_iter()
            .filter_map(|(name, is_set)| is_set.then_some(name))
            .collect::<Vec<_>>();
        if unsupported.is_empty() {
            return Ok(());
        }

        Err(FigshareError::InvalidState(format!(
            "{} not supported for {endpoint}",
            unsupported.join(", ")
        )))
    }
}

/// Builder for [`ArticleQuery`].
#[derive(Clone, Debug, Default)]
pub struct ArticleQueryBuilder {
    query: ArticleQuery,
}

impl ArticleQueryBuilder {
    /// Sets the free-form search string used by search endpoints.
    #[must_use]
    pub fn search_for(mut self, search_for: impl Into<String>) -> Self {
        self.query.search_for = Some(search_for.into());
        self
    }

    /// Sets the publication lower bound.
    #[must_use]
    pub fn published_since(mut self, published_since: impl Into<String>) -> Self {
        self.query.published_since = Some(published_since.into());
        self
    }

    /// Sets the modification lower bound.
    #[must_use]
    pub fn modified_since(mut self, modified_since: impl Into<String>) -> Self {
        self.query.modified_since = Some(modified_since.into());
        self
    }

    /// Restricts results to one institution.
    #[must_use]
    pub fn institution(mut self, institution: u64) -> Self {
        self.query.institution = Some(institution);
        self
    }

    /// Restricts results to one group.
    #[must_use]
    pub fn group(mut self, group: u64) -> Self {
        self.query.group = Some(group);
        self
    }

    /// Restricts results to one defined type.
    #[must_use]
    pub fn item_type(mut self, item_type: DefinedType) -> Self {
        self.query.item_type = Some(item_type);
        self
    }

    /// Filters by resource DOI.
    #[must_use]
    pub fn resource_doi(mut self, resource_doi: impl Into<String>) -> Self {
        self.query.resource_doi = Some(resource_doi.into());
        self
    }

    /// Filters by DOI.
    #[must_use]
    pub fn doi(mut self, doi: impl Into<String>) -> Self {
        self.query.doi = Some(doi.into());
        self
    }

    /// Filters by handle.
    #[must_use]
    pub fn handle(mut self, handle: impl Into<String>) -> Self {
        self.query.handle = Some(handle.into());
        self
    }

    /// Filters by project ID.
    #[must_use]
    pub fn project_id(mut self, project_id: u64) -> Self {
        self.query.project_id = Some(project_id);
        self
    }

    /// Legacy resource title filter retained only for compatibility checks.
    #[must_use]
    pub fn resource_title(mut self, resource_title: impl Into<String>) -> Self {
        self.query.resource_title = Some(resource_title.into());
        self
    }

    /// Sets the sort field.
    #[must_use]
    pub fn order(mut self, order: ArticleOrder) -> Self {
        self.query.order = Some(order);
        self
    }

    /// Sets the sort direction.
    #[must_use]
    pub fn order_direction(mut self, order_direction: OrderDirection) -> Self {
        self.query.order_direction = Some(order_direction);
        self
    }

    /// Uses page-based pagination.
    #[must_use]
    pub fn page(mut self, page: u64) -> Self {
        self.query.page = Some(page);
        self
    }

    /// Uses page-size pagination.
    #[must_use]
    pub fn page_size(mut self, page_size: u64) -> Self {
        self.query.page_size = Some(page_size);
        self
    }

    /// Uses offset-based pagination.
    #[must_use]
    pub fn offset(mut self, offset: u64) -> Self {
        self.query.offset = Some(offset);
        self
    }

    /// Uses limit-based pagination.
    #[must_use]
    pub fn limit(mut self, limit: u64) -> Self {
        self.query.limit = Some(limit);
        self
    }

    /// Adds a raw custom key-value pair.
    #[must_use]
    pub fn custom(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.query.custom.push((key.into(), value.into()));
        self
    }

    /// Finishes the builder.
    #[must_use]
    pub fn build(self) -> ArticleQuery {
        self.query
    }
}

#[cfg(test)]
mod tests {
    use super::{ArticleOrder, ArticleQuery, OrderDirection};
    use crate::metadata::DefinedType;

    #[test]
    fn query_serializes_public_list_pairs() {
        let query = ArticleQuery::builder()
            .published_since("2024-01-01")
            .item_type(DefinedType::Dataset)
            .doi("10.6084/m9.figshare.123")
            .order(ArticleOrder::PublishedDate)
            .order_direction(OrderDirection::Desc)
            .page(2)
            .page_size(25)
            .custom("foo", "bar")
            .build();

        let pairs = query.as_public_list_query_pairs().unwrap();
        assert!(pairs.contains(&("published_since".into(), "2024-01-01".into())));
        assert!(pairs.contains(&("item_type".into(), "3".into())));
        assert!(pairs.contains(&("doi".into(), "10.6084/m9.figshare.123".into())));
        assert!(pairs.contains(&("foo".into(), "bar".into())));
    }

    #[test]
    fn query_serializes_public_search_body_without_search_for() {
        let query = ArticleQuery::builder()
            .item_type(DefinedType::Dataset)
            .limit(10)
            .build();

        let body = query.as_public_search_body().unwrap();
        assert_eq!(body["item_type"], 3);
        assert_eq!(body["limit"], 10);
    }

    #[test]
    fn query_rejects_mixed_pagination_styles() {
        let query = ArticleQuery {
            page: Some(1),
            limit: Some(10),
            ..ArticleQuery::default()
        };
        assert!(query.as_public_list_query_pairs().is_err());
        assert!(query.as_public_search_body().is_err());
    }

    #[test]
    fn own_list_rejects_unsupported_filters() {
        let query = ArticleQuery::builder()
            .item_type(DefinedType::Dataset)
            .page(1)
            .build();
        assert!(query.as_own_list_query_pairs().is_err());
    }

    #[test]
    fn public_list_rejects_search_only_filters() {
        let query = ArticleQuery::builder().project_id(7).page(1).build();
        assert!(query.as_public_list_query_pairs().is_err());
    }
}
