use serde_json::Value;

fn object(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    value.as_object()
}

pub fn build_options(value: &Value) -> comrak::Options<'static> {
    let mut options = comrak::Options::default();
    let Some(groups) = object(value) else {
        return options;
    };

    if let Some(parse) = groups.get("parse").and_then(object) {
        set_bool(parse, "smart", &mut options.parse.smart);
        set_optional_string(
            parse,
            "default_info_string",
            &mut options.parse.default_info_string,
        );
        set_bool(
            parse,
            "relaxed_tasklist_matching",
            &mut options.parse.relaxed_tasklist_matching,
        );
        set_bool(
            parse,
            "relaxed_autolinks",
            &mut options.parse.relaxed_autolinks,
        );
        set_bool(
            parse,
            "leave_footnote_definitions",
            &mut options.parse.leave_footnote_definitions,
        );
        set_bool(parse, "ignore_setext", &mut options.parse.ignore_setext);
        set_bool(
            parse,
            "sourcepos_chars",
            &mut options.parse.sourcepos_chars,
        );
    }

    if let Some(render) = groups.get("render").and_then(object) {
        set_bool(render, "hardbreaks", &mut options.render.hardbreaks);
        set_bool(
            render,
            "github_pre_lang",
            &mut options.render.github_pre_lang,
        );
        set_bool(
            render,
            "full_info_string",
            &mut options.render.full_info_string,
        );
        set_usize(render, "width", &mut options.render.width);
        set_bool(render, "unsafe", &mut options.render.r#unsafe);
        set_bool(render, "escape", &mut options.render.escape);
        set_bool(render, "sourcepos", &mut options.render.sourcepos);
        set_bool(
            render,
            "escaped_char_spans",
            &mut options.render.escaped_char_spans,
        );
        set_bool(
            render,
            "ignore_empty_links",
            &mut options.render.ignore_empty_links,
        );
        set_bool(render, "gfm_quirks", &mut options.render.gfm_quirks);
        set_bool(
            render,
            "prefer_fenced",
            &mut options.render.prefer_fenced,
        );
        set_bool(
            render,
            "tasklist_classes",
            &mut options.render.tasklist_classes,
        );
        set_bool(
            render,
            "compact_html",
            &mut options.render.compact_html,
        );
        if render.get("alert_style").and_then(Value::as_str) == Some("semantic") {
            options.render.alert_style = comrak::options::AlertStyleType::Semantic;
        }
    }

    if let Some(extension) = groups.get("extension").and_then(object) {
        set_bool(
            extension,
            "strikethrough",
            &mut options.extension.strikethrough,
        );
        #[allow(deprecated)]
        set_bool(extension, "tagfilter", &mut options.extension.tagfilter);
        set_bool(extension, "table", &mut options.extension.table);
        set_bool(extension, "autolink", &mut options.extension.autolink);
        set_bool(extension, "tasklist", &mut options.extension.tasklist);
        set_bool(
            extension,
            "superscript",
            &mut options.extension.superscript,
        );
        if let Some(value) = extension.get("header_ids").and_then(Value::as_str) {
            options.extension.header_id_prefix = Some(value.to_owned());
        }
        set_bool(
            extension,
            "header_id_prefix_in_href",
            &mut options.extension.header_id_prefix_in_href,
        );
        set_bool(extension, "footnotes", &mut options.extension.footnotes);
        set_bool(
            extension,
            "inline_footnotes",
            &mut options.extension.inline_footnotes,
        );
        set_bool(
            extension,
            "description_lists",
            &mut options.extension.description_lists,
        );
        if let Some(value) = extension
            .get("front_matter_delimiter")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
        {
            options.extension.front_matter_delimiter = Some(value.to_owned());
        }
        set_bool(
            extension,
            "multiline_block_quotes",
            &mut options.extension.multiline_block_quotes,
        );
        set_bool(
            extension,
            "math_dollars",
            &mut options.extension.math_dollars,
        );
        set_bool(extension, "math_code", &mut options.extension.math_code);
        set_bool(extension, "math_latex", &mut options.extension.math_latex);
        set_bool(extension, "shortcodes", &mut options.extension.shortcodes);
        set_bool(
            extension,
            "wikilinks_title_after_pipe",
            &mut options.extension.wikilinks_title_after_pipe,
        );
        set_bool(
            extension,
            "wikilinks_title_before_pipe",
            &mut options.extension.wikilinks_title_before_pipe,
        );
        set_bool(extension, "underline", &mut options.extension.underline);
        set_bool(extension, "spoiler", &mut options.extension.spoiler);
        set_bool(extension, "greentext", &mut options.extension.greentext);
        set_bool(extension, "subscript", &mut options.extension.subscript);
        set_bool(extension, "subtext", &mut options.extension.subtext);
        set_bool(extension, "alerts", &mut options.extension.alerts);
        set_bool(
            extension,
            "cjk_friendly_emphasis",
            &mut options.extension.cjk_friendly_emphasis,
        );
        set_bool(extension, "highlight", &mut options.extension.highlight);
        set_bool(extension, "insert", &mut options.extension.insert);
        set_bool(
            extension,
            "block_directive",
            &mut options.extension.block_directive,
        );
    }

    options
}

fn set_bool(map: &serde_json::Map<String, Value>, key: &str, target: &mut bool) {
    if let Some(value) = map.get(key).and_then(Value::as_bool) {
        *target = value;
    }
}

fn set_usize(map: &serde_json::Map<String, Value>, key: &str, target: &mut usize) {
    if let Some(value) = map.get(key).and_then(Value::as_u64) {
        *target = value as usize;
    }
}

fn set_optional_string(
    map: &serde_json::Map<String, Value>,
    key: &str,
    target: &mut Option<String>,
) {
    if let Some(value) = map.get(key).and_then(Value::as_str) {
        *target = Some(value.to_owned());
    }
}
