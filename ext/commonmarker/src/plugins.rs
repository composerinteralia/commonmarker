use std::path::Path;

use comrak::plugins::syntect::{SyntectAdapter, SyntectAdapterBuilder};
use serde_json::Value;
use syntect::highlighting::ThemeSet;

use crate::CallError;

pub fn syntax_highlighter(value: &Value) -> Result<Option<SyntectAdapter>, CallError> {
    let Some(plugin) = value
        .as_object()
        .and_then(|plugins| plugins.get("syntax_highlighter"))
    else {
        return Ok(None);
    };

    if plugin.is_null() {
        return Ok(None);
    }

    let plugin = plugin.as_object().ok_or_else(|| {
        CallError::type_error("Expected a Hash for syntax_highlighter plugin")
    })?;
    let theme = plugin
        .get("theme")
        .and_then(Value::as_str)
        .ok_or_else(|| CallError::type_error("theme cannot be nil"))?;
    let path = plugin.get("path").and_then(Value::as_str).unwrap_or("");

    if theme.is_empty() {
        return Ok(Some(SyntectAdapter::new(None)));
    }

    if path.is_empty() {
        ThemeSet::load_defaults()
            .themes
            .get(theme)
            .ok_or_else(|| CallError::argument_error(format!("theme `{theme}` does not exist")))?;
        return Ok(Some(SyntectAdapter::new(Some(theme))));
    }

    let path = Path::new(path);
    if !path.exists() {
        return Err(CallError::argument_error(format!(
            "theme path `{}` does not exist",
            path.display()
        )));
    }
    if !path.is_dir() {
        return Err(CallError::argument_error("`path` needs to be a directory"));
    }

    let mut themes = ThemeSet::load_defaults();
    themes.add_from_folder(path).map_err(|error| {
        CallError::argument_error(format!("failed to load theme set from path: {error}"))
    })?;
    if !themes.themes.contains_key(theme) {
        return Err(CallError::argument_error(format!(
            "theme `{theme}` does not exist"
        )));
    }

    Ok(Some(
        SyntectAdapterBuilder::new()
            .theme_set(themes)
            .theme(theme)
            .build(),
    ))
}
