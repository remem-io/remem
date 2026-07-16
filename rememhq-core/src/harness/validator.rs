use serde_json::Value;

/// Error type for validation failures.
#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error("Missing required field: {0}")]
    MissingField(String),
    #[error("Invalid format for field {field}: {reason}")]
    InvalidFormat { field: String, reason: String },
    #[error("Type mismatch for field {field}: expected {expected}, got {actual}")]
    TypeMismatch {
        field: String,
        expected: String,
        actual: String,
    },
    #[error("Other validation error: {0}")]
    Other(String),
}

/// Returns the JSON type name of a value, as used in `ValidationError::TypeMismatch`.
fn json_type_name(val: &Value) -> &'static str {
    match val {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// A trait for validating structured outputs from the LLM.
pub trait Validator: Send + Sync {
    /// Validates the raw JSON output and returns the validated result or an error.
    fn validate(&self, raw: &Value) -> Result<(), ValidationError>;
}

/// A simple schema validator that ensures required fields exist and have the correct type.
pub struct SchemaValidator {
    required_fields: Vec<(String, String)>, // (field_name, expected_type)
}

impl SchemaValidator {
    pub fn new(required_fields: Vec<(String, String)>) -> Self {
        Self { required_fields }
    }
}

impl Validator for SchemaValidator {
    fn validate(&self, raw: &Value) -> Result<(), ValidationError> {
        if !raw.is_object() {
            return Err(ValidationError::TypeMismatch {
                field: "root".to_string(),
                expected: "object".to_string(),
                actual: json_type_name(raw).to_string(),
            });
        }

        let obj = raw.as_object().unwrap();

        for (field, expected_type) in &self.required_fields {
            if let Some(val) = obj.get(field) {
                let actual_type = json_type_name(val);

                if actual_type != expected_type {
                    return Err(ValidationError::TypeMismatch {
                        field: field.clone(),
                        expected: expected_type.clone(),
                        actual: actual_type.to_string(),
                    });
                }
            } else {
                return Err(ValidationError::MissingField(field.clone()));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn name_and_age_validator() -> SchemaValidator {
        SchemaValidator::new(vec![
            ("name".to_string(), "string".to_string()),
            ("age".to_string(), "number".to_string()),
        ])
    }

    #[test]
    fn test_valid_object_passes() {
        let validator = name_and_age_validator();
        let raw = json!({"name": "Alice", "age": 30});
        assert!(validator.validate(&raw).is_ok());
    }

    #[test]
    fn test_extra_fields_are_ignored() {
        let validator = name_and_age_validator();
        let raw = json!({"name": "Alice", "age": 30, "extra": true});
        assert!(validator.validate(&raw).is_ok());
    }

    #[test]
    fn test_missing_field() {
        let validator = name_and_age_validator();
        let raw = json!({"name": "Alice"});
        match validator.validate(&raw) {
            Err(ValidationError::MissingField(field)) => assert_eq!(field, "age"),
            other => panic!("expected MissingField(\"age\"), got {:?}", other),
        }
    }

    #[test]
    fn test_field_type_mismatch_reports_actual_type() {
        let validator = name_and_age_validator();
        let raw = json!({"name": "Alice", "age": "thirty"});
        match validator.validate(&raw) {
            Err(ValidationError::TypeMismatch {
                field,
                expected,
                actual,
            }) => {
                assert_eq!(field, "age");
                assert_eq!(expected, "number");
                assert_eq!(actual, "string");
            }
            other => panic!("expected TypeMismatch, got {:?}", other),
        }
    }

    #[test]
    fn test_non_object_root_reports_actual_type_not_hardcoded_other() {
        let validator = name_and_age_validator();

        // Regression test: this used to always report `actual: "other"` regardless
        // of what the root value actually was, which produced a useless message
        // like "expected object, got other" — including for arrays, strings, etc.
        for (raw, expected_actual) in [
            (json!([1, 2, 3]), "array"),
            (json!("just a string"), "string"),
            (json!(42), "number"),
            (json!(true), "boolean"),
            (json!(null), "null"),
        ] {
            match validator.validate(&raw) {
                Err(ValidationError::TypeMismatch {
                    field,
                    expected,
                    actual,
                }) => {
                    assert_eq!(field, "root");
                    assert_eq!(expected, "object");
                    assert_eq!(
                        actual, expected_actual,
                        "for input {:?}, expected actual type {:?} but got {:?}",
                        raw, expected_actual, actual
                    );
                }
                other => panic!("expected TypeMismatch for {:?}, got {:?}", raw, other),
            }
        }
    }
}
