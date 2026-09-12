use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<serde_json::Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: serde_json::Value, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
}

impl JsonRpcNotification {
    pub fn new(method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

impl Range {
    pub fn new(start_line: u32, start_char: u32, end_line: u32, end_char: u32) -> Self {
        Self {
            start: Position {
                line: start_line,
                character: start_char,
            },
            end: Position {
                line: end_line,
                character: end_char,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub severity: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    pub message: String,
}

pub struct DiagnosticSeverity;
impl DiagnosticSeverity {
    pub const ERROR: i32 = 1;
    pub const WARNING: i32 = 2;
    pub const INFORMATION: i32 = 3;
    pub const HINT: i32 = 4;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishDiagnosticsParams {
    pub uri: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    #[serde(rename = "rootUri")]
    pub root_uri: Option<String>,
    #[serde(rename = "rootPath")]
    pub root_path: Option<String>,
    pub capabilities: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeResult {
    pub capabilities: ServerCapabilities,
    #[serde(rename = "serverInfo")]
    pub server_info: ServerInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerCapabilities {
    #[serde(rename = "textDocumentSync")]
    pub text_document_sync: Option<i32>,
    #[serde(rename = "hoverProvider")]
    pub hover_provider: Option<bool>,
    #[serde(rename = "completionProvider")]
    pub completion_provider: Option<CompletionOptions>,
    #[serde(rename = "definitionProvider")]
    pub definition_provider: Option<bool>,
    #[serde(rename = "documentSymbolProvider")]
    pub document_symbol_provider: Option<bool>,
    #[serde(rename = "documentFormattingProvider")]
    pub document_formatting_provider: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompletionOptions {
    #[serde(rename = "triggerCharacters")]
    pub trigger_characters: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentItem {
    pub uri: String,
    #[serde(rename = "languageId")]
    pub language_id: String,
    pub version: i32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidOpenTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentItem,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionedTextDocumentIdentifier {
    pub uri: String,
    pub version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentContentChangeEvent {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidChangeTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub text_document: VersionedTextDocumentIdentifier,
    #[serde(rename = "contentChanges")]
    pub content_changes: Vec<TextDocumentContentChangeEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentIdentifier {
    pub uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DidCloseTextDocumentParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextDocumentPositionParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkupContent {
    pub kind: String,
    pub value: String,
}

impl MarkupContent {
    pub fn markdown(value: impl Into<String>) -> Self {
        Self {
            kind: "markdown".to_string(),
            value: value.into(),
        }
    }

    pub fn plaintext(value: impl Into<String>) -> Self {
        Self {
            kind: "plaintext".to_string(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hover {
    pub contents: MarkupContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<Range>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionItem {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub documentation: Option<MarkupContent>,
    #[serde(rename = "insertText", skip_serializing_if = "Option::is_none")]
    pub insert_text: Option<String>,
    #[serde(rename = "insertTextFormat", skip_serializing_if = "Option::is_none")]
    pub insert_text_format: Option<i32>,
}

pub struct CompletionItemKind;
impl CompletionItemKind {
    pub const TEXT: i32 = 1;
    pub const METHOD: i32 = 2;
    pub const FUNCTION: i32 = 3;
    pub const CONSTRUCTOR: i32 = 4;
    pub const FIELD: i32 = 5;
    pub const VARIABLE: i32 = 6;
    pub const CLASS: i32 = 7;
    pub const INTERFACE: i32 = 8;
    pub const MODULE: i32 = 9;
    pub const PROPERTY: i32 = 10;
    pub const UNIT: i32 = 11;
    pub const VALUE: i32 = 12;
    pub const ENUM: i32 = 13;
    pub const KEYWORD: i32 = 14;
    pub const SNIPPET: i32 = 15;
    pub const COLOR: i32 = 16;
    pub const FILE: i32 = 17;
    pub const REFERENCE: i32 = 18;
    pub const ENUM_MEMBER: i32 = 20;
    pub const CONSTANT: i32 = 21;
    pub const STRUCT: i32 = 22;
}

pub struct InsertTextFormat;
impl InsertTextFormat {
    pub const PLAIN_TEXT: i32 = 1;
    pub const SNIPPET: i32 = 2;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSymbol {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub kind: i32,
    pub range: Range,
    #[serde(rename = "selectionRange")]
    pub selection_range: Range,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<DocumentSymbol>>,
}

pub struct SymbolKind;
impl SymbolKind {
    pub const FILE: i32 = 1;
    pub const MODULE: i32 = 2;
    pub const NAMESPACE: i32 = 3;
    pub const PACKAGE: i32 = 4;
    pub const CLASS: i32 = 5;
    pub const METHOD: i32 = 6;
    pub const PROPERTY: i32 = 7;
    pub const FIELD: i32 = 8;
    pub const CONSTRUCTOR: i32 = 9;
    pub const ENUM: i32 = 10;
    pub const INTERFACE: i32 = 11;
    pub const FUNCTION: i32 = 12;
    pub const VARIABLE: i32 = 13;
    pub const CONSTANT: i32 = 14;
    pub const STRING: i32 = 15;
    pub const NUMBER: i32 = 16;
    pub const BOOLEAN: i32 = 17;
    pub const ARRAY: i32 = 18;
    pub const OBJECT: i32 = 19;
    pub const KEY: i32 = 20;
    pub const NULL: i32 = 21;
    pub const ENUM_MEMBER: i32 = 22;
    pub const STRUCT: i32 = 23;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentFormattingParams {
    #[serde(rename = "textDocument")]
    pub text_document: TextDocumentIdentifier,
    pub options: FormattingOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattingOptions {
    #[serde(rename = "tabSize")]
    pub tab_size: u32,
    #[serde(rename = "insertSpaces")]
    pub insert_spaces: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEdit {
    pub range: Range,
    #[serde(rename = "newText")]
    pub new_text: String,
}
