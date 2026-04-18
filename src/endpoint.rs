//! Endpoint selection for production or custom Figshare deployments.

use url::Url;

/// Base API endpoint used by the client.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Endpoint {
    /// The public Figshare production service.
    #[default]
    Production,
    /// A fully custom Figshare-compatible API base URL.
    Custom(
        /// Deployment root or base API URL, normalized to end in `/v2/` when
        /// no path is supplied.
        Url,
    ),
}

impl Endpoint {
    /// Returns the API base URL for this endpoint.
    ///
    /// # Errors
    ///
    /// Returns an error if the configured URL cannot be parsed into a valid
    /// base URL.
    ///
    /// # Examples
    ///
    /// ```
    /// use figshare_rs::Endpoint;
    ///
    /// let endpoint = Endpoint::Custom("http://localhost:1234".parse()?);
    /// assert_eq!(endpoint.base_url()?.as_str(), "http://localhost:1234/v2/");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn base_url(&self) -> Result<Url, url::ParseError> {
        match self {
            Self::Production => Url::parse("https://api.figshare.com/v2/"),
            Self::Custom(url) => Ok(normalize_base_url(url.clone())),
        }
    }
}

fn normalize_base_url(mut url: Url) -> Url {
    let path = url.path().trim_end_matches('/');
    let normalized = if path.is_empty() {
        "/v2/".to_owned()
    } else {
        format!("{path}/")
    };
    url.set_path(&normalized);
    url
}

#[cfg(test)]
mod tests {
    use super::Endpoint;
    use url::Url;

    #[test]
    fn uses_expected_production_url() {
        assert_eq!(
            Endpoint::Production.base_url().unwrap().as_str(),
            "https://api.figshare.com/v2/"
        );
    }

    #[test]
    fn preserves_custom_v2_url() {
        let url = Url::parse("http://localhost:1234/v2/").unwrap();
        assert_eq!(Endpoint::Custom(url.clone()).base_url().unwrap(), url);
    }

    #[test]
    fn normalizes_custom_url_without_trailing_slash() {
        let normalized = Endpoint::Custom(Url::parse("http://localhost:1234/v2").unwrap())
            .base_url()
            .unwrap();
        assert_eq!(normalized.as_str(), "http://localhost:1234/v2/");
    }

    #[test]
    fn normalizes_empty_custom_path_to_v2() {
        let normalized = Endpoint::Custom(Url::parse("http://localhost:1234").unwrap())
            .base_url()
            .unwrap();
        assert_eq!(normalized.as_str(), "http://localhost:1234/v2/");
    }
}
