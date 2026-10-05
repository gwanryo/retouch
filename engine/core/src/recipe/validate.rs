//! Validation rules (master plan §3.1, AC-E3c/E3d). Filled in by Tasks 5b and 5c.

use super::schema::Recipe;
use super::{Normalization, RecipeError};

pub(super) fn normalize(r: Recipe) -> Result<(Recipe, Vec<Normalization>), RecipeError> {
    if r.schema_version != crate::SCHEMA_VERSION {
        return Err(RecipeError::SchemaVersion(r.schema_version));
    }
    Ok((r, Vec::new()))
}
