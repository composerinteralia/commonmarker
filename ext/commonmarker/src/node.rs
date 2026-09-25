use std::borrow::Cow;
use std::cell::RefCell;

use comrak::arena_tree::Node as ArenaNode;
use comrak::nodes::{
    AlertType, Ast, AstNode, ListDelimType, ListType, NodeAlert, NodeBlockDirective, NodeCode,
    NodeCodeBlock, NodeDescriptionItem, NodeFootnoteDefinition, NodeFootnoteReference, NodeHeading,
    NodeHtmlBlock, NodeLink, NodeList, NodeMath, NodeMultilineBlockQuote, NodeShortCode, NodeTable,
    NodeTaskItem, NodeValue, NodeWikiLink, Sourcepos, TableAlignment,
};
use comrak::Arena;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::CallError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireNode {
    pub kind: String,
    #[serde(default)]
    pub data: Map<String, Value>,
    #[serde(default)]
    pub source_position: [usize; 4],
    #[serde(default)]
    pub children: Vec<WireNode>,
}

impl WireNode {
    pub fn from_comrak<'a>(node: &'a AstNode<'a>) -> Self {
        let ast = node.data.borrow();
        let (kind, data) = value_to_wire(&ast.value);
        let source_position = [
            ast.sourcepos.start.line,
            ast.sourcepos.start.column,
            ast.sourcepos.end.line,
            ast.sourcepos.end.column,
        ];
        drop(ast);

        Self {
            kind: kind.to_owned(),
            data,
            source_position,
            children: node.children().map(Self::from_comrak).collect(),
        }
    }

    pub fn to_comrak<'a>(&self, arena: &'a Arena<'a>) -> Result<&'a AstNode<'a>, CallError> {
        let value = wire_to_value(self)?;
        let sourcepos: Sourcepos = (
            self.source_position[0],
            self.source_position[1],
            self.source_position[2],
            self.source_position[3],
        )
            .into();
        let node = arena.alloc(ArenaNode::new(RefCell::new(Ast::new_with_sourcepos(
            value, sourcepos,
        ))));

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
        NodeValue::FrontMatter(literal) => {
            data.insert("literal".into(), json!(literal));
            "frontmatter"
        }
        NodeValue::BlockQuote => "block_quote",
        NodeValue::List(list) => {
            insert_list(&mut data, list);
            "list"
        }
        NodeValue::Item(list) => {
            insert_list(&mut data, list);
            "item"
        }
        NodeValue::DescriptionList => "description_list",
        NodeValue::DescriptionItem(item) => {
            data.insert("marker_offset".into(), json!(item.marker_offset));
            data.insert("padding".into(), json!(item.padding));
            data.insert("tight".into(), json!(item.tight));
            "description_item"
        }
        NodeValue::DescriptionTerm => "description_term",
        NodeValue::DescriptionDetails => "description_details",
        NodeValue::CodeBlock(block) => {
            data.insert("fenced".into(), json!(block.fenced));
            data.insert("fence_char".into(), json!(block.fence_char));
            data.insert("fence_length".into(), json!(block.fence_length));
            data.insert("fence_offset".into(), json!(block.fence_offset));
            data.insert("info".into(), json!(block.info));
            data.insert("literal".into(), json!(block.literal));
            data.insert("closed".into(), json!(block.closed));
            "code_block"
        }
        NodeValue::HtmlBlock(block) => {
            data.insert("block_type".into(), json!(block.block_type));
            data.insert("literal".into(), json!(block.literal));
            "html_block"
        }
        NodeValue::Paragraph => "paragraph",
        NodeValue::Heading(heading) => {
            data.insert("level".into(), json!(heading.level));
            data.insert("setext".into(), json!(heading.setext));
            data.insert("closed".into(), json!(heading.closed));
            "heading"
        }
        NodeValue::ThematicBreak => "thematic_break",
        NodeValue::FootnoteDefinition(footnote) => {
            data.insert("name".into(), json!(footnote.name));
            data.insert("total_references".into(), json!(footnote.total_references));
            "footnote_definition"
        }
        NodeValue::Table(table) => {
            data.insert(
                "alignments".into(),
                json!(table
                    .alignments
                    .iter()
                    .map(alignment_name)
                    .collect::<Vec<_>>()),
            );
            data.insert("num_columns".into(), json!(table.num_columns));
            data.insert("num_rows".into(), json!(table.num_rows));
            data.insert(
                "num_nonempty_cells".into(),
                json!(table.num_nonempty_cells),
            );
            "table"
        }
        NodeValue::TableRow(header) => {
            data.insert("header".into(), json!(header));
            "table_row"
        }
        NodeValue::TableCell => "table_cell",
        NodeValue::Text(content) => {
            data.insert("content".into(), json!(content));
            "text"
        }
        NodeValue::TaskItem(item) => {
            data.insert(
                "mark".into(),
                item.symbol
                    .map(|symbol| json!(symbol.to_string()))
                    .unwrap_or(Value::Null),
            );
            "taskitem"
        }
        NodeValue::SoftBreak => "softbreak",
        NodeValue::LineBreak => "linebreak",
        NodeValue::Code(code) => {
            data.insert("num_backticks".into(), json!(code.num_backticks));
            data.insert("literal".into(), json!(code.literal));
            "code"
        }
        NodeValue::HtmlInline(content) => {
            data.insert("content".into(), json!(content));
            "html_inline"
        }
        NodeValue::Raw(content) => {
            data.insert("content".into(), json!(content));
            "raw"
        }
        NodeValue::Emph => "emph",
        NodeValue::Strong => "strong",
        NodeValue::Strikethrough => "strikethrough",
        NodeValue::Highlight => "highlight",
        NodeValue::Insert => "insert",
        NodeValue::Superscript => "superscript",
        NodeValue::Link(link) => {
            insert_link(&mut data, link);
            "link"
        }
        NodeValue::Image(link) => {
            insert_link(&mut data, link);
            "image"
        }
        NodeValue::FootnoteReference(reference) => {
            data.insert("name".into(), json!(reference.name));
            data.insert("texts".into(), json!(reference.texts));
            data.insert("ref_num".into(), json!(reference.ref_num));
            data.insert("ix".into(), json!(reference.ix));
            "footnote_reference"
        }
        NodeValue::ShortCode(shortcode) => {
            data.insert("code".into(), json!(shortcode.code));
            data.insert("emoji".into(), json!(shortcode.emoji));
            "shortcode"
        }
        NodeValue::Math(math) => {
            data.insert("dollar_math".into(), json!(math.dollar_math));
            data.insert("display_math".into(), json!(math.display_math));
            data.insert("literal".into(), json!(math.literal));
            "math"
        }
        NodeValue::MultilineBlockQuote(quote) => {
            data.insert("fence_length".into(), json!(quote.fence_length));
            data.insert("fence_offset".into(), json!(quote.fence_offset));
            "multiline_block_quote"
        }
        NodeValue::Escaped => "escaped",
        NodeValue::WikiLink(link) => {
            data.insert("url".into(), json!(link.url));
            "wikilink"
        }
        NodeValue::Underline => "underline",
        NodeValue::Subscript => "subscript",
        NodeValue::SpoileredText => "spoiler",
        NodeValue::EscapedTag(tag) => {
            data.insert("tag".into(), json!(tag));
            "escaped_tag"
        }
        NodeValue::Alert(alert) => {
            data.insert("type".into(), json!(alert_type_name(alert.alert_type)));
            data.insert("title".into(), json!(alert.title));
            data.insert("multiline".into(), json!(alert.multiline));
            data.insert("fence_length".into(), json!(alert.fence_length));
            data.insert("fence_offset".into(), json!(alert.fence_offset));
            "alert"
        }
        NodeValue::Subtext => "subtext",
        NodeValue::BlockDirective(directive) => {
            data.insert("fence_length".into(), json!(directive.fence_length));
            data.insert("fence_offset".into(), json!(directive.fence_offset));
            data.insert("info".into(), json!(directive.info));
            "block_directive"
        }
    };

    (kind, data)
}

fn wire_to_value(node: &WireNode) -> Result<NodeValue, CallError> {
    let data = &node.data;
    Ok(match node.kind.as_str() {
        "document" => NodeValue::Document,
        "frontmatter" => NodeValue::FrontMatter(string(data, "literal")),
        "block_quote" => NodeValue::BlockQuote,
        "list" => NodeValue::List(read_list(data)),
        "item" => NodeValue::Item(read_list(data)),
        "description_list" => NodeValue::DescriptionList,
        "description_item" => NodeValue::DescriptionItem(NodeDescriptionItem {
            marker_offset: usize_value(data, "marker_offset"),
            padding: usize_value(data, "padding"),
            tight: bool_value(data, "tight"),
        }),
        "description_term" => NodeValue::DescriptionTerm,
        "description_details" => NodeValue::DescriptionDetails,
        "code_block" => NodeValue::CodeBlock(Box::new(NodeCodeBlock {
            fenced: bool_value(data, "fenced"),
            fence_char: u8_value(data, "fence_char", b'`'),
            fence_length: usize_value(data, "fence_length"),
            fence_offset: usize_value(data, "fence_offset"),
            info: string(data, "info"),
            literal: string(data, "literal"),
            closed: data.get("closed").and_then(Value::as_bool).unwrap_or(true),
        })),
        "html_block" => NodeValue::HtmlBlock(NodeHtmlBlock {
            block_type: u8_value(data, "block_type", 0),
            literal: string(data, "literal"),
        }),
        "paragraph" => NodeValue::Paragraph,
        "heading" => NodeValue::Heading(NodeHeading {
            level: u8_value(data, "level", 1),
            setext: bool_value(data, "setext"),
            closed: bool_value(data, "closed"),
        }),
        "thematic_break" => NodeValue::ThematicBreak,
        "footnote_definition" => NodeValue::FootnoteDefinition(NodeFootnoteDefinition {
            name: string(data, "name"),
            total_references: u32_value(data, "total_references", 1),
        }),
        "table" => NodeValue::Table(Box::new(NodeTable {
            alignments: data
                .get("alignments")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|value| match value.as_str() {
                    Some("left") => TableAlignment::Left,
                    Some("center") => TableAlignment::Center,
                    Some("right") => TableAlignment::Right,
                    _ => TableAlignment::None,
                })
                .collect(),
            num_columns: usize_value(data, "num_columns"),
            num_rows: usize_value(data, "num_rows"),
            num_nonempty_cells: usize_value(data, "num_nonempty_cells"),
        })),
        "table_row" => NodeValue::TableRow(bool_value(data, "header")),
        "table_cell" => NodeValue::TableCell,
        "text" => NodeValue::Text(Cow::Owned(string(data, "content"))),
        "taskitem" => NodeValue::TaskItem(NodeTaskItem {
            symbol: data
                .get("mark")
                .and_then(Value::as_str)
                .and_then(|value| value.chars().next()),
            symbol_sourcepos: (0, 0, 0, 0).into(),
        }),
        "softbreak" => NodeValue::SoftBreak,
        "linebreak" => NodeValue::LineBreak,
        "code" => NodeValue::Code(NodeCode {
            num_backticks: data
                .get("num_backticks")
                .and_then(Value::as_u64)
                .unwrap_or(1) as usize,
            literal: string(data, "literal"),
        }),
        "html_inline" => NodeValue::HtmlInline(string(data, "content")),
        "raw" => NodeValue::Raw(string(data, "content")),
        "emph" => NodeValue::Emph,
        "strong" => NodeValue::Strong,
        "strikethrough" => NodeValue::Strikethrough,
        "highlight" => NodeValue::Highlight,
        "insert" => NodeValue::Insert,
        "superscript" => NodeValue::Superscript,
        "link" => NodeValue::Link(Box::new(read_link(data))),
        "image" => NodeValue::Image(Box::new(read_link(data))),
        "footnote_reference" => NodeValue::FootnoteReference(Box::new(NodeFootnoteReference {
            name: string(data, "name"),
            texts: data
                .get("texts")
                .cloned()
                .map(serde_json::from_value)
                .transpose()
                .map_err(|error| CallError::argument_error(error.to_string()))?
                .unwrap_or_default(),
            ref_num: u32_value(data, "ref_num", 0),
            ix: u32_value(data, "ix", 0),
        })),
        "shortcode" => NodeValue::ShortCode(Box::new(NodeShortCode {
            code: string(data, "code"),
            emoji: string(data, "emoji"),
        })),
        "math" => NodeValue::Math(NodeMath {
            dollar_math: bool_value(data, "dollar_math"),
            display_math: bool_value(data, "display_math"),
            literal: string(data, "literal"),
        }),
        "multiline_block_quote" => {
            NodeValue::MultilineBlockQuote(NodeMultilineBlockQuote {
                fence_length: usize_value(data, "fence_length"),
                fence_offset: usize_value(data, "fence_offset"),
            })
        }
        "escaped" => NodeValue::Escaped,
        "wikilink" => NodeValue::WikiLink(NodeWikiLink {
            url: string(data, "url"),
        }),
        "underline" => NodeValue::Underline,
        "subscript" => NodeValue::Subscript,
        "spoiler" => NodeValue::SpoileredText,
        "escaped_tag" => {
            let tag = string(data, "tag");
            let tag: &'static str = Box::leak(tag.into_boxed_str());
            NodeValue::EscapedTag(tag)
        }
        "alert" => NodeValue::Alert(Box::new(NodeAlert {
            alert_type: parse_alert_type(data.get("type").and_then(Value::as_str))?,
            title: data
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_owned),
            multiline: bool_value(data, "multiline"),
            fence_length: usize_value(data, "fence_length"),
            fence_offset: usize_value(data, "fence_offset"),
        })),
        "subtext" => NodeValue::Subtext,
        "block_directive" => NodeValue::BlockDirective(Box::new(NodeBlockDirective {
            fence_length: usize_value(data, "fence_length"),
            fence_offset: usize_value(data, "fence_offset"),
            info: string(data, "info"),
        })),
        kind => {
            return Err(CallError::argument_error(format!(
                "unknown node type `{kind}`"
            )))
        }
    })
}

fn insert_list(data: &mut Map<String, Value>, list: &NodeList) {
    data.insert(
        "type".into(),
        json!(match list.list_type {
            ListType::Bullet => "bullet",
            ListType::Ordered => "ordered",
        }),
    );
    data.insert("marker_offset".into(), json!(list.marker_offset));
    data.insert("padding".into(), json!(list.padding));
    data.insert("start".into(), json!(list.start));
    data.insert(
        "delimiter".into(),
        json!(match list.delimiter {
            ListDelimType::Period => ".",
            ListDelimType::Paren => ")",
        }),
    );
    data.insert("bullet_char".into(), json!(list.bullet_char));
    data.insert("tight".into(), json!(list.tight));
    data.insert("task_list".into(), json!(list.is_task_list));
}

fn read_list(data: &Map<String, Value>) -> NodeList {
    NodeList {
        list_type: if data.get("type").and_then(Value::as_str) == Some("ordered") {
            ListType::Ordered
        } else {
            ListType::Bullet
        },
        marker_offset: usize_value(data, "marker_offset"),
        padding: usize_value(data, "padding"),
        start: usize_value(data, "start"),
        delimiter: if data.get("delimiter").and_then(Value::as_str) == Some(")") {
            ListDelimType::Paren
        } else {
            ListDelimType::Period
        },
        bullet_char: u8_value(data, "bullet_char", 0),
        tight: bool_value(data, "tight"),
        is_task_list: bool_value(data, "task_list"),
    }
}

fn insert_link(data: &mut Map<String, Value>, link: &NodeLink) {
    data.insert("url".into(), json!(link.url));
    data.insert("title".into(), json!(link.title));
}

fn read_link(data: &Map<String, Value>) -> NodeLink {
    NodeLink {
        url: string(data, "url"),
        title: string(data, "title"),
    }
}

fn alignment_name(alignment: &TableAlignment) -> &'static str {
    match alignment {
        TableAlignment::None => "none",
        TableAlignment::Left => "left",
        TableAlignment::Center => "center",
        TableAlignment::Right => "right",
    }
}

fn alert_type_name(alert_type: AlertType) -> &'static str {
    match alert_type {
        AlertType::Note => "note",
        AlertType::Tip => "tip",
        AlertType::Important => "important",
        AlertType::Warning => "warning",
        AlertType::Caution => "caution",
    }
}

fn parse_alert_type(value: Option<&str>) -> Result<AlertType, CallError> {
    match value.unwrap_or("note") {
        "note" => Ok(AlertType::Note),
        "tip" => Ok(AlertType::Tip),
        "important" => Ok(AlertType::Important),
        "warning" => Ok(AlertType::Warning),
        "caution" => Ok(AlertType::Caution),
        value => Err(CallError::argument_error(format!(
            "invalid alert type `{value}`"
        ))),
    }
}

fn string(data: &Map<String, Value>, key: &str) -> String {
    data.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn bool_value(data: &Map<String, Value>, key: &str) -> bool {
    data.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn usize_value(data: &Map<String, Value>, key: &str) -> usize {
    data.get(key).and_then(Value::as_u64).unwrap_or(0) as usize
}

fn u8_value(data: &Map<String, Value>, key: &str, default: u8) -> u8 {
    data.get(key)
        .and_then(Value::as_u64)
        .map(|value| value as u8)
        .unwrap_or(default)
}

fn u32_value(data: &Map<String, Value>, key: &str, default: u32) -> u32 {
    data.get(key)
        .and_then(Value::as_u64)
        .map(|value| value as u32)
        .unwrap_or(default)
}
