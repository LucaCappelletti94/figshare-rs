//! Article metadata builders and defined type helpers.

use std::collections::BTreeMap;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::ids::{CategoryId, Doi, LicenseId};

/// Typed article kind used by Figshare payloads and search filters.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DefinedType {
    /// Figure.
    Figure,
    /// Dataset.
    Dataset,
    /// Media.
    Media,
    /// Poster.
    Poster,
    /// Journal contribution.
    JournalContribution,
    /// Presentation.
    Presentation,
    /// Thesis.
    Thesis,
    /// Software.
    Software,
    /// Online resource.
    OnlineResource,
    /// Preprint.
    Preprint,
    /// Book.
    Book,
    /// Conference contribution.
    ConferenceContribution,
    /// Chapter.
    Chapter,
    /// Peer review.
    PeerReview,
    /// Educational resource.
    EducationalResource,
    /// Report.
    Report,
    /// Standard.
    Standard,
    /// Composition.
    Composition,
    /// Funding.
    Funding,
    /// Physical object.
    PhysicalObject,
    /// Data management plan.
    DataManagementPlan,
    /// Workflow.
    Workflow,
    /// Monograph.
    Monograph,
    /// Performance.
    Performance,
    /// Event.
    Event,
    /// Service.
    Service,
    /// Model.
    Model,
    /// Unknown server value preserved as-is.
    Unknown(String),
}

impl DefinedType {
    /// Returns the string form used by create and update payloads.
    ///
    /// # Examples
    ///
    /// ```
    /// use figshare_rs::DefinedType;
    ///
    /// assert_eq!(DefinedType::Software.api_name(), "software");
    /// assert_eq!(
    ///     DefinedType::JournalContribution.api_name(),
    ///     "journal contribution"
    /// );
    /// ```
    #[must_use]
    pub fn api_name(&self) -> &str {
        match self {
            Self::Figure => "figure",
            Self::Dataset => "dataset",
            Self::Media => "media",
            Self::Poster => "poster",
            Self::JournalContribution => "journal contribution",
            Self::Presentation => "presentation",
            Self::Thesis => "thesis",
            Self::Software => "software",
            Self::OnlineResource => "online resource",
            Self::Preprint => "preprint",
            Self::Book => "book",
            Self::ConferenceContribution => "conference contribution",
            Self::Chapter => "chapter",
            Self::PeerReview => "peer review",
            Self::EducationalResource => "educational resource",
            Self::Report => "report",
            Self::Standard => "standard",
            Self::Composition => "composition",
            Self::Funding => "funding",
            Self::PhysicalObject => "physical object",
            Self::DataManagementPlan => "data management plan",
            Self::Workflow => "workflow",
            Self::Monograph => "monograph",
            Self::Performance => "performance",
            Self::Event => "event",
            Self::Service => "service",
            Self::Model => "model",
            Self::Unknown(value) => value.as_str(),
        }
    }

    /// Returns the numeric form used by some list/search filters and presenters.
    #[must_use]
    pub fn api_id(&self) -> Option<u64> {
        match self {
            Self::Figure => Some(1),
            Self::Dataset => Some(3),
            Self::Media => Some(2),
            Self::Poster => Some(5),
            Self::JournalContribution => Some(6),
            Self::Presentation => Some(7),
            Self::Thesis => Some(8),
            Self::Software => Some(9),
            Self::OnlineResource => Some(11),
            Self::Preprint => Some(12),
            Self::Book => Some(13),
            Self::ConferenceContribution => Some(14),
            Self::Chapter => Some(15),
            Self::PeerReview => Some(16),
            Self::EducationalResource => Some(17),
            Self::Report => Some(18),
            Self::Standard => Some(19),
            Self::Composition => Some(20),
            Self::Funding => Some(21),
            Self::PhysicalObject => Some(22),
            Self::DataManagementPlan => Some(23),
            Self::Workflow => Some(24),
            Self::Monograph => Some(25),
            Self::Performance => Some(26),
            Self::Event => Some(27),
            Self::Service => Some(28),
            Self::Model => Some(29),
            Self::Unknown(value) => value.parse().ok(),
        }
    }

    /// Converts the integer representation used by presenter payloads into a
    /// typed [`DefinedType`].
    #[must_use]
    pub fn from_api_id(id: u64) -> Self {
        match id {
            1 => Self::Figure,
            2 => Self::Media,
            3 => Self::Dataset,
            5 => Self::Poster,
            6 => Self::JournalContribution,
            7 => Self::Presentation,
            8 => Self::Thesis,
            9 => Self::Software,
            11 => Self::OnlineResource,
            12 => Self::Preprint,
            13 => Self::Book,
            14 => Self::ConferenceContribution,
            15 => Self::Chapter,
            16 => Self::PeerReview,
            17 => Self::EducationalResource,
            18 => Self::Report,
            19 => Self::Standard,
            20 => Self::Composition,
            21 => Self::Funding,
            22 => Self::PhysicalObject,
            23 => Self::DataManagementPlan,
            24 => Self::Workflow,
            25 => Self::Monograph,
            26 => Self::Performance,
            27 => Self::Event,
            28 => Self::Service,
            29 => Self::Model,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// Converts the string representation used by create/update payloads into a
    /// typed [`DefinedType`].
    #[must_use]
    pub fn from_api_name(value: impl Into<String>) -> Self {
        let value = value.into();
        let normalized = value.to_ascii_lowercase().replace('_', " ");
        match normalized.as_str() {
            "figure" => Self::Figure,
            "media" => Self::Media,
            "dataset" => Self::Dataset,
            "poster" => Self::Poster,
            "paper" | "journal contribution" => Self::JournalContribution,
            "presentation" => Self::Presentation,
            "thesis" => Self::Thesis,
            "code" | "software" => Self::Software,
            "metadata" | "online resource" => Self::OnlineResource,
            "preprint" => Self::Preprint,
            "book" => Self::Book,
            "conference contribution" => Self::ConferenceContribution,
            "chapter" => Self::Chapter,
            "peer review" => Self::PeerReview,
            "educational resource" => Self::EducationalResource,
            "report" => Self::Report,
            "standard" => Self::Standard,
            "composition" => Self::Composition,
            "funding" => Self::Funding,
            "physical object" => Self::PhysicalObject,
            "data management plan" => Self::DataManagementPlan,
            "workflow" => Self::Workflow,
            "monograph" => Self::Monograph,
            "performance" => Self::Performance,
            "event" => Self::Event,
            "service" => Self::Service,
            "model" => Self::Model,
            _ => Self::Unknown(value),
        }
    }
}

impl Serialize for DefinedType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.api_name())
    }
}

impl<'de> Deserialize<'de> for DefinedType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DefinedTypeVisitor;

        impl Visitor<'_> for DefinedTypeVisitor {
            type Value = DefinedType;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a Figshare defined_type string or integer")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(DefinedType::from_api_id(value))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                let value = u64::try_from(value).map_err(E::custom)?;
                Ok(DefinedType::from_api_id(value))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if !value.is_finite() || value.fract() != 0.0 || value < 0.0 {
                    return Err(E::custom("expected an integer-like defined_type value"));
                }

                let value = value.to_string().parse::<u64>().map_err(E::custom)?;
                Ok(DefinedType::from_api_id(value))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(DefinedType::from_api_name(value))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(DefinedType::from_api_name(value))
            }
        }

        deserializer.deserialize_any(DefinedTypeVisitor)
    }
}

/// Reference to an author in create/update payloads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AuthorReference {
    /// Reference an existing author by ID.
    Id {
        /// Existing author identifier.
        id: u64,
    },
    /// Reference an author by free-form display name.
    Name {
        /// Author display name.
        name: String,
    },
}

impl AuthorReference {
    /// Creates an ID-based author reference.
    #[must_use]
    pub fn id(id: u64) -> Self {
        Self::Id { id }
    }

    /// Creates a name-based author reference.
    #[must_use]
    pub fn name(name: impl Into<String>) -> Self {
        Self::Name { name: name.into() }
    }
}

/// Builder errors for [`ArticleMetadata`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ArticleMetadataBuildError {
    /// The title field is required.
    #[error("missing required field: title")]
    MissingTitle,
    /// The defined type field is required.
    #[error("missing required field: defined_type")]
    MissingDefinedType,
}

/// High-level create/update payload used by workflow helpers.
#[derive(Clone, Debug, PartialEq)]
pub struct ArticleMetadata {
    /// Title of the article.
    pub title: String,
    /// Optional description.
    pub description: Option<String>,
    /// Required item type.
    pub defined_type: DefinedType,
    /// Optional tags.
    pub tags: Vec<String>,
    /// Optional keywords.
    pub keywords: Vec<String>,
    /// Optional references.
    pub references: Vec<String>,
    /// Optional category identifiers.
    pub categories: Vec<CategoryId>,
    /// Optional author references.
    pub authors: Vec<AuthorReference>,
    /// Optional custom fields.
    pub custom_fields: BTreeMap<String, Value>,
    /// Optional funding string.
    pub funding: Option<String>,
    /// Optional license identifier.
    pub license: Option<LicenseId>,
    /// Optional pre-reserved DOI.
    pub doi: Option<Doi>,
    /// Optional related resource DOI.
    pub resource_doi: Option<String>,
    /// Optional related resource title.
    pub resource_title: Option<String>,
}

impl ArticleMetadata {
    /// Starts building article metadata.
    ///
    /// # Examples
    ///
    /// ```
    /// use figshare_rs::{ArticleMetadata, DefinedType};
    ///
    /// let metadata = ArticleMetadata::builder()
    ///     .title("Example dataset")
    ///     .defined_type(DefinedType::Dataset)
    ///     .author_named("Doe, Jane")
    ///     .tag("example")
    ///     .build()?;
    ///
    /// assert_eq!(metadata.title, "Example dataset");
    /// assert_eq!(metadata.defined_type, DefinedType::Dataset);
    /// assert_eq!(metadata.tags, vec!["example".to_owned()]);
    /// assert_eq!(metadata.authors.len(), 1);
    /// # Ok::<(), figshare_rs::ArticleMetadataBuildError>(())
    /// ```
    #[must_use]
    pub fn builder() -> ArticleMetadataBuilder {
        ArticleMetadataBuilder::default()
    }

    pub(crate) fn to_payload(&self) -> Value {
        let mut object = Map::new();
        object.insert("title".into(), Value::String(self.title.clone()));
        object.insert(
            "defined_type".into(),
            Value::String(self.defined_type.api_name().to_owned()),
        );

        if let Some(description) = &self.description {
            object.insert("description".into(), Value::String(description.clone()));
        }
        if !self.tags.is_empty() {
            object.insert(
                "tags".into(),
                Value::Array(self.tags.iter().cloned().map(Value::String).collect()),
            );
        }
        if !self.keywords.is_empty() {
            object.insert(
                "keywords".into(),
                Value::Array(self.keywords.iter().cloned().map(Value::String).collect()),
            );
        }
        if !self.references.is_empty() {
            object.insert(
                "references".into(),
                Value::Array(self.references.iter().cloned().map(Value::String).collect()),
            );
        }
        if !self.categories.is_empty() {
            object.insert(
                "categories".into(),
                Value::Array(
                    self.categories
                        .iter()
                        .map(|category| Value::from(category.0))
                        .collect(),
                ),
            );
        }
        if !self.authors.is_empty() {
            object.insert(
                "authors".into(),
                Value::Array(
                    self.authors
                        .iter()
                        .map(|author| match author {
                            AuthorReference::Id { id } => {
                                let mut author = Map::new();
                                author.insert("id".into(), Value::from(*id));
                                Value::Object(author)
                            }
                            AuthorReference::Name { name } => {
                                let mut author = Map::new();
                                author.insert("name".into(), Value::String(name.clone()));
                                Value::Object(author)
                            }
                        })
                        .collect(),
                ),
            );
        }
        if !self.custom_fields.is_empty() {
            object.insert(
                "custom_fields".into(),
                Value::Object(self.custom_fields.clone().into_iter().collect()),
            );
        }
        if let Some(funding) = &self.funding {
            object.insert("funding".into(), Value::String(funding.clone()));
        }
        if let Some(license) = self.license {
            object.insert("license".into(), Value::from(license.0));
        }
        if let Some(doi) = &self.doi {
            object.insert("doi".into(), Value::String(doi.to_string()));
        }
        if let Some(resource_doi) = &self.resource_doi {
            object.insert("resource_doi".into(), Value::String(resource_doi.clone()));
        }
        if let Some(resource_title) = &self.resource_title {
            object.insert(
                "resource_title".into(),
                Value::String(resource_title.clone()),
            );
        }

        Value::Object(object)
    }
}

/// Builder for [`ArticleMetadata`].
#[derive(Clone, Debug, Default)]
pub struct ArticleMetadataBuilder {
    title: Option<String>,
    description: Option<String>,
    defined_type: Option<DefinedType>,
    tags: Vec<String>,
    keywords: Vec<String>,
    references: Vec<String>,
    categories: Vec<CategoryId>,
    authors: Vec<AuthorReference>,
    custom_fields: BTreeMap<String, Value>,
    funding: Option<String>,
    license: Option<LicenseId>,
    doi: Option<Doi>,
    resource_doi: Option<String>,
    resource_title: Option<String>,
}

impl ArticleMetadataBuilder {
    /// Sets the title.
    #[must_use]
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets the description.
    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the defined type.
    #[must_use]
    pub fn defined_type(mut self, defined_type: DefinedType) -> Self {
        self.defined_type = Some(defined_type);
        self
    }

    /// Adds one tag.
    #[must_use]
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Adds one keyword.
    #[must_use]
    pub fn keyword(mut self, keyword: impl Into<String>) -> Self {
        self.keywords.push(keyword.into());
        self
    }

    /// Adds one reference URL or citation.
    #[must_use]
    pub fn reference(mut self, reference: impl Into<String>) -> Self {
        self.references.push(reference.into());
        self
    }

    /// Adds one category ID.
    #[must_use]
    pub fn category_id(mut self, category: impl Into<CategoryId>) -> Self {
        self.categories.push(category.into());
        self
    }

    /// Adds one author reference.
    #[must_use]
    pub fn author(mut self, author: AuthorReference) -> Self {
        self.authors.push(author);
        self
    }

    /// Adds one author by ID.
    #[must_use]
    pub fn author_id(self, author_id: u64) -> Self {
        self.author(AuthorReference::id(author_id))
    }

    /// Adds one author by name.
    #[must_use]
    pub fn author_named(self, name: impl Into<String>) -> Self {
        self.author(AuthorReference::name(name))
    }

    /// Adds one string custom field.
    #[must_use]
    pub fn custom_field_text(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.custom_fields
            .insert(name.into(), Value::String(value.into()));
        self
    }

    /// Adds one raw JSON custom field.
    #[must_use]
    pub fn custom_field_json(mut self, name: impl Into<String>, value: Value) -> Self {
        self.custom_fields.insert(name.into(), value);
        self
    }

    /// Sets the funding field.
    #[must_use]
    pub fn funding(mut self, funding: impl Into<String>) -> Self {
        self.funding = Some(funding.into());
        self
    }

    /// Sets the license ID.
    #[must_use]
    pub fn license_id(mut self, license: impl Into<LicenseId>) -> Self {
        self.license = Some(license.into());
        self
    }

    /// Sets the DOI.
    #[must_use]
    pub fn doi(mut self, doi: Doi) -> Self {
        self.doi = Some(doi);
        self
    }

    /// Sets the related resource DOI.
    #[must_use]
    pub fn resource_doi(mut self, resource_doi: impl Into<String>) -> Self {
        self.resource_doi = Some(resource_doi.into());
        self
    }

    /// Sets the related resource title.
    #[must_use]
    pub fn resource_title(mut self, resource_title: impl Into<String>) -> Self {
        self.resource_title = Some(resource_title.into());
        self
    }

    /// Finishes the builder.
    ///
    /// # Errors
    ///
    /// Returns an error if required fields are missing.
    pub fn build(self) -> Result<ArticleMetadata, ArticleMetadataBuildError> {
        let title = self.title.ok_or(ArticleMetadataBuildError::MissingTitle)?;
        let defined_type = self
            .defined_type
            .ok_or(ArticleMetadataBuildError::MissingDefinedType)?;

        Ok(ArticleMetadata {
            title,
            description: self.description,
            defined_type,
            tags: self.tags,
            keywords: self.keywords,
            references: self.references,
            categories: self.categories,
            authors: self.authors,
            custom_fields: self.custom_fields,
            funding: self.funding,
            license: self.license,
            doi: self.doi,
            resource_doi: self.resource_doi,
            resource_title: self.resource_title,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ArticleMetadata, ArticleMetadataBuildError, AuthorReference, DefinedType};

    #[test]
    fn defined_type_accepts_strings_and_ids() {
        let dataset_from_name: DefinedType = serde_json::from_str("\"dataset\"").unwrap();
        let dataset_from_id: DefinedType = serde_json::from_str("3").unwrap();

        assert_eq!(dataset_from_name, DefinedType::Dataset);
        assert_eq!(dataset_from_id, DefinedType::Dataset);
        assert_eq!(
            serde_json::to_string(&DefinedType::Dataset).unwrap(),
            "\"dataset\""
        );
    }

    #[test]
    fn metadata_builder_requires_title_and_defined_type() {
        assert_eq!(
            ArticleMetadata::builder().build().unwrap_err(),
            ArticleMetadataBuildError::MissingTitle
        );
        assert_eq!(
            ArticleMetadata::builder().title("x").build().unwrap_err(),
            ArticleMetadataBuildError::MissingDefinedType
        );
    }

    #[test]
    fn metadata_builder_serializes_expected_payload() {
        let metadata = ArticleMetadata::builder()
            .title("Example")
            .defined_type(DefinedType::Dataset)
            .description("Description")
            .tag("data")
            .keyword("science")
            .reference("https://example.com")
            .category_id(3)
            .author(AuthorReference::id(7))
            .author_named("Doe, Jane")
            .custom_field_text("location", "Amsterdam")
            .license_id(1)
            .resource_doi("10.1234/example")
            .build()
            .unwrap();

        let payload = metadata.to_payload();
        assert_eq!(payload["title"], "Example");
        assert_eq!(payload["defined_type"], "dataset");
        assert_eq!(payload["categories"][0], 3);
        assert_eq!(payload["authors"][0]["id"], 7);
        assert_eq!(payload["authors"][1]["name"], "Doe, Jane");
        assert_eq!(payload["custom_fields"]["location"], "Amsterdam");
    }
}
