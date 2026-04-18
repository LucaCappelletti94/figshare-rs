//! Error types and HTTP error decoding for Figshare responses.

use reqwest::{Response, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

/// Field-specific validation error returned by Figshare.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldError {
    /// The field name, when Figshare reports one.
    #[serde(default)]
    pub field: Option<String>,
    /// Human-readable error message for the field.
    pub message: String,
}

/// Errors produced by the Figshare client.
#[derive(Debug, Error)]
pub enum FigshareError {
    /// Figshare returned a non-success HTTP status.
    #[error("Figshare returned HTTP {status}: {message:?}")]
    Http {
        /// HTTP status returned by Figshare.
        status: StatusCode,
        /// Summary message extracted from the response body, when available.
        message: Option<String>,
        /// Machine-readable error code returned by Figshare, when available.
        code: Option<String>,
        /// Field-level validation errors extracted from the response body.
        field_errors: Vec<FieldError>,
        /// Trimmed raw response body for diagnostics.
        raw_body: Option<String>,
    },
    /// A transport error occurred while sending or receiving a request.
    #[error(transparent)]
    Transport(
        /// Underlying transport error.
        #[from]
        reqwest::Error,
    ),
    /// JSON serialization or deserialization failed.
    #[error(transparent)]
    Json(
        /// Underlying JSON error.
        #[from]
        serde_json::Error,
    ),
    /// A local I/O operation failed.
    #[error(transparent)]
    Io(
        /// Underlying I/O error.
        #[from]
        std::io::Error,
    ),
    /// A URL could not be parsed or joined.
    #[error(transparent)]
    Url(
        /// Underlying URL parse error.
        #[from]
        url::ParseError,
    ),
    /// A required environment variable could not be read.
    #[error("failed to read environment variable {name}: {source}")]
    EnvVar {
        /// Environment variable name.
        name: String,
        /// Underlying environment lookup error.
        #[source]
        source: std::env::VarError,
    },
    /// Authentication was required for a private operation.
    #[error("authentication required for {0}")]
    MissingAuth(
        /// Description of the private operation.
        &'static str,
    ),
    /// Figshare returned data that violates a workflow invariant.
    #[error("invalid Figshare state: {0}")]
    InvalidState(
        /// Description of the invalid state.
        String,
    ),
    /// A required link relation was missing from a Figshare payload.
    #[error("missing Figshare link: {0}")]
    MissingLink(
        /// Missing link relation name.
        &'static str,
    ),
    /// A requested file name was not present on an article.
    #[error("missing article file: {name}")]
    MissingFile {
        /// Missing article file name.
        name: String,
    },
    /// Multiple uploads targeted the same final filename.
    #[error("duplicate upload filename: {filename}")]
    DuplicateUploadFilename {
        /// Duplicate filename seen in the upload set.
        filename: String,
    },
    /// A keep-existing upload would overwrite an existing draft file.
    #[error("article already contains file and replacement policy forbids overwrite: {filename}")]
    ConflictingDraftFile {
        /// Conflicting filename already present on the article.
        filename: String,
    },
    /// A selector could not be resolved to an article.
    #[error("unsupported selector: {0}")]
    UnsupportedSelector(
        /// Description of the unsupported selector.
        String,
    ),
    /// Polling timed out before Figshare reached the requested state.
    #[error("timed out waiting for Figshare {0}")]
    Timeout(
        /// Label for the operation that timed out.
        &'static str,
    ),
}

impl FigshareError {
    pub(crate) async fn from_response(response: Response) -> Self {
        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);

        let body = match response.bytes().await {
            Ok(body) => body,
            Err(error) => return Self::Transport(error),
        };

        decode_http_error(status, content_type.as_deref(), &body)
    }
}

pub(crate) fn decode_http_error(
    status: StatusCode,
    content_type: Option<&str>,
    body: &[u8],
) -> FigshareError {
    let raw_body = trimmed_body(body);
    let parsed = if looks_like_json(content_type, body) {
        parse_json_error(body)
    } else {
        None
    };

    let (message, code, field_errors) = match parsed {
        Some((message, code, field_errors)) => (message, code, field_errors),
        None => (raw_body.clone(), None, Vec::new()),
    };

    FigshareError::Http {
        status,
        message,
        code,
        field_errors,
        raw_body,
    }
}

fn looks_like_json(content_type: Option<&str>, body: &[u8]) -> bool {
    if content_type
        .is_some_and(|value| value.starts_with("application/json") || value.ends_with("+json"))
    {
        return true;
    }

    body.iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| matches!(byte, b'{' | b'['))
}

fn parse_json_error(body: &[u8]) -> Option<(Option<String>, Option<String>, Vec<FieldError>)> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let message = value
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let code = value.get("code").and_then(Value::as_str).map(str::to_owned);
    let field_errors = value
        .get("errors")
        .and_then(parse_field_errors)
        .or_else(|| value.get("data").and_then(parse_field_errors))
        .unwrap_or_default();

    Some((message, code, field_errors))
}

fn parse_field_errors(value: &Value) -> Option<Vec<FieldError>> {
    match value {
        Value::Array(items) => {
            let mut errors = Vec::new();
            for item in items {
                match item {
                    Value::Object(map) => {
                        let message = map
                            .get("message")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .or_else(|| {
                                map.get("detail").and_then(Value::as_str).map(str::to_owned)
                            })
                            .unwrap_or_else(|| "unknown error".to_owned());
                        errors.push(FieldError {
                            field: map.get("field").and_then(Value::as_str).map(str::to_owned),
                            message,
                        });
                    }
                    Value::String(message) => errors.push(FieldError {
                        field: None,
                        message: message.clone(),
                    }),
                    _ => {}
                }
            }
            Some(errors)
        }
        Value::Object(map) => {
            let mut errors = Vec::new();
            for (field, message) in map {
                let message = if let Some(message) = message.as_str() {
                    message.to_owned()
                } else {
                    message.to_string()
                };
                errors.push(FieldError {
                    field: Some(field.clone()),
                    message,
                });
            }
            Some(errors)
        }
        _ => None,
    }
}

fn trimmed_body(body: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(body);
    for line in text.lines().map(str::trim) {
        if !line.is_empty() {
            return Some(line.chars().take(512).collect());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::{decode_http_error, parse_field_errors, parse_json_error, trimmed_body};
    use reqwest::StatusCode;
    use serde_json::json;

    #[test]
    fn parses_json_error_bodies() {
        let error = decode_http_error(
            StatusCode::BAD_REQUEST,
            Some("application/json"),
            br#"{"message":"bad metadata","code":"ValidationFailed","data":{"title":"required"}}"#,
        );

        match error {
            super::FigshareError::Http {
                message,
                code,
                field_errors,
                ..
            } => {
                assert_eq!(message.as_deref(), Some("bad metadata"));
                assert_eq!(code.as_deref(), Some("ValidationFailed"));
                assert_eq!(field_errors.len(), 1);
                assert_eq!(field_errors[0].field.as_deref(), Some("title"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn parses_plaintext_error_bodies() {
        let error = decode_http_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            Some("text/plain"),
            b"upstream exploded\nstack trace omitted",
        );

        match error {
            super::FigshareError::Http { message, .. } => {
                assert_eq!(message.as_deref(), Some("upstream exploded"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn parses_mixed_error_shapes() {
        let parsed =
            parse_json_error(br#"{"message":"bad","errors":["first",{"field":"x"}]}"#).unwrap();
        assert_eq!(parsed.0.as_deref(), Some("bad"));
        assert_eq!(parsed.2.len(), 2);

        let object_errors = parse_field_errors(&json!({
            "metadata.title": { "detail": "required" }
        }))
        .unwrap();
        assert_eq!(object_errors[0].field.as_deref(), Some("metadata.title"));
        assert_eq!(object_errors[0].message, r#"{"detail":"required"}"#);
    }

    #[test]
    fn parses_non_json_and_empty_bodies() {
        let malformed = decode_http_error(
            StatusCode::BAD_REQUEST,
            Some("application/json"),
            br#"{"broken":"json""#,
        );
        match malformed {
            super::FigshareError::Http {
                message, raw_body, ..
            } => {
                assert_eq!(message.as_deref(), Some(r#"{"broken":"json""#));
                assert_eq!(raw_body.as_deref(), Some(r#"{"broken":"json""#));
            }
            other => panic!("unexpected error: {other:?}"),
        }

        let empty = decode_http_error(StatusCode::BAD_GATEWAY, Some("text/plain"), b"   ");
        match empty {
            super::FigshareError::Http {
                message, raw_body, ..
            } => {
                assert_eq!(message, None);
                assert_eq!(raw_body, None);
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn trimmed_body_keeps_first_non_empty_line() {
        assert_eq!(
            trimmed_body(b"   \n  first line  \nsecond line"),
            Some("first line".into())
        );
    }

    #[tokio::test]
    async fn from_response_decodes_reqwest_response() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0_u8; 1024];
            let _ = stream.read(&mut buffer).await;
            let body = br#"{"message":"bad","code":"BadThing","data":{"field":"problem"}}"#;
            let response = format!(
                "HTTP/1.1 422 Unprocessable Entity\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.write_all(body).await;
            let _ = stream.write_all(b"\r\n").await;
            let _ = stream.shutdown().await;
        });

        let response = reqwest::get(format!("http://{address}/")).await.unwrap();
        let error = super::FigshareError::from_response(response).await;

        match error {
            super::FigshareError::Http {
                status,
                message,
                code,
                field_errors,
                ..
            } => {
                assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
                assert_eq!(message.as_deref(), Some("bad"));
                assert_eq!(code.as_deref(), Some("BadThing"));
                assert_eq!(field_errors[0].field.as_deref(), Some("field"));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }
}
