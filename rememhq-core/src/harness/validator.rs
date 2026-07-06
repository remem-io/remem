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
                actual: "other".to_string(),
            });
        }

        let obj = raw.as_object().unwrap();

        for (field, expected_type) in &self.required_fields {
            if let Some(val) = obj.get(field) {
                let actual_type = match val {
                    Value::Null => "null",
                    Value::Bool(_) => "boolean",
                    Value::Number(_) => "number",
                    Value::String(_) => "string",
                    Value::Array(_) => "array",
                    Value::Object(_) => "object",
                };

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
