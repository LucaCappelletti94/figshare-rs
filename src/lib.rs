#![doc = include_str!("../README.md")]
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(
    clippy::all,
    clippy::pedantic,
    clippy::expect_used,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::unwrap_used
)]
#![allow(clippy::module_name_repetitions)]
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::missing_errors_doc,
        clippy::missing_panics_doc,
        clippy::panic,
        clippy::unwrap_used
    )
)]

pub mod client;
pub mod downloads;
pub mod endpoint;
pub mod error;
pub mod ids;
pub mod metadata;
pub mod model;
pub mod poll;
pub mod query;
mod serde_util;
pub mod upload;
pub mod workflow;
pub use client::{Auth, FigshareClient, FigshareClientBuilder};
pub use downloads::{DownloadStream, ResolvedDownload};
pub use endpoint::Endpoint;
pub use error::{FieldError, FigshareError};
pub use ids::{ArticleId, CategoryId, Doi, DoiError, FileId, LicenseId};
pub use metadata::{
    ArticleMetadata, ArticleMetadataBuildError, ArticleMetadataBuilder, AuthorReference,
    DefinedType,
};
pub use model::{
    Article, ArticleAuthor, ArticleCategory, ArticleEmbargo, ArticleFile, ArticleLicense,
    ArticleStatus, ArticleVersion, CustomField, FileStatus, UploadPart, UploadPartStatus,
    UploadSession, UploadStatus,
};
pub use poll::PollOptions;
pub use query::{ArticleOrder, ArticleQuery, ArticleQueryBuilder, OrderDirection};
pub use upload::{FileReplacePolicy, UploadSource, UploadSpec};
pub use workflow::PublishedArticle;
