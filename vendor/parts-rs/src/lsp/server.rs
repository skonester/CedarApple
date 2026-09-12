use rustc_hash::FxHashMap;
use std::io::{self, BufRead, Read, Write};

use crate::lsp::{
    analyzer::Document,
    protocol::{
        CompletionOptions, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
        DidOpenTextDocumentParams, DocumentFormattingParams, Hover, InitializeParams,
        InitializeResult, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse,
        PublishDiagnosticsParams, ServerCapabilities, ServerInfo, TextDocumentPositionParams,
    },
};

pub struct LspServer {
    pub documents: FxHashMap<String, Document>,
    pub is_shutdown: bool,
}

impl Default for LspServer {
    fn default() -> Self {
        Self::new()
    }
}

impl LspServer {
    pub fn new() -> Self {
        Self {
            documents: FxHashMap::default(),
            is_shutdown: false,
        }
    }

    /// Process a raw JSON-RPC payload string and return responses/notifications as strings.
    pub fn handle_message(&mut self, payload: &str) -> Vec<String> {
        let req: JsonRpcRequest = match serde_json::from_str(payload) {
            Ok(r) => r,
            Err(e) => {
                let err_resp = JsonRpcResponse::error(
                    serde_json::Value::Null,
                    -32700,
                    format!("Parse error: {}", e),
                );
                return vec![serde_json::to_string(&err_resp).unwrap_or_default()];
            }
        };

        let mut outputs = Vec::new();

        match req.method.as_str() {
            "initialize" => {
                let _params: Option<InitializeParams> =
                    req.params.and_then(|p| serde_json::from_value(p).ok());

                let result = InitializeResult {
                    capabilities: ServerCapabilities {
                        text_document_sync: Some(1), // Full sync (1)
                        hover_provider: Some(true),
                        completion_provider: Some(CompletionOptions {
                            trigger_characters: Some(vec![
                                ".".to_string(),
                                ":".to_string(),
                                "@".to_string(),
                                "!".to_string(),
                            ]),
                        }),
                        definition_provider: Some(true),
                        document_symbol_provider: Some(true),
                        document_formatting_provider: Some(true),
                    },
                    server_info: ServerInfo {
                        name: "parts-lsp".to_string(),
                        version: Some(env!("CARGO_PKG_VERSION").to_string()),
                    },
                };

                if let Some(id) = req.id {
                    let resp = JsonRpcResponse::success(id, serde_json::to_value(result).unwrap());
                    outputs.push(serde_json::to_string(&resp).unwrap());
                }
            }
            "initialized" => {
                // Client confirmed initialization; nothing to respond
            }
            "textDocument/didOpen" => {
                if let Some(params_val) = req.params
                    && let Ok(params) =
                        serde_json::from_value::<DidOpenTextDocumentParams>(params_val)
                {
                    let doc = Document::new(
                        params.text_document.uri.clone(),
                        params.text_document.version,
                        params.text_document.text,
                    );
                    let diags = doc.diagnostics();
                    self.documents.insert(params.text_document.uri.clone(), doc);

                    let notif = JsonRpcNotification::new(
                        "textDocument/publishDiagnostics",
                        serde_json::to_value(PublishDiagnosticsParams {
                            uri: params.text_document.uri,
                            version: Some(params.text_document.version),
                            diagnostics: diags,
                        })
                        .unwrap(),
                    );
                    outputs.push(serde_json::to_string(&notif).unwrap());
                }
            }
            "textDocument/didChange" => {
                if let Some(params_val) = req.params
                    && let Ok(params) =
                        serde_json::from_value::<DidChangeTextDocumentParams>(params_val)
                    && let Some(change) = params.content_changes.into_iter().last()
                    && let Some(doc) = self.documents.get_mut(&params.text_document.uri)
                {
                    doc.update(params.text_document.version, change.text);
                    let diags = doc.diagnostics();

                    let notif = JsonRpcNotification::new(
                        "textDocument/publishDiagnostics",
                        serde_json::to_value(PublishDiagnosticsParams {
                            uri: params.text_document.uri,
                            version: Some(params.text_document.version),
                            diagnostics: diags,
                        })
                        .unwrap(),
                    );
                    outputs.push(serde_json::to_string(&notif).unwrap());
                }
            }
            "textDocument/didClose" => {
                if let Some(params_val) = req.params
                    && let Ok(params) =
                        serde_json::from_value::<DidCloseTextDocumentParams>(params_val)
                {
                    self.documents.remove(&params.text_document.uri);
                    // Clear diagnostics on close
                    let notif = JsonRpcNotification::new(
                        "textDocument/publishDiagnostics",
                        serde_json::to_value(PublishDiagnosticsParams {
                            uri: params.text_document.uri,
                            version: None,
                            diagnostics: vec![],
                        })
                        .unwrap(),
                    );
                    outputs.push(serde_json::to_string(&notif).unwrap());
                }
            }
            "textDocument/hover" => {
                if let Some(id) = req.id {
                    let hover_res: Option<Hover> = req.params.and_then(|p| {
                        let params: TextDocumentPositionParams = serde_json::from_value(p).ok()?;
                        let doc = self.documents.get(&params.text_document.uri)?;
                        doc.hover(params.position)
                    });

                    let resp = JsonRpcResponse::success(
                        id,
                        serde_json::to_value(hover_res).unwrap_or(serde_json::Value::Null),
                    );
                    outputs.push(serde_json::to_string(&resp).unwrap());
                }
            }
            "textDocument/completion" => {
                if let Some(id) = req.id {
                    let items = req
                        .params
                        .and_then(|p| {
                            let params: TextDocumentPositionParams =
                                serde_json::from_value(p).ok()?;
                            let doc = self.documents.get(&params.text_document.uri)?;
                            Some(doc.completion(params.position))
                        })
                        .unwrap_or_default();

                    let resp = JsonRpcResponse::success(
                        id,
                        serde_json::to_value(items).unwrap_or(serde_json::Value::Null),
                    );
                    outputs.push(serde_json::to_string(&resp).unwrap());
                }
            }
            "textDocument/documentSymbol" => {
                if let Some(id) = req.id {
                    let symbols = req
                        .params
                        .and_then(|p| {
                            let params: crate::lsp::protocol::DidCloseTextDocumentParams =
                                serde_json::from_value(p).ok()?;
                            let doc = self.documents.get(&params.text_document.uri)?;
                            Some(doc.document_symbols())
                        })
                        .unwrap_or_default();

                    let resp = JsonRpcResponse::success(
                        id,
                        serde_json::to_value(symbols).unwrap_or(serde_json::Value::Null),
                    );
                    outputs.push(serde_json::to_string(&resp).unwrap());
                }
            }
            "textDocument/definition" => {
                if let Some(id) = req.id {
                    let loc = req.params.and_then(|p| {
                        let params: TextDocumentPositionParams = serde_json::from_value(p).ok()?;
                        let doc = self.documents.get(&params.text_document.uri)?;
                        doc.definition(params.position)
                    });

                    let resp = JsonRpcResponse::success(
                        id,
                        serde_json::to_value(loc).unwrap_or(serde_json::Value::Null),
                    );
                    outputs.push(serde_json::to_string(&resp).unwrap());
                }
            }
            "textDocument/formatting" => {
                if let Some(id) = req.id {
                    let edits = req
                        .params
                        .and_then(|p| {
                            let params: DocumentFormattingParams =
                                serde_json::from_value(p).ok()?;
                            let doc = self.documents.get(&params.text_document.uri)?;
                            Some(doc.format())
                        })
                        .unwrap_or_default();

                    let resp = JsonRpcResponse::success(
                        id,
                        serde_json::to_value(edits).unwrap_or(serde_json::Value::Null),
                    );
                    outputs.push(serde_json::to_string(&resp).unwrap());
                }
            }
            "shutdown" => {
                self.is_shutdown = true;
                if let Some(id) = req.id {
                    let resp = JsonRpcResponse::success(id, serde_json::Value::Null);
                    outputs.push(serde_json::to_string(&resp).unwrap());
                }
            }
            "exit" => {
                std::process::exit(if self.is_shutdown { 0 } else { 1 });
            }
            _ => {
                if let Some(id) = req.id {
                    let resp = JsonRpcResponse::error(
                        id,
                        -32601,
                        format!("Method not found: {}", req.method),
                    );
                    outputs.push(serde_json::to_string(&resp).unwrap());
                }
            }
        }

        outputs
    }

    /// Run the LSP server on standard input and output streams until EOF or exit.
    pub fn run_stdio(&mut self) -> io::Result<()> {
        let stdin = io::stdin();
        let mut reader = stdin.lock();
        let stdout = io::stdout();
        let mut writer = stdout.lock();

        loop {
            // 1. Read headers until \r\n\r\n
            let mut content_length: Option<usize> = None;
            let mut line = String::new();

            loop {
                line.clear();
                let bytes_read = reader.read_line(&mut line)?;
                if bytes_read == 0 {
                    return Ok(()); // EOF reached
                }

                let trimmed = line.trim();
                if trimmed.is_empty() {
                    break; // Header section ended
                }

                if let Some(len_str) = trimmed.strip_prefix("Content-Length:")
                    && let Ok(len) = len_str.trim().parse::<usize>()
                {
                    content_length = Some(len);
                }
            }

            let length = match content_length {
                Some(l) => l,
                None => continue,
            };

            // 2. Read exact payload of `length` bytes
            let mut body_buf = vec![0u8; length];
            reader.read_exact(&mut body_buf)?;

            let body_str = match String::from_utf8(body_buf) {
                Ok(s) => s,
                Err(_) => continue,
            };

            // 3. Dispatch and respond
            let responses = self.handle_message(&body_str);
            for resp in responses {
                let msg_bytes = resp.as_bytes();
                write!(
                    writer,
                    "Content-Length: {}\r\n\r\n{}",
                    msg_bytes.len(),
                    resp
                )?;
                writer.flush()?;
            }
        }
    }
}
