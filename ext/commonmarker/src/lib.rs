use std::ffi::{c_char, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::slice;

use comrak::options::Plugins;
use comrak::{parse_document, Arena};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use node::WireNode;

mod node;
mod options;
mod plugins;

#[derive(Debug, Deserialize)]
struct Request {
    operation: String,
    #[serde(default)]
    markdown: String,
    #[serde(default)]
    options: Value,
    #[serde(default)]
    plugins: Value,
    node: Option<WireNode>,
    format: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum Response {
    Success { ok: bool, value: Value },
    Failure { ok: bool, error: ErrorResponse },
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    kind: &'static str,
    message: String,
}

#[derive(Debug)]
pub struct CallError {
    kind: &'static str,
    message: String,
}

impl CallError {
    pub fn argument_error(message: impl Into<String>) -> Self {
        Self {
            kind: "argument",
            message: message.into(),
        }
    }

    pub fn type_error(message: impl Into<String>) -> Self {
        Self {
            kind: "type",
            message: message.into(),
        }
    }

    fn runtime_error(message: impl Into<String>) -> Self {
        Self {
            kind: "runtime",
            message: message.into(),
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn commonmarker_call(input: *const u8, input_len: usize) -> *mut c_char {
    let response = catch_unwind(AssertUnwindSafe(|| {
        if input.is_null() {
            return failure(CallError::argument_error("input pointer cannot be null"));
        }

        let bytes = unsafe { slice::from_raw_parts(input, input_len) };
        let request = serde_json::from_slice(bytes)
            .map_err(|error| CallError::argument_error(format!("invalid request: {error}")));

        match request.and_then(handle_request) {
            Ok(value) => Response::Success {
                ok: true,
                value,
            },
            Err(error) => failure(error),
        }
    }))
    .unwrap_or_else(|payload| {
        let message = payload
            .downcast_ref::<&str>()
            .map(|message| (*message).to_owned())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "Rust panic".to_owned());
        failure(CallError::runtime_error(message))
    });

    let json = serde_json::to_string(&response).unwrap_or_else(|error| {
        format!(
            r#"{{"ok":false,"error":{{"kind":"runtime","message":"response serialization failed: {error}"}}}}"#
        )
    });
    CString::new(json)
        .expect("JSON responses cannot contain null bytes")
        .into_raw()
}

#[no_mangle]
pub unsafe extern "C" fn commonmarker_free(pointer: *mut c_char) {
    if !pointer.is_null() {
        drop(unsafe { CString::from_raw(pointer) });
    }
}

fn handle_request(request: Request) -> Result<Value, CallError> {
    match request.operation.as_str() {
        "parse" => parse(&request),
        "render_markdown" => render_markdown(&request),
        "render_ast" => render_ast(&request),
        operation => Err(CallError::argument_error(format!(
            "unknown operation `{operation}`"
        ))),
    }
}

fn parse(request: &Request) -> Result<Value, CallError> {
    let options = options::build_options(&request.options);
    let arena = Arena::new();
    let root = parse_document(&arena, &request.markdown, &options);
    serde_json::to_value(WireNode::from_comrak(root))
        .map_err(|error| CallError::runtime_error(error.to_string()))
}

fn render_markdown(request: &Request) -> Result<Value, CallError> {
    let options = options::build_options(&request.options);
    let adapter = plugins::syntax_highlighter(&request.plugins)?;
    let mut comrak_plugins = Plugins::default();
    comrak_plugins.render.codefence_syntax_highlighter = adapter
        .as_ref()
        .map(|adapter| adapter as &dyn comrak::adapters::SyntaxHighlighterAdapter);

    let arena = Arena::new();
    let root = parse_document(&arena, &request.markdown, &options);
    let mut output = String::with_capacity(request.markdown.len() * 2);
    comrak::html::format_document_with_plugins(
        root,
        &options,
        &mut output,
        &comrak_plugins,
    )
    .map_err(|error| CallError::runtime_error(error.to_string()))?;

    Ok(Value::String(output))
}

fn render_ast(request: &Request) -> Result<Value, CallError> {
    let wire_node = request
        .node
        .as_ref()
        .ok_or_else(|| CallError::argument_error("render_ast requires a node"))?;
    let options = options::build_options(&request.options);
    let adapter = plugins::syntax_highlighter(&request.plugins)?;
    let mut comrak_plugins = Plugins::default();
    comrak_plugins.render.codefence_syntax_highlighter = adapter
        .as_ref()
        .map(|adapter| adapter as &dyn comrak::adapters::SyntaxHighlighterAdapter);

    let arena = Arena::new();
    let root = wire_node.to_comrak(&arena)?;
    let mut output = String::new();
    match request.format.as_deref().unwrap_or("html") {
        "html" => comrak::format_html_with_plugins(
            root,
            &options,
            &mut output,
            &comrak_plugins,
        ),
        "commonmark" => comrak::format_commonmark_with_plugins(
            root,
            &options,
            &mut output,
            &comrak_plugins,
        ),
        format => {
            return Err(CallError::argument_error(format!(
                "unknown render format `{format}`"
            )))
        }
    }
    .map_err(|error| CallError::runtime_error(error.to_string()))?;

    Ok(Value::String(output))
}

fn failure(error: CallError) -> Response {
    Response::Failure {
        ok: false,
        error: ErrorResponse {
            kind: error.kind,
            message: error.message,
        },
    }
}
