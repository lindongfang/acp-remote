use std::collections::HashSet;

use serde::{Deserialize, Deserializer, de};
use serde_json::{Map, Value};
use thiserror::Error;

pub const MAX_PROTOCOL_JSON_DEPTH: usize = 8;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum StrictJsonError {
    #[error("JSON is invalid")]
    Invalid,
    #[error("JSON contains a duplicate object field")]
    DuplicateField,
    #[error("JSON root must be an object")]
    NonObject,
    #[error("JSON nesting exceeds the protocol limit")]
    TooDeep,
}

pub fn parse_object(input: &[u8], max_depth: usize) -> Result<Map<String, Value>, StrictJsonError> {
    validate_depth(input, max_depth)?;
    let mut deserializer = serde_json::Deserializer::from_slice(input);
    let value = StrictValue::deserialize(&mut deserializer).map_err(|error| {
        if error.to_string().contains("duplicate JSON key") {
            StrictJsonError::DuplicateField
        } else {
            StrictJsonError::Invalid
        }
    })?;
    deserializer.end().map_err(|_| StrictJsonError::Invalid)?;
    match value.0 {
        Value::Object(object) => Ok(object),
        _ => Err(StrictJsonError::NonObject),
    }
}

fn validate_depth(input: &[u8], max_depth: usize) -> Result<(), StrictJsonError> {
    let mut depth = 0_usize;
    let mut in_string = false;
    let mut escaped = false;
    for byte in input {
        if in_string {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match *byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth = depth.checked_add(1).ok_or(StrictJsonError::TooDeep)?;
                if depth > max_depth {
                    return Err(StrictJsonError::TooDeep);
                }
            }
            b'}' | b']' => {
                depth = depth.checked_sub(1).ok_or(StrictJsonError::Invalid)?;
            }
            _ => {}
        }
    }
    if in_string || escaped || depth != 0 {
        return Err(StrictJsonError::Invalid);
    }
    Ok(())
}

struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(StrictValueVisitor)
    }
}

struct StrictValueVisitor;

impl<'de> serde::de::Visitor<'de> for StrictValueVisitor {
    type Value = StrictValue;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value with unique object field names")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Bool(value)))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Number(value.into())))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Number(value.into())))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .map(StrictValue)
            .ok_or_else(|| E::custom("JSON number is not finite"))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::String(value.to_owned())))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::String(value)))
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(StrictValue(Value::Null))
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element::<StrictValue>()? {
            values.push(value.0);
        }
        Ok(StrictValue(Value::Array(values)))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut fields = Map::new();
        let mut names = HashSet::new();
        while let Some((name, value)) = object.next_entry::<String, StrictValue>()? {
            if !names.insert(name.clone()) {
                return Err(de::Error::custom("duplicate JSON key"));
            }
            fields.insert(name, value.0);
        }
        Ok(StrictValue(Value::Object(fields)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_fields_at_any_depth() {
        assert_eq!(
            parse_object(br#"{"v":1,"v":1}"#, MAX_PROTOCOL_JSON_DEPTH),
            Err(StrictJsonError::DuplicateField)
        );
        assert_eq!(
            parse_object(
                br#"{"v":1,"nested":{"field":1,"field":2}}"#,
                MAX_PROTOCOL_JSON_DEPTH
            ),
            Err(StrictJsonError::DuplicateField)
        );
    }

    #[test]
    fn enforces_the_eight_level_protocol_boundary() {
        assert!(parse_object(br#"{"a":{"b":{"c":{"d":{"e":{"f":{"g":{"h":1}}}}}}}}"#, 8).is_ok());
        assert_eq!(
            parse_object(
                br#"{"a":{"b":{"c":{"d":{"e":{"f":{"g":{"h":{"i":1}}}}}}}}}"#,
                8
            ),
            Err(StrictJsonError::TooDeep)
        );
        assert_eq!(
            parse_object(br#"["not-an-object"]"#, MAX_PROTOCOL_JSON_DEPTH),
            Err(StrictJsonError::NonObject)
        );
    }

    #[test]
    fn ignores_brackets_inside_strings() {
        assert!(parse_object(br#"{"value":"[[[[[[[[[","other":"\\\""}"#, 2).is_ok());
    }
}
