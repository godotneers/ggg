//! Serde deserialization helpers shared across the HTTP API clients
//! (`godot/asset_lib.rs`, `godot/asset_store.rs`).

use serde::{Deserialize, Deserializer};

/// Deserialize a field that an API returns as a JSON string containing a
/// decimal integer (e.g. `"1586"`) into a `u32`.
pub fn de_string_u32<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let s = String::deserialize(d)?;
    s.parse::<u32>().map_err(serde::de::Error::custom)
}

/// Deserialize `download_hash`: treat an empty string as `None`.
pub fn de_optional_hash<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let s = String::deserialize(d)?;
    Ok(if s.is_empty() { None } else { Some(s) })
}
