use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Standard,
    Debugging,
    Refactoring,
    Exploration,
    Writing,
}

impl Mode {
    pub fn adjust_recall_limit(&self, base_limit: usize) -> usize {
        match self {
            Mode::Standard => base_limit,
            Mode::Debugging => base_limit * 2,
            Mode::Refactoring => base_limit * 3,
            Mode::Exploration => base_limit,
            Mode::Writing => base_limit * 2,
        }
    }

    pub fn adjust_token_budget(&self, base_budget: usize) -> usize {
        match self {
            Mode::Standard => base_budget,
            Mode::Debugging => base_budget + 2000,
            Mode::Refactoring => base_budget + 4000,
            Mode::Exploration => base_budget - 1000,
            Mode::Writing => base_budget + 2000,
        }
    }
}
