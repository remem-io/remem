//! PII Detection, Redaction, Masking, and Policy Enforcement Engine.
//!
//! Protects sensitive user and enterprise data with configurable policies (Mask, Redact, Reject).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Policy action when PII or secrets are detected in content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiiAction {
    /// Replace detected sensitive text with a typed placeholder (e.g. `[REDACTED_API_KEY]`).
    #[default]
    Mask,
    /// Remove detected sensitive text entirely.
    Redact,
    /// Block storage operation with an error.
    Reject,
}

/// Category of sensitive entity detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PiiCategory {
    ApiKey,
    JwtToken,
    AwsArn,
    CreditCard,
    SocialSecurityNumber,
    EmailAddress,
    PhoneNumber,
    PasswordOrSecret,
}

/// A single detected sensitive entity match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiMatch {
    pub category: PiiCategory,
    pub original_text: String,
    pub placeholder: String,
    pub start_pos: usize,
    pub end_pos: usize,
}

/// Comprehensive PII Detection and Policy Engine.
#[derive(Debug, Clone)]
pub struct PiiPolicyEngine {
    pub action: PiiAction,
}

impl Default for PiiPolicyEngine {
    fn default() -> Self {
        Self {
            action: PiiAction::Mask,
        }
    }
}

impl PiiPolicyEngine {
    pub fn new(action: PiiAction) -> Self {
        Self { action }
    }

    /// Scan text and return all detected sensitive matches.
    pub fn detect(&self, text: &str) -> Vec<PiiMatch> {
        let mut matches = Vec::new();

        // 1. API Keys (OpenAI, Anthropic, Google, generic tokens)
        if let Ok(re) = regex::Regex::new(
            r"(?i)\b(sk-(?:ant-|proj-)?[a-zA-Z0-9_\-]{20,}|AIzaSy[a-zA-Z0-9_\-]{33})\b",
        ) {
            for m in re.find_iter(text) {
                matches.push(PiiMatch {
                    category: PiiCategory::ApiKey,
                    original_text: m.as_str().to_string(),
                    placeholder: "[REDACTED_API_KEY]".to_string(),
                    start_pos: m.start(),
                    end_pos: m.end(),
                });
            }
        }

        // 2. JWT Tokens
        if let Ok(re) =
            regex::Regex::new(r"\beyJ[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\.[a-zA-Z0-9_-]{10,}\b")
        {
            for m in re.find_iter(text) {
                matches.push(PiiMatch {
                    category: PiiCategory::JwtToken,
                    original_text: m.as_str().to_string(),
                    placeholder: "[REDACTED_JWT_TOKEN]".to_string(),
                    start_pos: m.start(),
                    end_pos: m.end(),
                });
            }
        }

        // 3. AWS ARNs
        if let Ok(re) = regex::Regex::new(
            r"\barn:aws:[a-zA-Z0-9_-]+:[a-zA-Z0-9_-]*:[0-9]{12}:[a-zA-Z0-9_/-]+\b",
        ) {
            for m in re.find_iter(text) {
                matches.push(PiiMatch {
                    category: PiiCategory::AwsArn,
                    original_text: m.as_str().to_string(),
                    placeholder: "[REDACTED_AWS_ARN]".to_string(),
                    start_pos: m.start(),
                    end_pos: m.end(),
                });
            }
        }

        // 4. Credit Cards (Visa, Mastercard, Amex, Discover)
        if let Ok(re) = regex::Regex::new(
            r"\b(?:4[0-9]{12}(?:[0-9]{3})?|5[1-5][0-9]{14}|3[47][0-9]{13}|6(?:011|5[0-9]{2})[0-9]{12})\b",
        ) {
            for m in re.find_iter(text) {
                matches.push(PiiMatch {
                    category: PiiCategory::CreditCard,
                    original_text: m.as_str().to_string(),
                    placeholder: "[REDACTED_CREDIT_CARD]".to_string(),
                    start_pos: m.start(),
                    end_pos: m.end(),
                });
            }
        }

        // 5. Social Security Numbers (SSN)
        if let Ok(re) = regex::Regex::new(r"\b\d{3}-\d{2}-\d{4}\b") {
            for m in re.find_iter(text) {
                matches.push(PiiMatch {
                    category: PiiCategory::SocialSecurityNumber,
                    original_text: m.as_str().to_string(),
                    placeholder: "[REDACTED_SSN]".to_string(),
                    start_pos: m.start(),
                    end_pos: m.end(),
                });
            }
        }

        // 6. Emails
        if let Ok(re) = regex::Regex::new(r"\b[a-zA-Z0-9_.+-]+@[a-zA-Z0-9-]+\.[a-zA-Z0-9-.]+\b") {
            for m in re.find_iter(text) {
                matches.push(PiiMatch {
                    category: PiiCategory::EmailAddress,
                    original_text: m.as_str().to_string(),
                    placeholder: "[REDACTED_EMAIL]".to_string(),
                    start_pos: m.start(),
                    end_pos: m.end(),
                });
            }
        }

        // 7. Phone Numbers (US & International formats)
        if let Ok(re) =
            regex::Regex::new(r"\b(?:\+?1[-. ]?)?\(?[0-9]{3}\)?[-. ][0-9]{3}[-. ][0-9]{4}\b")
        {
            for m in re.find_iter(text) {
                matches.push(PiiMatch {
                    category: PiiCategory::PhoneNumber,
                    original_text: m.as_str().to_string(),
                    placeholder: "[REDACTED_PHONE]".to_string(),
                    start_pos: m.start(),
                    end_pos: m.end(),
                });
            }
        }

        matches
    }

    /// Process content according to the configured PII policy action.
    pub fn process_content(&self, text: &str) -> Result<String, String> {
        let matches = self.detect(text);
        if matches.is_empty() {
            return Ok(text.to_string());
        }

        match self.action {
            PiiAction::Reject => {
                let categories: Vec<String> = matches
                    .iter()
                    .map(|m| format!("{:?}", m.category))
                    .collect();
                Err(format!(
                    "Memory rejected due to sensitive data: {}",
                    categories.join(", ")
                ))
            }
            PiiAction::Mask => {
                let mut result = text.to_string();
                for m in &matches {
                    result = result.replace(&m.original_text, &m.placeholder);
                }
                Ok(result)
            }
            PiiAction::Redact => {
                let mut result = text.to_string();
                for m in &matches {
                    result = result.replace(&m.original_text, "");
                }
                Ok(result)
            }
        }
    }
}

/// Token vault allowing authorized unmasking of sanitized tokens during audit.
#[derive(Debug, Default, Clone)]
pub struct PiiTokenVault {
    tokens: HashMap<String, String>, // placeholder -> original
}

impl PiiTokenVault {
    pub fn new() -> Self {
        Self {
            tokens: HashMap::new(),
        }
    }

    pub fn register(&mut self, placeholder: String, original: String) {
        self.tokens.insert(placeholder, original);
    }

    pub fn unmask(&self, text: &str) -> String {
        let mut unmasked = text.to_string();
        for (placeholder, original) in &self.tokens {
            unmasked = unmasked.replace(placeholder, original);
        }
        unmasked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pii_masking_comprehensive() {
        let engine = PiiPolicyEngine::new(PiiAction::Mask);
        let input = "Contact alice@example.com, SSN 123-45-6789, key sk-ant-12345678901234567890, phone 415-555-1234";
        let sanitized = engine.process_content(input).unwrap();

        assert!(sanitized.contains("[REDACTED_EMAIL]"));
        assert!(sanitized.contains("[REDACTED_SSN]"));
        assert!(sanitized.contains("[REDACTED_API_KEY]"));
        assert!(sanitized.contains("[REDACTED_PHONE]"));
    }

    #[test]
    fn test_pii_reject_policy() {
        let engine = PiiPolicyEngine::new(PiiAction::Reject);
        let input = "My secret token is sk-123456789012345678901234";
        assert!(engine.process_content(input).is_err());
    }

    #[test]
    fn test_token_vault_unmask() {
        let mut vault = PiiTokenVault::new();
        vault.register("[KEY_1]".to_string(), "secret_api_key_xyz".to_string());

        let masked = "Use [KEY_1] to authenticate";
        let unmasked = vault.unmask(masked);
        assert_eq!(unmasked, "Use secret_api_key_xyz to authenticate");
    }
}
