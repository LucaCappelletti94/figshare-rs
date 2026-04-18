use std::fmt;

use serde::de::{IntoDeserializer, Visitor};
use serde::Deserializer;

/// Deserializes an integer field while tolerating integer-like float and
/// numeric string wire shapes from Figshare.
pub(crate) fn deserialize_u64ish<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    struct U64ishVisitor;

    impl Visitor<'_> for U64ishVisitor {
        type Value = u64;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a non-negative integer, integer-like float, or numeric string")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(value)
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            u64::try_from(value).map_err(E::custom)
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if !value.is_finite() || value.fract() != 0.0 || value < 0.0 {
                return Err(E::custom("expected an integer-like numeric value"));
            }

            value.to_string().parse::<u64>().map_err(E::custom)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            value.parse::<u64>().map_err(E::custom)
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            self.visit_str(&value)
        }
    }

    deserializer.deserialize_any(U64ishVisitor)
}

/// Deserializes an optional integer field while tolerating integer-like float
/// and numeric string wire shapes from Figshare.
pub(crate) fn deserialize_option_u64ish<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    struct OptionalU64ishVisitor;

    impl<'de> Visitor<'de> for OptionalU64ishVisitor {
        type Value = Option<u64>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(
                "an optional non-negative integer, integer-like float, or numeric string",
            )
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserialize_u64ish(deserializer).map(Some)
        }
    }

    deserializer.deserialize_option(OptionalU64ishVisitor)
}

/// Deserializes a string field that Figshare sometimes emits as either a
/// string or an integer-like value.
#[allow(dead_code)]
pub(crate) fn deserialize_stringish<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    struct StringishVisitor;

    impl Visitor<'_> for StringishVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a string, integer, or integer-like float")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
            Ok(value.to_string())
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            u64::try_from(value)
                .map(|value| value.to_string())
                .map_err(E::custom)
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            deserialize_u64ish(value.into_deserializer()).map(|value| value.to_string())
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
            Ok(value.to_owned())
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
            Ok(value)
        }
    }

    deserializer.deserialize_any(StringishVisitor)
}

/// Deserializes a bool field that Figshare sometimes emits as `true`/`false`,
/// `1`/`0`, or string equivalents.
pub(crate) fn deserialize_boolish<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    struct BoolishVisitor;

    impl Visitor<'_> for BoolishVisitor {
        type Value = bool;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a bool, 0/1 integer, or boolean-like string")
        }

        fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
            Ok(value)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            match value {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(E::custom(
                    "expected 0 or 1 for a boolean-like numeric value",
                )),
            }
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            match value {
                0 => Ok(false),
                1 => Ok(true),
                _ => Err(E::custom(
                    "expected 0 or 1 for a boolean-like numeric value",
                )),
            }
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            match value.trim().to_ascii_lowercase().as_str() {
                "true" | "1" => Ok(true),
                "false" | "0" => Ok(false),
                _ => Err(E::custom("expected a boolean-like string")),
            }
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            self.visit_str(&value)
        }
    }

    deserializer.deserialize_any(BoolishVisitor)
}

/// Deserializes an optional bool field while tolerating integer and string
/// shapes from Figshare.
pub(crate) fn deserialize_option_boolish<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: Deserializer<'de>,
{
    struct OptionalBoolishVisitor;

    impl<'de> Visitor<'de> for OptionalBoolishVisitor {
        type Value = Option<bool>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an optional boolean-like value")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E> {
            Ok(None)
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserialize_boolish(deserializer).map(Some)
        }
    }

    deserializer.deserialize_option(OptionalBoolishVisitor)
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::{
        deserialize_boolish, deserialize_option_boolish, deserialize_option_u64ish,
        deserialize_stringish, deserialize_u64ish,
    };

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct U64Holder {
        #[serde(deserialize_with = "deserialize_u64ish")]
        value: u64,
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct OptionalU64Holder {
        #[serde(default, deserialize_with = "deserialize_option_u64ish")]
        value: Option<u64>,
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct StringHolder {
        #[serde(deserialize_with = "deserialize_stringish")]
        value: String,
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct BoolHolder {
        #[serde(deserialize_with = "deserialize_boolish")]
        value: bool,
    }

    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct OptionalBoolHolder {
        #[serde(default, deserialize_with = "deserialize_option_boolish")]
        value: Option<bool>,
    }

    #[test]
    fn u64ish_accepts_integer_like_values() {
        assert_eq!(
            serde_json::from_value::<U64Holder>(serde_json::json!({ "value": 12 }))
                .unwrap()
                .value,
            12
        );
        assert_eq!(
            serde_json::from_value::<U64Holder>(serde_json::json!({ "value": 13.0 }))
                .unwrap()
                .value,
            13
        );
        assert_eq!(
            serde_json::from_value::<U64Holder>(serde_json::json!({ "value": "14" }))
                .unwrap()
                .value,
            14
        );
    }

    #[test]
    fn optional_u64ish_handles_none_and_values() {
        assert_eq!(
            serde_json::from_value::<OptionalU64Holder>(serde_json::json!({})).unwrap(),
            OptionalU64Holder { value: None }
        );
        assert_eq!(
            serde_json::from_value::<OptionalU64Holder>(serde_json::json!({ "value": "15" }))
                .unwrap(),
            OptionalU64Holder { value: Some(15) }
        );
    }

    #[test]
    fn stringish_accepts_strings_and_integer_like_numbers() {
        assert_eq!(
            serde_json::from_value::<StringHolder>(serde_json::json!({ "value": "abc" })).unwrap(),
            StringHolder {
                value: "abc".into()
            }
        );
        assert_eq!(
            serde_json::from_value::<StringHolder>(serde_json::json!({ "value": 16.0 })).unwrap(),
            StringHolder { value: "16".into() }
        );
    }

    #[test]
    fn boolish_accepts_bool_numeric_and_string_values() {
        assert_eq!(
            serde_json::from_value::<BoolHolder>(serde_json::json!({ "value": true })).unwrap(),
            BoolHolder { value: true }
        );
        assert_eq!(
            serde_json::from_value::<BoolHolder>(serde_json::json!({ "value": 1 })).unwrap(),
            BoolHolder { value: true }
        );
        assert_eq!(
            serde_json::from_value::<BoolHolder>(serde_json::json!({ "value": "false" })).unwrap(),
            BoolHolder { value: false }
        );
    }

    #[test]
    fn optional_boolish_handles_none_and_values() {
        assert_eq!(
            serde_json::from_value::<OptionalBoolHolder>(serde_json::json!({})).unwrap(),
            OptionalBoolHolder { value: None }
        );
        assert_eq!(
            serde_json::from_value::<OptionalBoolHolder>(serde_json::json!({ "value": "1" }))
                .unwrap(),
            OptionalBoolHolder { value: Some(true) }
        );
    }

    #[test]
    fn u64ish_rejects_invalid_values() {
        assert!(serde_json::from_value::<U64Holder>(serde_json::json!({ "value": -1 })).is_err());
        assert!(serde_json::from_value::<U64Holder>(serde_json::json!({ "value": 1.25 })).is_err());
        assert!(
            serde_json::from_value::<U64Holder>(serde_json::json!({ "value": "nope" })).is_err()
        );
    }

    #[test]
    fn optional_u64ish_handles_null_and_invalid_values() {
        assert_eq!(
            serde_json::from_value::<OptionalU64Holder>(serde_json::json!({ "value": null }))
                .unwrap(),
            OptionalU64Holder { value: None }
        );
        assert!(
            serde_json::from_value::<OptionalU64Holder>(serde_json::json!({ "value": -2 }))
                .is_err()
        );
    }

    #[test]
    fn stringish_covers_integer_and_invalid_negative_values() {
        assert_eq!(
            serde_json::from_value::<StringHolder>(serde_json::json!({ "value": 17 })).unwrap(),
            StringHolder { value: "17".into() }
        );
        assert!(
            serde_json::from_value::<StringHolder>(serde_json::json!({ "value": -1 })).is_err()
        );
    }

    #[test]
    fn boolish_rejects_invalid_numeric_and_string_values() {
        assert!(serde_json::from_value::<BoolHolder>(serde_json::json!({ "value": 2 })).is_err());
        assert!(
            serde_json::from_value::<BoolHolder>(serde_json::json!({ "value": "yes" })).is_err()
        );
    }

    #[test]
    fn optional_boolish_handles_null_and_invalid_values() {
        assert_eq!(
            serde_json::from_value::<OptionalBoolHolder>(serde_json::json!({ "value": null }))
                .unwrap(),
            OptionalBoolHolder { value: None }
        );
        assert!(
            serde_json::from_value::<OptionalBoolHolder>(serde_json::json!({ "value": 2 }))
                .is_err()
        );
    }
}
