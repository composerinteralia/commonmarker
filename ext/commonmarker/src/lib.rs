use std::ffi::{c_char, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::slice;
use std::borrow::Cow;

use comrak::nodes::{
    Ast, AstNode, ListDelimType, ListType, NodeCode, NodeCodeBlock, NodeFootnoteDefinition,
    NodeFootnoteReference, NodeHeading, NodeHtmlBlock, NodeLink, NodeList, NodeTable, NodeTaskItem,
    NodeValue, Sourcepos, TableAlignment,
};
use comrak::{format_commonmark, format_html, format_xml, parse_document, Arena, Options};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

#[derive(Debug, Deserialize)]
struct Request {
    operation: String,
    #[serde(default)]
    markdown: String,
    #[serde(default)]
    options: u32,
    #[serde(default)]
    extensions: Vec<String>,
    node: Option<WireNode>,
    format: Option<String>,
    width: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Response {
    Success { ok: bool, value: Value },
    Failure { ok: bool, error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct WireNode {
    kind: String,
    #[serde(default)]
    data: Map<String, Value>,
    #[serde(default)]
    sourcepos: [usize; 4],
    #[serde(default)]
    children: Vec<WireNode>,
}

#[no_mangle]
pub unsafe extern "C" fn commonmarker_call(input: *const u8, input_len: usize) -> *mut c_char {
    let response = catch_unwind(AssertUnwindSafe(|| {
        if input.is_null() {
            return failure("input pointer cannot be null");
        }

        let bytes = unsafe { slice::from_raw_parts(input, input_len) };
        match serde_json::from_slice::<Request>(bytes)
            .map_err(|error| error.to_string())
            .and_then(handle_request)
        {
            Ok(value) => Response::Success { ok: true, value },
            Err(error) => failure(error),
        }
    }))
    .unwrap_or_else(|_| failure("Rust panic"));

    let json = serde_json::to_string(&response)
        .unwrap_or_else(|_| r#"{"ok":false,"error":"response serialization failed"}"#.to_owned());
    CString::new(json)
        .expect("JSON cannot contain null bytes")
        .into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn commonmarker_free_string(pointer: *mut c_char) {
    if !pointer.is_null() {
        drop(unsafe { CString::from_raw(pointer) });
    }
}

fn handle_request(request: Request) -> Result<Value, String> {
    match request.operation.as_str() {
        "parse" => {
            let options = build_options(request.options, &request.extensions, request.width);
            let arena = Arena::new();
            let parse_markdown = normalize_liberal_html_whitespace(&request.markdown, request.options);
            let root = parse_document(&arena, &parse_markdown, &options);
            apply_compatibility_transforms(&arena, root, &request.markdown, request.options);
            serde_json::to_value(WireNode::from_comrak(root)).map_err(|error| error.to_string())
        }
        "render_markdown" => {
            let options = build_options(request.options, &request.extensions, request.width);
            let arena = Arena::new();
            let parse_markdown = normalize_liberal_html_whitespace(&request.markdown, request.options);
            let root = parse_document(&arena, &parse_markdown, &options);
            apply_compatibility_transforms(&arena, root, &request.markdown, request.options);
            let legacy_markers = mark_legacy_numeric_noncharacters(root, &request.markdown);
            render(
                root,
                &options,
                request.format.as_deref().unwrap_or("html"),
                request.options,
                legacy_markers,
            )
        }
        "render_ast" => {
            let options = build_options(request.options, &request.extensions, request.width);
            let wire = request
                .node
                .as_ref()
                .ok_or_else(|| "render_ast requires a node".to_owned())?;
            let arena = Arena::new();
            let root = wire.to_comrak(&arena)?;
            render(
                root,
                &options,
                request.format.as_deref().unwrap_or("html"),
                request.options,
                None,
            )
        }
        operation => Err(format!("unknown operation `{operation}`")),
    }
}

fn render<'a>(
    root: &'a AstNode<'a>,
    options: &Options<'_>,
    format: &str,
    option_bits: u32,
    legacy_markers: Option<(char, char)>,
) -> Result<Value, String> {
    let mut output = String::new();
    match format {
        "html" => {
            flatten_nested_strong(root);
            format_html(root, options, &mut output)
        }
        "xml" => format_xml(root, options, &mut output),
        "commonmark" => format_commonmark(root, options, &mut output),
        "plaintext" => {
            let plaintext_options = PlaintextOptions {
                collapse_spaces: options.render.width > 0
                    && option_bits & (1 << 4) == 0
                    && option_bits & (1 << 2) == 0,
                softbreak_as_newline: option_bits & (1 << 2) != 0
                    || (options.render.width == 0 && option_bits & (1 << 4) == 0),
            };
            format_plaintext(root, &mut output, plaintext_options);
            Ok(())
        }
        _ => return Err(format!("unknown format `{format}`")),
    }
    .map_err(|error| error.to_string())?;
    if format == "html" && option_bits & (1 << 15) != 0 {
        output = output
            .replace(r#" align="left""#, r#" style="text-align: left""#)
            .replace(r#" align="center""#, r#" style="text-align: center""#)
            .replace(r#" align="right""#, r#" style="text-align: right""#);
    }
    Ok(legacy_output_value(output, legacy_markers))
}

fn flatten_nested_strong<'a>(root: &'a AstNode<'a>) {
    let nodes = root.descendants().collect::<Vec<_>>();
    for node in nodes {
        let is_strong = matches!(node.data.borrow().value, NodeValue::Strong);
        let parent_is_strong = node
            .parent()
            .is_some_and(|parent| matches!(parent.data.borrow().value, NodeValue::Strong));
        if !is_strong || !parent_is_strong {
            continue;
        }

        let children = node.children().collect::<Vec<_>>();
        for child in children {
            node.insert_before(child);
        }
        node.detach();
    }
}

fn apply_compatibility_transforms<'a>(
    arena: &'a Arena<'a>,
    root: &'a AstNode<'a>,
    markdown: &str,
    option_bits: u32,
) {
    let lines = markdown.lines().collect::<Vec<_>>();
    let nodes = root.descendants().collect::<Vec<_>>();
    for node in &nodes {
        let mut ast = node.data.borrow_mut();
        if matches!(ast.value, NodeValue::HtmlInline(_))
            && ast.sourcepos.start.line == ast.sourcepos.end.line
        {
            if let Some(literal) = source_literal(&lines, ast.sourcepos) {
                ast.value = NodeValue::HtmlInline(literal.to_owned());
            }
        }
        if matches!(ast.value, NodeValue::List(_)) {
            let original_end_line = ast.sourcepos.end.line;
            while lines
                .get(ast.sourcepos.end.line)
                .is_some_and(|line| line.is_empty())
            {
                ast.sourcepos.end.line += 1;
            }
            let added_lines = ast.sourcepos.end.line - original_end_line;
            if added_lines == 0 {
                continue;
            }
            ast.sourcepos.end.column = 0;
            drop(ast);
            if let Some(last_child) = node.last_child() {
                let mut child_ast = last_child.data.borrow_mut();
                child_ast.sourcepos.end.line += added_lines;
                child_ast.sourcepos.end.column = 0;
            }
        }
    }

    if option_bits & (1 << 12) != 0 {
        for node in &nodes {
            convert_liberal_html_tags(arena, node, &lines);
        }
    }

    if option_bits & (1 << 14) == 0 {
        return;
    }

    for node in nodes {
        let sourcepos = node.data.borrow().sourcepos;
        if !matches!(node.data.borrow().value, NodeValue::Strikethrough)
            || sourcepos.start.line != sourcepos.end.line
        {
            continue;
        }

        let Some(line) = lines.get(sourcepos.start.line.saturating_sub(1)) else {
            continue;
        };
        let start = sourcepos.start.column.saturating_sub(1);
        let end = sourcepos.end.column.min(line.len());
        let Some(literal) = line.get(start..end) else {
            continue;
        };
        if literal.starts_with('~')
            && !literal.starts_with("~~")
            && literal.ends_with('~')
            && !literal.ends_with("~~")
        {
            node.data.borrow_mut().value = NodeValue::Text(Cow::Owned(literal.to_owned()));
            let children = node.children().collect::<Vec<_>>();
            for child in children {
                child.detach();
            }
        }
    }
}

fn normalize_liberal_html_whitespace(markdown: &str, option_bits: u32) -> Cow<'_, str> {
    if option_bits & (1 << 12) == 0 || !markdown.contains('\u{a0}') {
        return Cow::Borrowed(markdown);
    }

    let mut output = markdown.as_bytes().to_vec();
    let mut inside_tag = false;
    let mut position = 0;
    while position < output.len() {
        match output[position] {
            b'<' => inside_tag = true,
            b'>' | b'\n' | 0 => inside_tag = false,
            0xc2 if inside_tag && output.get(position + 1) == Some(&0xa0) => {
                output[position] = b' ';
                output[position + 1] = b' ';
                position += 1;
            }
            _ => {}
        }
        position += 1;
    }

    Cow::Owned(String::from_utf8(output).expect("replacing NBSP preserves UTF-8"))
}

fn source_literal<'a>(lines: &[&'a str], sourcepos: Sourcepos) -> Option<&'a str> {
    lines
        .get(sourcepos.start.line.saturating_sub(1))
        .and_then(|line| {
            line.get(
                sourcepos.start.column.saturating_sub(1)
                    ..sourcepos.end.column.min(line.len()),
            )
        })
}

fn convert_liberal_html_tags<'a>(
    arena: &'a Arena<'a>,
    node: &'a AstNode<'a>,
    lines: &[&str],
) {
    let ast = node.data.borrow();
    let NodeValue::Text(text) = &ast.value else {
        return;
    };
    let sourcepos = ast.sourcepos;
    if sourcepos.start.line != sourcepos.end.line {
        return;
    }
    let Some(source) = source_literal(lines, sourcepos) else {
        return;
    };
    if source != text {
        return;
    }
    let text = text.to_string();
    let segments = liberal_html_segments(&text);
    if segments.len() == 1 && !segments[0].1 {
        return;
    }
    drop(ast);

    let mut column = sourcepos.start.column;
    for (literal, is_html) in segments {
        let end_column = column + literal.len().saturating_sub(1);
        let segment_sourcepos =
            Sourcepos::from((sourcepos.start.line, column, sourcepos.end.line, end_column));
        let value = if is_html {
            NodeValue::HtmlInline(literal.to_owned())
        } else {
            NodeValue::Text(Cow::Owned(literal.to_owned()))
        };
        let segment = arena.alloc(Ast::new_with_sourcepos(value, segment_sourcepos).into());
        node.insert_before(segment);
        column = end_column + 1;
    }
    node.detach();
}

fn liberal_html_segments(text: &str) -> Vec<(&str, bool)> {
    let Some(start) = text.find('<') else {
        return vec![(text, false)];
    };
    let line_end = text[start + 1..]
        .find(['\n', '\0'])
        .map_or(text.len(), |offset| start + 1 + offset);
    let Some(end) = text[start + 1..line_end].rfind('>') else {
        return vec![(text, false)];
    };
    let end = start + 1 + end + 1;
    if end <= start + 2 {
        return vec![(text, false)];
    }

    let mut segments = Vec::with_capacity(3);
    if start > 0 {
        segments.push((&text[..start], false));
    }
    segments.push((&text[start..end], true));
    if end < text.len() {
        segments.push((&text[end..], false));
    }
    segments
}

fn mark_legacy_numeric_noncharacters<'a>(
    root: &'a AstNode<'a>,
    markdown: &str,
) -> Option<(char, char)> {
    let markers = legacy_markers(markdown)?;
    let lines = markdown.lines().collect::<Vec<_>>();

    for node in root.descendants() {
        let mut ast = node.data.borrow_mut();
        let NodeValue::Text(text) = &ast.value else {
            continue;
        };
        if ast.sourcepos.start.line != ast.sourcepos.end.line {
            continue;
        }
        let Some(line) = lines.get(ast.sourcepos.start.line.saturating_sub(1)) else {
            continue;
        };
        let start = ast.sourcepos.start.column.saturating_sub(1);
        let end = ast.sourcepos.end.column.min(line.len());
        let origins = numeric_noncharacter_origins(line, start, end);
        if origins.is_empty() {
            continue;
        }

        let mut origins = origins.into_iter();
        let mut changed = false;
        let mut marked = String::with_capacity(text.len());
        for character in text.chars() {
            if character != '\u{fffe}' && character != '\u{ffff}' {
                marked.push(character);
                continue;
            }

            let origin = origins.next();
            if origin == Some((character, true)) {
                marked.push(if character == '\u{fffe}' {
                    markers.0
                } else {
                    markers.1
                });
                changed = true;
            } else {
                marked.push(character);
            }
        }
        if changed {
            ast.value = NodeValue::Text(Cow::Owned(marked));
        }
    }

    Some(markers)
}

fn legacy_markers(markdown: &str) -> Option<(char, char)> {
    let mut available = (0xe000..=0xf8ff)
        .filter_map(char::from_u32)
        .filter(|character| !markdown.contains(*character));
    Some((available.next()?, available.next()?))
}

fn numeric_noncharacter_origins(
    line: &str,
    start: usize,
    end: usize,
) -> Vec<(char, bool)> {
    let mut origins = Vec::new();
    let mut position = start;
    while position < end {
        let source = &line[position..end];
        if let Some((character, length)) = numeric_noncharacter_entity(source) {
            let escaped = line[..position]
                .bytes()
                .rev()
                .take_while(|byte| *byte == b'\\')
                .count()
                % 2
                == 1;
            if !escaped {
                origins.push((character, true));
                position += length;
                continue;
            }
        }

        let character = source.chars().next().expect("source is not empty");
        if character == '\u{fffe}' || character == '\u{ffff}' {
            origins.push((character, false));
        }
        position += character.len_utf8();
    }
    origins
}

fn numeric_noncharacter_entity(source: &str) -> Option<(char, usize)> {
    let bytes = source.as_bytes();
    if bytes.len() < 4 || bytes[0] != b'&' || bytes[1] != b'#' {
        return None;
    }

    let (radix, mut position) = if matches!(bytes.get(2), Some(b'x' | b'X')) {
        (16, 3)
    } else {
        (10, 2)
    };
    let digit_start = position;
    while position < bytes.len()
        && if radix == 16 {
            bytes[position].is_ascii_hexdigit()
        } else {
            bytes[position].is_ascii_digit()
        }
    {
        position += 1;
    }
    if position == digit_start || position >= bytes.len() || bytes[position] != b';' {
        return None;
    }

    let value = u32::from_str_radix(&source[digit_start..position], radix).ok()?;
    let character = match value {
        0xfffe => '\u{fffe}',
        0xffff => '\u{ffff}',
        _ => return None,
    };
    Some((character, position + 1))
}

fn legacy_output_value(output: String, markers: Option<(char, char)>) -> Value {
    let Some((fffe_marker, ffff_marker)) = markers else {
        return Value::String(output);
    };

    let mut replacements = Vec::new();
    if output.contains(fffe_marker) {
        replacements.push(json!([fffe_marker.to_string(), 0xfe]));
    }
    if output.contains(ffff_marker) {
        replacements.push(json!([ffff_marker.to_string(), 0xff]));
    }
    if replacements.is_empty() {
        Value::String(output)
    } else {
        json!({
            "__commonmarker_output": output,
            "legacy_bytes": replacements,
        })
    }
}

fn build_options(option_bits: u32, extensions: &[String], width: Option<usize>) -> Options<'static> {
    let mut options = Options::default();
    options.parse.smart = option_bits & (1 << 10) != 0;
    options.render.sourcepos = option_bits & (1 << 1) != 0;
    options.render.hardbreaks = option_bits & (1 << 2) != 0;
    options.render.github_pre_lang = option_bits & (1 << 11) != 0;
    options.render.r#unsafe = option_bits & (1 << 17) != 0;
    options.render.full_info_string = option_bits & (1 << 16) != 0;
    options.render.width = width.unwrap_or(0);

    for extension in extensions {
        match extension.as_str() {
            "table" => options.extension.table = true,
            "tasklist" => options.extension.tasklist = true,
            "strikethrough" => options.extension.strikethrough = true,
            "autolink" => options.extension.autolink = true,
            "tagfilter" => options.extension.tagfilter = true,
            _ => {}
        }
    }
    if option_bits & (1 << 13) != 0 {
        options.extension.footnotes = true;
    }

    options
}

impl WireNode {
    fn from_comrak<'a>(node: &'a AstNode<'a>) -> Self {
        let ast = node.data.borrow();
        let (kind, data) = value_to_wire(&ast.value);
        let sourcepos = if matches!(ast.value, NodeValue::SoftBreak) {
            [0, 0, 0, 0]
        } else {
            [
            ast.sourcepos.start.line,
            ast.sourcepos.start.column,
            ast.sourcepos.end.line,
            ast.sourcepos.end.column,
            ]
        };
        drop(ast);

        Self {
            kind: kind.to_owned(),
            data,
            sourcepos,
            children: node
                .children()
                .map(Self::from_comrak)
                .filter(|child| {
                    child.kind != "text"
                        || child
                            .data
                            .get("literal")
                            .and_then(Value::as_str)
                            .is_some_and(|literal| !literal.is_empty())
                })
                .collect(),
        }
    }

    fn to_comrak<'a>(&self, arena: &'a Arena<'a>) -> Result<&'a AstNode<'a>, String> {
        let value = wire_to_value(self)?;
        let sourcepos: Sourcepos = (
            self.sourcepos[0],
            self.sourcepos[1],
            self.sourcepos[2],
            self.sourcepos[3],
        )
            .into();
        let node = arena.alloc(Ast::new_with_sourcepos(value, sourcepos).into());
        for child in &self.children {
            node.append(child.to_comrak(arena)?);
        }
        Ok(node)
    }
}

fn value_to_wire(value: &NodeValue) -> (&'static str, Map<String, Value>) {
    let mut data = Map::new();
    let kind = match value {
        NodeValue::Document => "document",
        NodeValue::BlockQuote => "blockquote",
        NodeValue::List(list) => {
            list_to_wire(&mut data, list);
            "list"
        }
        NodeValue::Item(list) => {
            list_to_wire(&mut data, list);
            "list_item"
        }
        NodeValue::CodeBlock(code) => {
            data.insert("literal".into(), json!(code.literal));
            data.insert("fence_info".into(), json!(code.info));
            data.insert("fenced".into(), json!(code.fenced));
            data.insert("fence_char".into(), json!(code.fence_char));
            data.insert("fence_length".into(), json!(code.fence_length));
            "code_block"
        }
        NodeValue::HtmlBlock(html) => {
            data.insert("literal".into(), json!(html.literal));
            data.insert("block_type".into(), json!(html.block_type));
            "html"
        }
        NodeValue::Paragraph => "paragraph",
        NodeValue::Heading(heading) => {
            data.insert("header_level".into(), json!(heading.level));
            data.insert("setext".into(), json!(heading.setext));
            "header"
        }
        NodeValue::ThematicBreak => "hrule",
        NodeValue::FootnoteDefinition(footnote) => {
            data.insert("literal".into(), json!(footnote.name));
            data.insert("total_references".into(), json!(footnote.total_references));
            "footnote_definition"
        }
        NodeValue::Table(table) => {
            data.insert(
                "table_alignments".into(),
                json!(table
                    .alignments
                    .iter()
                    .map(alignment_name)
                    .collect::<Vec<_>>()),
            );
            "table"
        }
        NodeValue::TableRow(header) => {
            if *header {
                "table_header"
            } else {
                "table_row"
            }
        }
        NodeValue::TableCell => "table_cell",
        NodeValue::Text(text) => {
            data.insert("literal".into(), json!(text));
            "text"
        }
        NodeValue::TaskItem(item) => {
            data.insert("tasklist".into(), json!(true));
            data.insert("checked".into(), json!(item.symbol.is_some()));
            "list_item"
        }
        NodeValue::SoftBreak => "softbreak",
        NodeValue::LineBreak => "linebreak",
        NodeValue::Code(code) => {
            data.insert("literal".into(), json!(code.literal));
            "code"
        }
        NodeValue::HtmlInline(html) => {
            data.insert("literal".into(), json!(html));
            "inline_html"
        }
        NodeValue::Emph => "emph",
        NodeValue::Strong => "strong",
        NodeValue::Strikethrough => "strikethrough",
        NodeValue::Link(link) => {
            link_to_wire(&mut data, link);
            "link"
        }
        NodeValue::Image(link) => {
            link_to_wire(&mut data, link);
            "image"
        }
        NodeValue::FootnoteReference(footnote) => {
            data.insert("literal".into(), json!(footnote.ix.to_string()));
            data.insert("ref_num".into(), json!(footnote.ref_num));
            data.insert("ix".into(), json!(footnote.ix));
            "footnote_reference"
        }
        other => {
            data.insert("debug".into(), json!(format!("{other:?}")));
            "unsupported"
        }
    };
    (kind, data)
}

fn list_to_wire(data: &mut Map<String, Value>, list: &NodeList) {
    data.insert(
        "list_type".into(),
        json!(match list.list_type {
            ListType::Bullet => "bullet_list",
            ListType::Ordered => "ordered_list",
        }),
    );
    data.insert("list_start".into(), json!(list.start));
    data.insert("list_tight".into(), json!(list.tight));
    data.insert(
        "delimiter".into(),
        json!(match list.delimiter {
            ListDelimType::Period => "period",
            ListDelimType::Paren => "paren",
        }),
    );
    data.insert("bullet_char".into(), json!(list.bullet_char));
}

fn link_to_wire(data: &mut Map<String, Value>, link: &NodeLink) {
    data.insert("url".into(), json!(link.url));
    data.insert("title".into(), json!(link.title));
}

fn wire_to_value(node: &WireNode) -> Result<NodeValue, String> {
    let value = match node.kind.as_str() {
        "document" => NodeValue::Document,
        "blockquote" => NodeValue::BlockQuote,
        "list" => NodeValue::List(wire_to_list(&node.data)),
        "list_item" if bool_value(&node.data, "tasklist") => {
            NodeValue::TaskItem(NodeTaskItem {
                symbol: bool_value(&node.data, "checked").then_some('x'),
                symbol_sourcepos: sourcepos(node.sourcepos),
            })
        }
        "list_item" => NodeValue::Item(wire_to_list(&node.data)),
        "code_block" => NodeValue::CodeBlock(Box::new(NodeCodeBlock {
            fenced: bool_value(&node.data, "fenced"),
            fence_char: usize_value(&node.data, "fence_char") as u8,
            fence_length: usize_value(&node.data, "fence_length"),
            info: string_value(&node.data, "fence_info"),
            literal: string_value(&node.data, "literal"),
            closed: true,
            ..Default::default()
        })),
        "html" => NodeValue::HtmlBlock(NodeHtmlBlock {
            block_type: usize_value(&node.data, "block_type") as u8,
            literal: string_value(&node.data, "literal"),
        }),
        "paragraph" => NodeValue::Paragraph,
        "header" => NodeValue::Heading(NodeHeading {
            level: usize_value(&node.data, "header_level") as u8,
            setext: bool_value(&node.data, "setext"),
            closed: false,
        }),
        "hrule" => NodeValue::ThematicBreak,
        "footnote_definition" => NodeValue::FootnoteDefinition(NodeFootnoteDefinition {
            name: string_value(&node.data, "literal"),
            total_references: usize_value(&node.data, "total_references") as u32,
        }),
        "table" => {
            let alignments = node
                .data
                .get("table_alignments")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|alignment| match alignment.as_str() {
                    Some("left") => TableAlignment::Left,
                    Some("center") => TableAlignment::Center,
                    Some("right") => TableAlignment::Right,
                    _ => TableAlignment::None,
                })
                .collect::<Vec<_>>();
            NodeValue::Table(Box::new(NodeTable {
                num_columns: alignments.len(),
                alignments,
                ..Default::default()
            }))
        }
        "table_header" => NodeValue::TableRow(true),
        "table_row" => NodeValue::TableRow(false),
        "table_cell" => NodeValue::TableCell,
        "text" => NodeValue::Text(string_value(&node.data, "literal").into()),
        "softbreak" => NodeValue::SoftBreak,
        "linebreak" => NodeValue::LineBreak,
        "code" => NodeValue::Code(NodeCode {
            literal: string_value(&node.data, "literal"),
            ..Default::default()
        }),
        "inline_html" => NodeValue::HtmlInline(string_value(&node.data, "literal")),
        "emph" => NodeValue::Emph,
        "strong" => NodeValue::Strong,
        "strikethrough" => NodeValue::Strikethrough,
        "link" => NodeValue::Link(Box::new(NodeLink {
            url: string_value(&node.data, "url"),
            title: string_value(&node.data, "title"),
        })),
        "image" => NodeValue::Image(Box::new(NodeLink {
            url: string_value(&node.data, "url"),
            title: string_value(&node.data, "title"),
        })),
        "footnote_reference" => NodeValue::FootnoteReference(Box::new(NodeFootnoteReference {
            name: string_value(&node.data, "literal"),
            ref_num: usize_value(&node.data, "ref_num") as u32,
            ix: usize_value(&node.data, "ix") as u32,
            ..Default::default()
        })),
        kind => return Err(format!("unsupported node type `{kind}`")),
    };
    Ok(value)
}

fn wire_to_list(data: &Map<String, Value>) -> NodeList {
    NodeList {
        list_type: if string_value(data, "list_type") == "ordered_list" {
            ListType::Ordered
        } else {
            ListType::Bullet
        },
        start: usize_value(data, "list_start"),
        tight: bool_value(data, "list_tight"),
        delimiter: if string_value(data, "delimiter") == "paren" {
            ListDelimType::Paren
        } else {
            ListDelimType::Period
        },
        bullet_char: usize_value(data, "bullet_char") as u8,
        ..Default::default()
    }
}

fn string_value(data: &Map<String, Value>, key: &str) -> String {
    data.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn usize_value(data: &Map<String, Value>, key: &str) -> usize {
    data.get(key)
        .and_then(Value::as_u64)
        .unwrap_or_default() as usize
}

fn bool_value(data: &Map<String, Value>, key: &str) -> bool {
    data.get(key)
        .and_then(Value::as_bool)
        .unwrap_or_default()
}

fn sourcepos(value: [usize; 4]) -> Sourcepos {
    (value[0], value[1], value[2], value[3]).into()
}

fn alignment_name(alignment: &TableAlignment) -> &'static str {
    match alignment {
        TableAlignment::None => "none",
        TableAlignment::Left => "left",
        TableAlignment::Center => "center",
        TableAlignment::Right => "right",
    }
}

#[derive(Clone, Copy)]
struct PlaintextOptions {
    collapse_spaces: bool,
    softbreak_as_newline: bool,
}

fn format_plaintext<'a>(
    node: &'a AstNode<'a>,
    output: &mut String,
    options: PlaintextOptions,
) {
    let children = node.children().collect::<Vec<_>>();
    for (index, child) in children.iter().enumerate() {
        format_plain_block(child, output, options);
        if index + 1 < children.len() {
            if !output.ends_with('\n') {
                output.push('\n');
            }
            if !output.ends_with("\n\n") {
                output.push('\n');
            }
        }
    }
    if !output.ends_with('\n') {
        output.push('\n');
    }
}

fn format_plain_block<'a>(
    node: &'a AstNode<'a>,
    output: &mut String,
    options: PlaintextOptions,
) {
    use std::fmt::Write;

    match node.data.borrow().value.clone() {
        NodeValue::Paragraph => format_plain_inlines(node, output, options),
        NodeValue::List(list) => {
            for (index, item) in node.children().enumerate() {
                if list.list_type == ListType::Ordered {
                    let _ = write!(output, "{}.  ", list.start + index);
                } else {
                    output.push_str("  - ");
                }
                if let Some(first) = item.first_child() {
                    format_plain_block(first, output, options);
                }
                output.push('\n');
            }
            while output.ends_with("\n\n") {
                output.pop();
            }
        }
        NodeValue::Table(_) => {
            for row in node.children() {
                let header = matches!(row.data.borrow().value, NodeValue::TableRow(true));
                output.push('|');
                for cell in row.children() {
                    output.push(' ');
                    format_plain_inlines(cell, output, options);
                    output.push_str(" |");
                }
                output.push('\n');
                if header {
                    output.push('|');
                    for _ in row.children() {
                        output.push_str(" --- |");
                    }
                    output.push('\n');
                }
            }
            output.pop();
        }
        NodeValue::CodeBlock(code) => output.push_str(code.literal.trim_end_matches('\n')),
        NodeValue::BlockQuote => {
            for child in node.children() {
                format_plain_block(child, output, options);
            }
        }
        _ => format_plain_inlines(node, output, options),
    }
}

fn format_plain_inlines<'a>(
    node: &'a AstNode<'a>,
    output: &mut String,
    options: PlaintextOptions,
) {
    match node.data.borrow().value.clone() {
        NodeValue::Text(text) if options.collapse_spaces => {
            let mut previous_was_space = false;
            for character in text.chars() {
                if character == ' ' && previous_was_space {
                    continue;
                }
                previous_was_space = character == ' ';
                output.push(character);
            }
        }
        NodeValue::Text(text) => output.push_str(&text),
        NodeValue::Code(code) => output.push_str(&code.literal),
        NodeValue::SoftBreak if options.softbreak_as_newline => output.push('\n'),
        NodeValue::SoftBreak => output.push(' '),
        NodeValue::LineBreak => output.push('\n'),
        NodeValue::HtmlBlock(html) => output.push_str(&html.literal),
        NodeValue::HtmlInline(html) => output.push_str(&html),
        NodeValue::FootnoteReference(footnote) => output.push_str(&footnote.name),
        NodeValue::Strikethrough => {
            output.push('~');
            for child in node.children() {
                format_plain_inlines(child, output, options);
            }
            output.push('~');
        }
        _ => {
            for child in node.children() {
                format_plain_inlines(child, output, options);
            }
        }
    }
}

fn failure(error: impl ToString) -> Response {
    Response::Failure {
        ok: false,
        error: error.to_string(),
    }
}
