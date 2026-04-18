//! Core data models for articles, files, licenses, uploads, and related payloads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use crate::ids::{ArticleId, CategoryId, Doi, FileId, LicenseId};
use crate::metadata::DefinedType;
use crate::serde_util::{
    deserialize_boolish, deserialize_option_boolish, deserialize_option_u64ish, deserialize_u64ish,
};

macro_rules! string_enum {
    ($(#[$enum_meta:meta])* $name:ident { $($(#[$variant_meta:meta])* $variant:ident => $value:literal),+ $(,)? }) => {
        $(#[$enum_meta])*
        #[derive(Clone, Debug, PartialEq, Eq)]
        #[non_exhaustive]
        pub enum $name {
            $($(#[$variant_meta])* $variant,)+
            /// A server value unknown to this crate version.
            Unknown(
                /// Raw server value.
                String
            ),
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(match self {
                    $(Self::$variant => $value,)+
                    Self::Unknown(value) => value.as_str(),
                })
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Ok(match value.as_str() {
                    $($value => Self::$variant,)+
                    _ => Self::Unknown(value),
                })
            }
        }
    };
}

string_enum!(
    /// Private/public article status.
    ArticleStatus {
        /// Draft article.
        Draft => "draft",
        /// Published article.
        Public => "public"
    }
);

string_enum!(
    /// Article-level or file-level embargo mode.
    ArticleEmbargo {
        /// Whole-article embargo.
        Article => "article",
        /// File-level embargo.
        File => "file"
    }
);

string_enum!(
    /// File status as reported by Figshare.
    FileStatus {
        /// Newly created file entry.
        Created => "created",
        /// Available file.
        Available => "available"
    }
);

string_enum!(
    /// Upload session status from the upload service.
    UploadStatus {
        /// Waiting for parts to be uploaded.
        Pending => "PENDING",
        /// Upload assembly completed.
        Completed => "COMPLETED",
        /// Upload aborted.
        Aborted => "ABORTED"
    }
);

string_enum!(
    /// Upload part status from the upload service.
    UploadPartStatus {
        /// Waiting for part bytes.
        Pending => "PENDING",
        /// Part bytes uploaded successfully.
        Complete => "COMPLETE"
    }
);

/// Category representation returned by Figshare.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArticleCategory {
    /// Parent category ID, when present.
    #[serde(
        default,
        deserialize_with = "deserialize_option_u64ish",
        skip_serializing_if = "Option::is_none"
    )]
    pub parent_id: Option<u64>,
    /// Category identifier.
    pub id: CategoryId,
    /// Category title.
    pub title: String,
    /// Additional untyped fields preserved for forward compatibility.
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

/// License representation returned by Figshare.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArticleLicense {
    /// License identifier.
    #[serde(rename = "value")]
    pub id: LicenseId,
    /// License short name.
    pub name: String,
    /// License documentation URL, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<Url>,
    /// Additional untyped fields preserved for forward compatibility.
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

/// Custom field entry returned on detailed article payloads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct CustomField {
    /// Custom field name.
    pub name: String,
    /// Custom field value.
    pub value: Value,
    /// Whether the field is mandatory, when present.
    #[serde(
        default,
        deserialize_with = "deserialize_option_boolish",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_mandatory: Option<bool>,
    /// Additional untyped fields preserved for forward compatibility.
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

/// Author representation returned on detailed article payloads.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct ArticleAuthor {
    /// Author identifier, when present.
    #[serde(
        default,
        deserialize_with = "deserialize_option_u64ish",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<u64>,
    /// Full display name, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub full_name: Option<String>,
    /// Alternate display name field, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Figshare URL slug, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_name: Option<String>,
    /// ORCID identifier, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orcid_id: Option<String>,
    /// Additional untyped fields preserved for forward compatibility.
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

impl ArticleAuthor {
    /// Returns the best display name available for the author.
    #[must_use]
    pub fn display_name(&self) -> Option<&str> {
        self.full_name.as_deref().or(self.name.as_deref())
    }
}

/// Public/private file representation attached to an article.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArticleFile {
    /// File identifier.
    pub id: FileId,
    /// File name.
    pub name: String,
    /// File size in bytes.
    #[serde(default, deserialize_with = "deserialize_u64ish")]
    pub size: u64,
    /// Whether the file is only linked and not stored on Figshare.
    #[serde(
        default,
        deserialize_with = "deserialize_option_boolish",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_link_only: Option<bool>,
    /// Public or private file download URL, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_url: Option<Url>,
    /// Private file status, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<FileStatus>,
    /// Viewer type hint, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub viewer_type: Option<String>,
    /// Preview state hint, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_state: Option<String>,
    /// Upload session URL, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload_url: Option<Url>,
    /// Upload token, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload_token: Option<String>,
    /// Client-provided MD5 used when initiating the upload, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supplied_md5: Option<String>,
    /// Server-computed MD5, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub computed_md5: Option<String>,
    /// Additional untyped fields preserved for forward compatibility.
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

impl ArticleFile {
    /// Returns the upload session URL, when the file is hosted on Figshare.
    #[must_use]
    pub fn upload_session_url(&self) -> Option<&Url> {
        self.upload_url.as_ref()
    }
}

/// One public article version pointer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArticleVersion {
    /// Version number.
    #[serde(default, deserialize_with = "deserialize_u64ish")]
    pub version: u64,
    /// API URL for the version resource.
    pub url: Url,
    /// Additional untyped fields preserved for forward compatibility.
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

/// Article payload shared across public and own article endpoints.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Article {
    /// Article identifier.
    pub id: ArticleId,
    /// Article title.
    pub title: String,
    /// Version-specific DOI, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doi: Option<Doi>,
    /// Group identifier, when present.
    #[serde(
        default,
        deserialize_with = "deserialize_option_u64ish",
        skip_serializing_if = "Option::is_none"
    )]
    pub group_id: Option<u64>,
    /// Figshare-provided article URL, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<Url>,
    /// Public HTML URL, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_public_html: Option<Url>,
    /// Public API URL, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_public_api: Option<Url>,
    /// Private HTML URL, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_private_html: Option<Url>,
    /// Private API URL, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url_private_api: Option<Url>,
    /// Public Figshare landing page, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub figshare_url: Option<Url>,
    /// Publication timestamp as returned by Figshare, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_date: Option<String>,
    /// Last modification timestamp as returned by Figshare, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_date: Option<String>,
    /// Creation timestamp as returned by Figshare, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_date: Option<String>,
    /// Preview thumbnail URL, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb: Option<Url>,
    /// Typed article kind, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defined_type: Option<DefinedType>,
    /// Related resource title, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_title: Option<String>,
    /// Related resource DOI, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_doi: Option<String>,
    /// Citation string, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub citation: Option<String>,
    /// Confidentiality reason, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidential_reason: Option<String>,
    /// Embargo mode, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embargo_type: Option<ArticleEmbargo>,
    /// Whether the article is confidential, when present.
    #[serde(
        default,
        deserialize_with = "deserialize_option_boolish",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_confidential: Option<bool>,
    /// Total article size in bytes, when present.
    #[serde(
        default,
        deserialize_with = "deserialize_option_u64ish",
        skip_serializing_if = "Option::is_none"
    )]
    pub size: Option<u64>,
    /// Funding string, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub funding: Option<String>,
    /// Tags, when present.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Version number, when present.
    #[serde(
        default,
        deserialize_with = "deserialize_option_u64ish",
        skip_serializing_if = "Option::is_none"
    )]
    pub version: Option<u64>,
    /// Whether the article is active, when present.
    #[serde(
        default,
        deserialize_with = "deserialize_option_boolish",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_active: Option<bool>,
    /// Whether the article is only a metadata record, when present.
    #[serde(
        default,
        deserialize_with = "deserialize_option_boolish",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_metadata_record: Option<bool>,
    /// Metadata reason, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata_reason: Option<String>,
    /// Article status, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<ArticleStatus>,
    /// Description, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Whether the article is embargoed, when present.
    #[serde(
        default,
        deserialize_with = "deserialize_option_boolish",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_embargoed: Option<bool>,
    /// Embargo end timestamp, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub embargo_date: Option<String>,
    /// Whether the article is public, when present.
    #[serde(
        default,
        deserialize_with = "deserialize_option_boolish",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_public: Option<bool>,
    /// Whether the article contains a linked file, when present.
    #[serde(
        default,
        deserialize_with = "deserialize_option_boolish",
        skip_serializing_if = "Option::is_none"
    )]
    pub has_linked_file: Option<bool>,
    /// Attached categories.
    #[serde(default)]
    pub categories: Vec<ArticleCategory>,
    /// Attached license, when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<ArticleLicense>,
    /// Attached references.
    #[serde(default)]
    pub references: Vec<String>,
    /// Attached files embedded in the article payload, when present.
    ///
    /// Figshare caps embedded article file lists, so use dedicated file-list
    /// endpoints or the download helpers when complete enumeration matters.
    #[serde(default)]
    pub files: Vec<ArticleFile>,
    /// Attached authors, when present.
    #[serde(default)]
    pub authors: Vec<ArticleAuthor>,
    /// Attached custom fields, when present.
    #[serde(default)]
    pub custom_fields: Vec<CustomField>,
    /// Additional untyped fields preserved for forward compatibility.
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

impl Article {
    /// Returns `true` when the article is public according to the available
    /// flags in the payload.
    #[must_use]
    pub fn is_public_article(&self) -> bool {
        self.is_public.unwrap_or_else(|| {
            self.status
                .as_ref()
                .is_some_and(|status| matches!(status, ArticleStatus::Public))
                || self.published_date.is_some()
        })
    }

    /// Returns the best version number visible in the payload.
    #[must_use]
    pub fn version_number(&self) -> Option<u64> {
        self.version
    }

    /// Finds a file by exact file name within the embedded article payload.
    ///
    /// Figshare may return only a partial embedded file list, so prefer the
    /// dedicated file-list endpoints when completeness matters.
    ///
    /// # Examples
    ///
    /// ```
    /// use figshare_rs::{Article, FileId};
    ///
    /// let article: Article = serde_json::from_value(serde_json::json!({
    ///     "id": 1,
    ///     "title": "Example",
    ///     "files": [{
    ///         "id": 7,
    ///         "name": "artifact.bin",
    ///         "size": 12
    ///     }]
    /// }))?;
    ///
    /// let file = article.file_by_name("artifact.bin").expect("embedded file");
    /// assert_eq!(file.id, FileId(7));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn file_by_name(&self, name: &str) -> Option<&ArticleFile> {
        self.files.iter().find(|file| file.name == name)
    }

    /// Finds a file by ID.
    #[must_use]
    pub fn file_by_id(&self, id: FileId) -> Option<&ArticleFile> {
        self.files.iter().find(|file| file.id == id)
    }
}

/// Upload session information returned by the upload service.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UploadSession {
    /// Upload token.
    pub token: String,
    /// Target file name.
    pub name: String,
    /// Total file size in bytes.
    #[serde(default, deserialize_with = "deserialize_u64ish")]
    pub size: u64,
    /// Expected MD5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub md5: Option<String>,
    /// Upload status.
    pub status: UploadStatus,
    /// Upload parts.
    #[serde(default)]
    pub parts: Vec<UploadPart>,
    /// Additional untyped fields preserved for forward compatibility.
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

impl UploadSession {
    /// Returns `true` when the upload completed successfully.
    #[must_use]
    pub fn is_completed(&self) -> bool {
        matches!(self.status, UploadStatus::Completed)
    }
}

/// One upload part.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UploadPart {
    /// Part number.
    #[serde(rename = "partNo", default, deserialize_with = "deserialize_u64ish")]
    pub part_no: u64,
    /// Inclusive start offset.
    #[serde(
        rename = "startOffset",
        default,
        deserialize_with = "deserialize_u64ish"
    )]
    pub start_offset: u64,
    /// Inclusive end offset.
    #[serde(rename = "endOffset", default, deserialize_with = "deserialize_u64ish")]
    pub end_offset: u64,
    /// Part status.
    pub status: UploadPartStatus,
    /// Whether the part is locked.
    #[serde(default, deserialize_with = "deserialize_boolish")]
    pub locked: bool,
    /// Additional untyped fields preserved for forward compatibility.
    #[serde(flatten, default)]
    pub extra: BTreeMap<String, Value>,
}

impl UploadPart {
    /// Returns the exact byte length of the part.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.end_offset - self.start_offset + 1
    }

    /// Returns whether the part describes an empty byte range.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.end_offset < self.start_offset
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Article, ArticleAuthor, ArticleFile, ArticleStatus, FileStatus, UploadPart,
        UploadPartStatus, UploadSession, UploadStatus,
    };
    use crate::metadata::DefinedType;
    use serde_json::json;

    #[test]
    fn article_preserves_unknown_fields_and_flexible_wire_types() {
        let article: Article = serde_json::from_value(json!({
            "id": "42",
            "title": "Example",
            "defined_type": 3,
            "is_public": 1,
            "files": [{
                "id": 7,
                "name": "artifact.bin",
                "size": "12",
                "status": "created",
                "is_link_only": 0
            }],
            "mystery": "value"
        }))
        .unwrap();

        assert_eq!(article.id.0, 42);
        assert_eq!(article.defined_type, Some(DefinedType::Dataset));
        assert!(article.is_public_article());
        assert_eq!(article.files[0].size, 12);
        assert_eq!(article.extra.get("mystery"), Some(&json!("value")));
    }

    #[test]
    fn article_helpers_find_files_and_flags() {
        let article: Article = serde_json::from_value(json!({
            "id": 10,
            "title": "Example",
            "status": "public",
            "version": "3",
            "files": [{
                "id": 8,
                "name": "artifact.bin",
                "size": 5
            }]
        }))
        .unwrap();

        assert!(article.is_public_article());
        assert_eq!(article.version_number(), Some(3));
        assert!(article.file_by_name("artifact.bin").is_some());
        assert!(article.file_by_id(crate::FileId(8)).is_some());
    }

    #[test]
    fn author_display_name_uses_best_available_field() {
        let author = ArticleAuthor {
            full_name: Some("Doe, Jane".into()),
            ..ArticleAuthor::default()
        };
        assert_eq!(author.display_name(), Some("Doe, Jane"));
    }

    #[test]
    fn upload_models_deserialize_and_expose_helpers() {
        let session: UploadSession = serde_json::from_value(json!({
            "token": "upload-token",
            "name": "artifact.bin",
            "size": 4,
            "md5": "abcd",
            "status": "COMPLETED",
            "parts": [{
                "partNo": 1,
                "startOffset": 0,
                "endOffset": 3,
                "status": "COMPLETE",
                "locked": false
            }]
        }))
        .unwrap();

        assert!(session.is_completed());
        assert_eq!(session.parts[0].len(), 4);
    }

    #[test]
    fn string_enums_preserve_unknown_values() {
        let status: ArticleStatus = serde_json::from_value(json!("queued")).unwrap();
        let file_status: FileStatus = serde_json::from_value(json!("processing")).unwrap();
        let upload_status: UploadStatus = serde_json::from_value(json!("SOMETHING")).unwrap();
        let part_status: UploadPartStatus = serde_json::from_value(json!("WAITING")).unwrap();

        assert!(matches!(status, ArticleStatus::Unknown(value) if value == "queued"));
        assert!(matches!(file_status, FileStatus::Unknown(value) if value == "processing"));
        assert!(matches!(upload_status, UploadStatus::Unknown(value) if value == "SOMETHING"));
        assert!(matches!(part_status, UploadPartStatus::Unknown(value) if value == "WAITING"));
    }

    #[test]
    fn file_and_upload_parts_accept_boolish_fields() {
        let file: ArticleFile = serde_json::from_value(json!({
            "id": 22,
            "name": "artifact.bin",
            "size": 3,
            "is_link_only": "0"
        }))
        .unwrap();
        let part: UploadPart = serde_json::from_value(json!({
            "partNo": 1,
            "startOffset": 4,
            "endOffset": 7,
            "status": "PENDING",
            "locked": "1"
        }))
        .unwrap();

        assert_eq!(file.is_link_only, Some(false));
        assert!(part.locked);
    }
}
