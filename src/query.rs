//! # Path-Based Typed Queries and Class-Aware Style Resolution
//!
//! Provides [`FromGssPath`] and [`GssExt`] for extracting typed values from [`Object`] using
//! path slices or dot-separated paths (e.g. `&["ship.max_thrust"]`), along with hierarchical
//! class fallback helpers (`element.class.prop` -> `element_class.prop` -> `class.prop` -> `element.prop`).

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use crate::Object;

/// Splits any dot-separated segments in `path` into individual path components.
pub fn normalize_path<'a>(path: &[&'a str]) -> Vec<&'a str> {
    if path.iter().any(|p| p.contains('.')) {
        path.iter().flat_map(|p| p.split('.')).collect()
    } else {
        path.to_vec()
    }
}

/// Trait for deserializing or extracting typed values from a [`crate::Object`] at a given path.
///
/// Unlike [`crate::FromGssValue`], which inspects a single leaf [`crate::Value`], `FromGssPath`
/// can inspect composite sub-objects (such as `{ x = 1.0, y = 2.0 }` or `{ r = 255, g = 0, b = 0 }`)
/// and normalize dotted paths.
pub trait FromGssPath: Sized {
    /// Extracts a value of type `Self` from `obj` at the given `path`.
    fn from_gss(obj: &Object, path: &[&str]) -> Option<Self>;
}

/// Extension methods for [`Object`] to query typed values with [`FromGssPath`].
pub trait GssExt {
    /// Retrieves a typed value at `path`, returning `None` if not found or conversion fails.
    fn get_val<T: FromGssPath>(&self, path: &[&str]) -> Option<T>;

    /// Retrieves a typed value at `path`, returning `default` if not found or conversion fails.
    fn get_val_or<T: FromGssPath>(&self, path: &[&str], default: T) -> T;
}

impl GssExt for Object {
    fn get_val<T: FromGssPath>(&self, path: &[&str]) -> Option<T> {
        T::from_gss(self, path)
    }

    fn get_val_or<T: FromGssPath>(&self, path: &[&str], default: T) -> T {
        T::from_gss(self, path).unwrap_or(default)
    }
}

impl FromGssPath for f32 {
    fn from_gss(obj: &Object, path: &[&str]) -> Option<Self> {
        let norm = normalize_path(path);
        if let Some(&val) = obj.get::<f32>(&norm) {
            Some(val)
        } else if let Some(&val) = obj.get::<u32>(&norm) {
            Some(val as f32)
        } else if let Some(&val) = obj.get::<i32>(&norm) {
            Some(val as f32)
        } else {
            None
        }
    }
}

impl FromGssPath for u32 {
    fn from_gss(obj: &Object, path: &[&str]) -> Option<Self> {
        let norm = normalize_path(path);
        if let Some(&val) = obj.get::<u32>(&norm) {
            Some(val)
        } else if let Some(&val) = obj.get::<i32>(&norm) {
            Some(val.max(0) as u32)
        } else if let Some(&val) = obj.get::<f32>(&norm) {
            Some(val.max(0.0) as u32)
        } else {
            None
        }
    }
}

impl FromGssPath for i32 {
    fn from_gss(obj: &Object, path: &[&str]) -> Option<Self> {
        let norm = normalize_path(path);
        if let Some(&val) = obj.get::<i32>(&norm) {
            Some(val)
        } else if let Some(&val) = obj.get::<u32>(&norm) {
            Some(val as i32)
        } else if let Some(&val) = obj.get::<f32>(&norm) {
            Some(val as i32)
        } else {
            None
        }
    }
}

impl FromGssPath for usize {
    fn from_gss(obj: &Object, path: &[&str]) -> Option<Self> {
        let norm = normalize_path(path);
        if let Some(&val) = obj.get::<usize>(&norm) {
            Some(val)
        } else if let Some(&val) = obj.get::<u32>(&norm) {
            Some(val as usize)
        } else if let Some(&val) = obj.get::<i32>(&norm) {
            Some(val.max(0) as usize)
        } else if let Some(&val) = obj.get::<f32>(&norm) {
            Some(val.max(0.0) as usize)
        } else {
            None
        }
    }
}

impl FromGssPath for bool {
    fn from_gss(obj: &Object, path: &[&str]) -> Option<Self> {
        let norm = normalize_path(path);
        obj.get::<bool>(&norm).copied()
    }
}

impl FromGssPath for String {
    fn from_gss(obj: &Object, path: &[&str]) -> Option<Self> {
        let norm = normalize_path(path);
        obj.get::<String>(&norm).cloned()
    }
}

/// Retrieves a boolean property from the stylesheet for a specific element name and field.
pub fn get_bool_field(obj: &Object, name: &str, field: &str, default: bool) -> bool {
    obj.get_val_or(&[name, field], default)
}

/// Retrieves a float (`f32`) property from the stylesheet at the given path.
pub fn get_f32_field(obj: &Object, path: &[&str], default: f32) -> f32 {
    obj.get_val_or(path, default)
}

/// Retrieves a string property from the stylesheet at the given path.
pub fn get_string_field(obj: &Object, path: &[&str], default: &str) -> String {
    obj.get_val_or(path, default.to_string())
}

/// Retrieves a `usize` property from the stylesheet at the given path.
pub fn get_usize_field(obj: &Object, path: &[&str], default: usize) -> usize {
    obj.get_val_or(path, default)
}

/// Resolves a typed property with hierarchical class fallback:
/// 1. `&[name, class, prop]` (`element.class.prop`)
/// 2. `&[&format!("{}_{}", name, class), prop]` (`element_class.prop`)
/// 3. `&[class, prop]` (`class.prop`)
/// 4. `&[name, prop]` (`element.prop`)
/// 5. `default`
pub fn get_val_with_class<T: FromGssPath>(
    obj: &Object,
    name: &str,
    class: Option<&str>,
    prop: &str,
    default: T,
) -> T {
    if let Some(cls) = class {
        if let Some(val) = obj.get_val::<T>(&[name, cls, prop]) {
            return val;
        }
        let merged = format!("{}_{}", name, cls);
        if let Some(val) = obj.get_val::<T>(&[&merged, prop]) {
            return val;
        }
        if let Some(val) = obj.get_val::<T>(&[cls, prop]) {
            return val;
        }
    }
    obj.get_val_or(&[name, prop], default)
}

/// Retrieves a float property with hierarchical class fallback.
pub fn get_f32_field_with_class(
    obj: &Object,
    name: &str,
    class: Option<&str>,
    prop: &str,
    default: f32,
) -> f32 {
    get_val_with_class(obj, name, class, prop, default)
}

/// Retrieves a boolean property with hierarchical class fallback.
pub fn get_bool_field_with_class(
    obj: &Object,
    name: &str,
    class: Option<&str>,
    prop: &str,
    default: bool,
) -> bool {
    get_val_with_class(obj, name, class, prop, default)
}

/// Retrieves a relative coordinate or float property from the stylesheet at the given path.
///
/// - If the retrieved value is a percentage (e.g. `80%`) or float (e.g. `0.8`), it is scaled by `scale`.
/// - If the retrieved value is an exact integer literal (e.g. `120`), it is treated as absolute pixels.
pub fn get_relative_field(obj: &Object, path: &[&str], scale: f32, default: f32) -> f32 {
    get_optional_relative_field(obj, path, scale).unwrap_or(default)
}

/// Retrieves an optional relative coordinate or float property from the stylesheet.
///
/// - If the retrieved value is a percentage (e.g. `80%`) or float (e.g. `0.8`), it is scaled by `scale`.
/// - If the retrieved value is an exact integer literal (e.g. `120`), it is treated as absolute pixels.
pub fn get_optional_relative_field(obj: &Object, path: &[&str], scale: f32) -> Option<f32> {
    let norm = normalize_path(path);
    if let Some(&val) = obj.get::<u32>(&norm) {
        Some(val as f32)
    } else if let Some(&val) = obj.get::<i32>(&norm) {
        Some(val as f32)
    } else if let Some(&val) = obj.get::<f32>(&norm) {
        Some(val * scale)
    } else {
        None
    }
}

/// Returns `true` if the stylesheet contains an entry for the selector `name`.
pub fn has_selector(obj: &Object, name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    let norm = normalize_path(&[name]);
    obj.get::<Object>(&norm).is_some()
}

static WARNED_SELECTORS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Emits a one-time warning to `eprintln!` if `name` is not found in a non-empty stylesheet.
pub fn warn_missing_selector(obj: &Object, name: &str) {
    if name.is_empty() || obj.get_fields().len() == 0 || has_selector(obj, name) {
        return;
    }
    if let Ok(mut warned) = WARNED_SELECTORS.lock()
        && warned.insert(name.to_string())
    {
        eprintln!(
            "[gss] warning: selector '{}' not found in stylesheet.",
            name
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_str;

    #[test]
    fn test_gss_query_helpers_and_class_fallback() {
        let style = parse_str(
            r#"
            ship = {
                max_thrust = 420.0,
            },
            warning = {
                font_size = 28.0,
            },
            badge = {
                font_size = 18.0,
                active = false,
                scale = 50%,
                pixel_offset = 150,
                title = "Shields",
                count = 4,
                danger = {
                    font_size = 32.0,
                    active = true,
                },
            },
            badge_custom = {
                font_size = 24.0,
            },
            "#,
        )
        .unwrap();

        assert_eq!(style.get_val::<f32>(&["ship.max_thrust"]), Some(420.0));
        assert_eq!(get_f32_field(&style, &["badge", "font_size"], 12.0), 18.0);
        assert_eq!(
            get_f32_field_with_class(&style, "badge", Some("danger"), "font_size", 12.0),
            32.0
        );
        assert_eq!(
            get_f32_field_with_class(&style, "badge", Some("custom"), "font_size", 12.0),
            24.0
        );
        assert_eq!(
            get_f32_field_with_class(&style, "badge", Some("warning"), "font_size", 12.0),
            28.0
        );
        assert!(get_bool_field_with_class(
            &style,
            "badge",
            Some("danger"),
            "active",
            false
        ));
        assert_eq!(
            get_relative_field(&style, &["badge", "scale"], 800.0, 0.0),
            400.0
        );
        assert_eq!(
            get_relative_field(&style, &["badge", "pixel_offset"], 800.0, 0.0),
            150.0
        );
        assert_eq!(
            get_string_field(&style, &["badge", "title"], "Default"),
            "Shields"
        );
        assert_eq!(get_usize_field(&style, &["badge", "count"], 1), 4);
        assert!(has_selector(&style, "badge"));
        assert!(!has_selector(&style, "nonexistent"));
    }
}
