//! Upload input types and file replacement policies.

use std::fmt;
use std::io::Read;
use std::path::PathBuf;

/// Policy for reconciling existing article files with new uploads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileReplacePolicy {
    /// Replace visible article files after the new uploads succeed.
    ReplaceAll,
    /// Replace files that share the same filename.
    UpsertByFilename,
    /// Keep existing files and add new uploads alongside them.
    KeepExistingAndAdd,
}

/// Source data for a single upload.
pub enum UploadSource {
    /// Upload from a local file path.
    Path(
        /// Local source path.
        PathBuf,
    ),
    /// Upload from a blocking reader with an explicit content length.
    Reader {
        /// Reader that produces the upload bytes.
        reader: Box<dyn Read + Send>,
        /// Exact number of bytes that the reader will produce.
        content_length: u64,
    },
}

impl fmt::Debug for UploadSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(path) => f.debug_tuple("Path").field(path).finish(),
            Self::Reader { content_length, .. } => f
                .debug_struct("Reader")
                .field("content_length", content_length)
                .finish_non_exhaustive(),
        }
    }
}

/// Specification for one file upload.
#[derive(Debug)]
pub struct UploadSpec {
    /// Filename to expose in Figshare.
    pub filename: String,
    /// Upload source.
    pub source: UploadSource,
}

impl UploadSpec {
    /// Builds an upload spec from a local path.
    ///
    /// # Errors
    ///
    /// Returns an error if the path does not contain a final filename segment.
    pub fn from_path(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path = path.into();
        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(path_without_filename_error)?;

        Ok(Self {
            filename,
            source: UploadSource::Path(path),
        })
    }

    /// Builds an upload spec from a reader and explicit metadata.
    #[must_use]
    pub fn from_reader(
        filename: impl Into<String>,
        reader: impl Read + Send + 'static,
        content_length: u64,
    ) -> Self {
        Self {
            filename: filename.into(),
            source: UploadSource::Reader {
                reader: Box::new(reader),
                content_length,
            },
        }
    }
}

fn path_without_filename_error() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        "path has no final file name segment",
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{path_without_filename_error, UploadSource, UploadSpec};

    #[test]
    fn path_upload_extracts_filename() {
        let spec = UploadSpec::from_path(PathBuf::from("/tmp/archive.tar.gz")).unwrap();
        assert_eq!(spec.filename, "archive.tar.gz");
    }

    #[test]
    fn path_upload_rejects_missing_filename() {
        let error = UploadSpec::from_path(PathBuf::from("/")).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test]
    fn missing_filename_error_has_stable_message() {
        let error = path_without_filename_error();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
        assert_eq!(error.to_string(), "path has no final file name segment");
    }

    #[test]
    fn reader_upload_debug_hides_reader() {
        let spec = UploadSpec::from_reader("artifact.bin", std::io::Cursor::new(vec![1, 2, 3]), 3);

        match spec.source {
            UploadSource::Reader { content_length, .. } => assert_eq!(content_length, 3),
            UploadSource::Path(_) => panic!("expected reader source"),
        }
        assert!(format!("{spec:?}").contains("artifact.bin"));
    }

    #[test]
    fn path_upload_debug_shows_path_variant() {
        let spec = UploadSpec::from_path(PathBuf::from("/tmp/archive.tar.gz")).unwrap();

        match &spec.source {
            UploadSource::Path(path) => assert_eq!(path, &PathBuf::from("/tmp/archive.tar.gz")),
            UploadSource::Reader { .. } => panic!("expected path source"),
        }
        assert!(format!("{:?}", spec.source).contains("archive.tar.gz"));
    }
}
