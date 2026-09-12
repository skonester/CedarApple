use parts::lsp::{
    LspServer,
    protocol::{CompletionItem, Diagnostic, DocumentSymbol, Hover, Location, TextEdit},
};

#[test]
fn test_lsp_initialize() {
    let mut server = LspServer::new();
    let init_req = r#"{
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "rootUri": "file:///workspace",
            "capabilities": {}
        }
    }"#;

    let resps = server.handle_message(init_req);
    assert_eq!(resps.len(), 1);
    let val: serde_json::Value = serde_json::from_str(&resps[0]).unwrap();
    assert_eq!(val["id"], 1);
    assert!(
        val["result"]["capabilities"]["hoverProvider"]
            .as_bool()
            .unwrap()
    );
    assert!(val["result"]["capabilities"]["completionProvider"].is_object());
    assert!(
        val["result"]["capabilities"]["definitionProvider"]
            .as_bool()
            .unwrap()
    );
    assert!(
        val["result"]["capabilities"]["documentSymbolProvider"]
            .as_bool()
            .unwrap()
    );
}

#[test]
fn test_lsp_did_open_and_diagnostics() {
    let mut server = LspServer::new();

    // 1. Valid file - no diagnostics
    let open_valid = r#"{
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///test.pts",
                "languageId": "parts",
                "version": 1,
                "text": "let x = 10;\nreturn x + 5;"
            }
        }
    }"#;

    let resps = server.handle_message(open_valid);
    assert_eq!(resps.len(), 1);
    let notif: serde_json::Value = serde_json::from_str(&resps[0]).unwrap();
    assert_eq!(notif["method"], "textDocument/publishDiagnostics");
    let diags: Vec<Diagnostic> =
        serde_json::from_value(notif["params"]["diagnostics"].clone()).unwrap();
    assert!(diags.is_empty());

    // 2. File with error - publishes diagnostics
    let open_invalid = r#"{
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///bad.pts",
                "languageId": "parts",
                "version": 1,
                "text": "let = 10;"
            }
        }
    }"#;

    let resps = server.handle_message(open_invalid);
    assert_eq!(resps.len(), 1);
    let notif: serde_json::Value = serde_json::from_str(&resps[0]).unwrap();
    let diags: Vec<Diagnostic> =
        serde_json::from_value(notif["params"]["diagnostics"].clone()).unwrap();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].range.start.line, 0);
    assert!(diags[0].message.contains("expected"));
}

#[test]
fn test_lsp_did_change_updates_diagnostics() {
    let mut server = LspServer::new();

    let open = r#"{
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///edit.pts",
                "languageId": "parts",
                "version": 1,
                "text": "let = 10;"
            }
        }
    }"#;
    server.handle_message(open);

    // Fix error in didChange
    let change = r#"{
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": {
                "uri": "file:///edit.pts",
                "version": 2
            },
            "contentChanges": [
                {
                    "text": "let x = 10;"
                }
            ]
        }
    }"#;

    let resps = server.handle_message(change);
    assert_eq!(resps.len(), 1);
    let notif: serde_json::Value = serde_json::from_str(&resps[0]).unwrap();
    let diags: Vec<Diagnostic> =
        serde_json::from_value(notif["params"]["diagnostics"].clone()).unwrap();
    assert!(diags.is_empty());
}

#[test]
fn test_lsp_hover() {
    let mut server = LspServer::new();

    let open = r#"{
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///hover.pts",
                "languageId": "parts",
                "version": 1,
                "text": "let add(a, b) = a + b;\nlet total = add(10, 20);\nprintln(total);"
            }
        }
    }"#;
    server.handle_message(open);

    // Hover on 'let' keyword (line 0, col 1)
    let hover_let = r#"{
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/hover",
        "params": {
            "textDocument": { "uri": "file:///hover.pts" },
            "position": { "line": 0, "character": 1 }
        }
    }"#;
    let resps = server.handle_message(hover_let);
    let val: serde_json::Value = serde_json::from_str(&resps[0]).unwrap();
    let hover: Hover = serde_json::from_value(val["result"].clone()).unwrap();
    assert!(hover.contents.value.contains("`let` declaration"));

    // Hover on 'println' (line 2, col 2)
    let hover_println = r#"{
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/hover",
        "params": {
            "textDocument": { "uri": "file:///hover.pts" },
            "position": { "line": 2, "character": 2 }
        }
    }"#;
    let resps = server.handle_message(hover_println);
    let val: serde_json::Value = serde_json::from_str(&resps[0]).unwrap();
    let hover: Hover = serde_json::from_value(val["result"].clone()).unwrap();
    assert!(hover.contents.value.contains("println"));

    // Hover on user-defined function 'add' (line 1, col 13)
    let hover_add = r#"{
        "jsonrpc": "2.0",
        "id": 4,
        "method": "textDocument/hover",
        "params": {
            "textDocument": { "uri": "file:///hover.pts" },
            "position": { "line": 1, "character": 13 }
        }
    }"#;
    let resps = server.handle_message(hover_add);
    let val: serde_json::Value = serde_json::from_str(&resps[0]).unwrap();
    let hover: Hover = serde_json::from_value(val["result"].clone()).unwrap();
    assert!(hover.contents.value.contains("let add(a, b)"));
}

#[test]
fn test_lsp_completion() {
    let mut server = LspServer::new();

    let open = r#"{
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///complete.pts",
                "languageId": "parts",
                "version": 1,
                "text": "let my_variable = 100;"
            }
        }
    }"#;
    server.handle_message(open);

    let comp_req = r#"{
        "jsonrpc": "2.0",
        "id": 5,
        "method": "textDocument/completion",
        "params": {
            "textDocument": { "uri": "file:///complete.pts" },
            "position": { "line": 0, "character": 5 }
        }
    }"#;

    let resps = server.handle_message(comp_req);
    let val: serde_json::Value = serde_json::from_str(&resps[0]).unwrap();
    let items: Vec<CompletionItem> = serde_json::from_value(val["result"].clone()).unwrap();
    assert!(!items.is_empty());

    let labels: Vec<String> = items.into_iter().map(|i| i.label).collect();
    assert!(labels.contains(&"let".to_string()));
    assert!(labels.contains(&"println".to_string()));
    assert!(labels.contains(&"Result::Ok".to_string()));
    assert!(labels.contains(&"@std/std.pts".to_string()));
    assert!(labels.contains(&"my_variable".to_string()));
}

#[test]
fn test_lsp_document_symbols() {
    let mut server = LspServer::new();

    let open = r#"{
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///symbols.pts",
                "languageId": "parts",
                "version": 1,
                "text": "let add(a, b) = a + b;\nenum Status {\n    Active,\n    Inactive(reason)\n}\nlet count = 42;"
            }
        }
    }"#;
    server.handle_message(open);

    let sym_req = r#"{
        "jsonrpc": "2.0",
        "id": 6,
        "method": "textDocument/documentSymbol",
        "params": {
            "textDocument": { "uri": "file:///symbols.pts" }
        }
    }"#;

    let resps = server.handle_message(sym_req);
    let val: serde_json::Value = serde_json::from_str(&resps[0]).unwrap();
    let symbols: Vec<DocumentSymbol> = serde_json::from_value(val["result"].clone()).unwrap();

    let names: Vec<String> = symbols.iter().map(|s| s.name.clone()).collect();
    assert!(names.contains(&"add".to_string()));
    assert!(names.contains(&"Status".to_string()));
    assert!(names.contains(&"count".to_string()));

    let status_sym = symbols.iter().find(|s| s.name == "Status").unwrap();
    let variant_names: Vec<String> = status_sym
        .children
        .as_ref()
        .unwrap()
        .iter()
        .map(|s| s.name.clone())
        .collect();
    assert!(variant_names.contains(&"Active".to_string()));
    assert!(variant_names.contains(&"Inactive".to_string()));
}

#[test]
fn test_lsp_definition() {
    let mut server = LspServer::new();

    let open = r#"{
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///def.pts",
                "languageId": "parts",
                "version": 1,
                "text": "let calculate_tax(amt) = amt * 2;\nlet val = calculate_tax(50);"
            }
        }
    }"#;
    server.handle_message(open);

    // Goto definition for calculate_tax on line 1, col 12
    let def_req = r#"{
        "jsonrpc": "2.0",
        "id": 7,
        "method": "textDocument/definition",
        "params": {
            "textDocument": { "uri": "file:///def.pts" },
            "position": { "line": 1, "character": 12 }
        }
    }"#;

    let resps = server.handle_message(def_req);
    let val: serde_json::Value = serde_json::from_str(&resps[0]).unwrap();
    let loc: Location = serde_json::from_value(val["result"].clone()).unwrap();
    assert_eq!(loc.uri, "file:///def.pts");
    assert_eq!(loc.range.start.line, 0);
}

#[test]
fn test_lsp_formatting() {
    let mut server = LspServer::new();

    let open = r#"{
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///fmt.pts",
                "languageId": "parts",
                "version": 1,
                "text": "let x = 10;   \nlet y = 20;  "
            }
        }
    }"#;
    server.handle_message(open);

    let fmt_req = r#"{
        "jsonrpc": "2.0",
        "id": 8,
        "method": "textDocument/formatting",
        "params": {
            "textDocument": { "uri": "file:///fmt.pts" },
            "options": {
                "tabSize": 4,
                "insertSpaces": true
            }
        }
    }"#;

    let resps = server.handle_message(fmt_req);
    let val: serde_json::Value = serde_json::from_str(&resps[0]).unwrap();
    let edits: Vec<TextEdit> = serde_json::from_value(val["result"].clone()).unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "let x = 10;\nlet y = 20;\n");
}

#[test]
fn test_lsp_close_clears_diagnostics() {
    let mut server = LspServer::new();

    let open = r#"{
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": "file:///close.pts",
                "languageId": "parts",
                "version": 1,
                "text": "let = 10;"
            }
        }
    }"#;
    server.handle_message(open);

    let close = r#"{
        "jsonrpc": "2.0",
        "method": "textDocument/didClose",
        "params": {
            "textDocument": { "uri": "file:///close.pts" }
        }
    }"#;

    let resps = server.handle_message(close);
    assert_eq!(resps.len(), 1);
    let notif: serde_json::Value = serde_json::from_str(&resps[0]).unwrap();
    let diags: Vec<Diagnostic> =
        serde_json::from_value(notif["params"]["diagnostics"].clone()).unwrap();
    assert!(diags.is_empty());
}

#[test]
fn test_lsp_shutdown() {
    let mut server = LspServer::new();

    let req = r#"{
        "jsonrpc": "2.0",
        "id": 9,
        "method": "shutdown"
    }"#;

    let resps = server.handle_message(req);
    assert_eq!(resps.len(), 1);
    let val: serde_json::Value = serde_json::from_str(&resps[0]).unwrap();
    assert_eq!(val["id"], 9);
    assert!(server.is_shutdown);
}
