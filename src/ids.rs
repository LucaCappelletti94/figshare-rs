//! Small identifier newtypes used throughout the public API.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::serde_util::deserialize_u64ish;

macro_rules! id_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(
            /// Raw numeric identifier returned by Figshare.
            pub u64
        );

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<u64> for $name {
            fn from(value: u64) -> Self {
                Self(value)
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_u64(self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserialize_u64ish(deserializer).map(Self)
            }
        }
    };
}

id_newtype!(
    /// Identifier for a Figshare article.
    ArticleId
);
id_newtype!(
    /// Identifier for a file attached to a Figshare article.
    FileId
);
id_newtype!(
    /// Identifier for a Figshare category.
    CategoryId
);
id_newtype!(
    /// Identifier for a Figshare license.
    LicenseId
);

/// DOI string wrapper used by public article selectors and response payloads.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct Doi(
    /// Raw DOI value.
    pub String,
);

/// Errors raised while parsing or validating DOI selectors.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum DoiError {
    /// The normalized DOI string was empty.
    #[error("DOI cannot be empty")]
    Empty,
    /// The DOI did not match the expected `10.<registrant>/<suffix>` shape.
    #[error("invalid DOI: {0}")]
    Invalid(String),
}

impl Doi {
    /// Creates a normalized DOI wrapper from a raw DOI-like input.
    ///
    /// # Errors
    ///
    /// Returns an error if the normalized value does not resemble a DOI.
    ///
    /// # Examples
    ///
    /// ```
    /// use figshare_rs::Doi;
    ///
    /// let doi = Doi::new(" HTTPS://DOI.ORG/10.6084/M9.FIGSHARE.123 ")?;
    /// assert_eq!(doi.as_str(), "10.6084/m9.figshare.123");
    /// # Ok::<(), figshare_rs::DoiError>(())
    /// ```
    pub fn new(value: impl AsRef<str>) -> Result<Self, DoiError> {
        let normalized = normalize_doi(value.as_ref());
        validate_doi(&normalized)?;
        Ok(Self(normalized))
    }

    /// Returns the raw DOI string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Doi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl TryFrom<String> for Doi {
    type Error = DoiError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<&str> for Doi {
    type Error = DoiError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl FromStr for Doi {
    type Err = DoiError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl<'de> Deserialize<'de> for Doi {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

fn normalize_doi(value: &str) -> String {
    let trimmed = value.trim();
    let without_prefix = trim_doi_prefix(trimmed);
    without_prefix.trim().to_ascii_lowercase()
}

fn trim_doi_prefix(value: &str) -> &str {
    const PREFIXES: [&str; 4] = [
        "doi:",
        "https://doi.org/",
        "http://doi.org/",
        "https://dx.doi.org/",
    ];

    for prefix in PREFIXES {
        if value.len() >= prefix.len() && value[..prefix.len()].eq_ignore_ascii_case(prefix) {
            return &value[prefix.len()..];
        }
    }

    value
}

fn validate_doi(value: &str) -> Result<(), DoiError> {
    if value.is_empty() {
        return Err(DoiError::Empty);
    }

    let Some((registrant, suffix)) = value.split_once('/') else {
        return Err(DoiError::Invalid(value.to_owned()));
    };

    if registrant.len() <= 3 || !registrant.starts_with("10.") || suffix.is_empty() {
        return Err(DoiError::Invalid(value.to_owned()));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ArticleId, CategoryId, Doi, DoiError, FileId, LicenseId};

    #[test]
    fn numeric_ids_deserialize_from_strings_and_numbers() {
        let article: ArticleId = serde_json::from_str("\"12\"").unwrap();
        let file: FileId = serde_json::from_str("13").unwrap();
        let category: CategoryId = serde_json::from_str("\"14\"").unwrap();
        let license: LicenseId = serde_json::from_str("15.0").unwrap();

        assert_eq!(article.0, 12);
        assert_eq!(file.0, 13);
        assert_eq!(category.0, 14);
        assert_eq!(license.0, 15);
    }

    #[test]
    fn doi_round_trips_through_display_and_parse() {
        let doi: Doi = "10.6084/m9.figshare.123".parse().unwrap();
        assert_eq!(doi.as_str(), "10.6084/m9.figshare.123");
        assert_eq!(doi.to_string(), "10.6084/m9.figshare.123");
    }

    #[test]
    fn doi_normalization_trims_prefixes_and_case() {
        assert_eq!(
            Doi::new("  HTTPS://DOI.ORG/10.6084/M9.FIGSHARE.123  ")
                .unwrap()
                .as_str(),
            "10.6084/m9.figshare.123"
        );
        assert_eq!(
            Doi::new("doi:10.6084/M9.FIGSHARE.456").unwrap().as_str(),
            "10.6084/m9.figshare.456"
        );
    }

    #[test]
    fn doi_validation_rejects_empty_or_invalid_values() {
        assert_eq!(Doi::new("  ").unwrap_err(), DoiError::Empty);
        assert!(matches!(
            Doi::new("figshare.123").unwrap_err(),
            DoiError::Invalid(value) if value == "figshare.123"
        ));
    }
}
