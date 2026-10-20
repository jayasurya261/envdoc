use crate::model::definition::Definition;
use crate::model::path::RelPath;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A parsed contract representing the declared environment interface (e.g. `.env.example`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contract {
    /// Relative path to the contract file.
    pub path: RelPath,
    /// Variables declared within the contract.
    pub definitions: Vec<Definition>,
}

impl Contract {
    /// Constructs a new `Contract`.
    pub fn new(path: impl Into<RelPath>, definitions: Vec<Definition>) -> Self {
        Self {
            path: path.into(),
            definitions,
        }
    }

    /// Returns `true` if the contract contains zero variable declarations.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    /// Returns the number of variable declarations in the contract.
    #[inline]
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    /// Finds a contract definition by variable name.
    pub fn find(&self, name: &str) -> Option<&Definition> {
        self.definitions.iter().find(|d| d.name.as_str() == name)
    }

    /// Returns a map of variable name to `Definition` reference.
    pub fn definition_map(&self) -> HashMap<&str, &Definition> {
        self.definitions
            .iter()
            .map(|d| (d.name.as_str(), d))
            .collect()
    }
}

