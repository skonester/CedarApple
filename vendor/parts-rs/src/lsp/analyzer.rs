use std::path::{Path, PathBuf};

use crate::{
    diagnostic::DiagnosticLevel,
    lsp::protocol::{
        CompletionItem, CompletionItemKind, Diagnostic, DiagnosticSeverity, DocumentSymbol, Hover,
        InsertTextFormat, Location, MarkupContent, Position, Range, SymbolKind, TextEdit,
    },
    parser::{Ast, Parser, Value as ParserValue},
    scanner::{Scanner, TokenType},
    scanner_rules::ScannerRule,
    tools::LanguageTools,
};

#[derive(Debug, Clone)]
pub struct Document {
    pub uri: String,
    pub version: i32,
    pub text: String,
    pub lines: Vec<String>,
}

impl Document {
    pub fn new(uri: String, version: i32, text: String) -> Self {
        let lines = text.lines().map(|s| s.to_string()).collect();
        Self {
            uri,
            version,
            text,
            lines,
        }
    }

    pub fn update(&mut self, version: i32, text: String) {
        self.version = version;
        self.lines = text.lines().map(|s| s.to_string()).collect();
        self.text = text;
    }

    pub fn get_import_path(&self) -> PathBuf {
        if self.uri.starts_with("file://") {
            let path_str = &self.uri[7..];
            if let Some(parent) = Path::new(path_str).parent() {
                return parent.to_path_buf();
            }
        }
        std::env::current_dir().unwrap_or_default()
    }

    pub fn word_at(&self, pos: Position) -> Option<String> {
        let line = self.lines.get(pos.line as usize)?;
        let col = pos.character as usize;
        if col > line.len() {
            return None;
        }

        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            return None;
        }

        let target_col = if col >= chars.len() {
            chars.len().saturating_sub(1)
        } else {
            col
        };

        if !is_identifier_char(chars[target_col]) && chars[target_col] != '@' {
            if target_col > 0 && is_identifier_char(chars[target_col - 1]) {
                // Look right behind caret
                let mut start = target_col - 1;
                while start > 0 && is_identifier_char(chars[start - 1]) {
                    start -= 1;
                }
                return Some(chars[start..target_col].iter().collect());
            }
            return None;
        }

        let mut start = target_col;
        while start > 0 && (is_identifier_char(chars[start - 1]) || chars[start - 1] == '@') {
            start -= 1;
        }

        let mut end = target_col;
        while end < chars.len() && is_identifier_char(chars[end]) {
            end += 1;
        }

        if start < end {
            Some(chars[start..end].iter().collect())
        } else {
            None
        }
    }

    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        let import_path = self.get_import_path();
        let report =
            LanguageTools::check_with_import_path(&self.text, Some(&self.uri), import_path);

        let mut diagnostics = Vec::new();
        for diag in report.diagnostics {
            let (start_line, start_char, end_line, end_char) = match (diag.line, diag.column) {
                (Some(l), Some(c)) => {
                    let l_idx = l.saturating_sub(1) as u32;
                    let c_idx = c.saturating_sub(1) as u32;
                    let len = (diag.length as u32).max(1);
                    (l_idx, c_idx, l_idx, c_idx + len)
                }
                _ => (0, 0, 0, 1),
            };

            let severity = match diag.level {
                DiagnosticLevel::Error => Some(DiagnosticSeverity::ERROR),
                DiagnosticLevel::Warning => Some(DiagnosticSeverity::WARNING),
                DiagnosticLevel::Note | DiagnosticLevel::Info => {
                    Some(DiagnosticSeverity::INFORMATION)
                }
                DiagnosticLevel::Help => Some(DiagnosticSeverity::HINT),
            };

            let mut msg = diag.message;
            if let Some(ref label) = diag.label
                && !msg.contains(label)
            {
                msg = format!("{}: {}", msg, label);
            }
            if let Some(ref help) = diag.help {
                msg = format!("{}\nhelp: {}", msg, help);
            }
            for note in &diag.notes {
                msg = format!("{}\nnote: {}", msg, note);
            }

            diagnostics.push(Diagnostic {
                range: Range::new(start_line, start_char, end_line, end_char),
                severity,
                code: None,
                source: Some("parts".to_string()),
                message: msg,
            });
        }

        diagnostics
    }

    pub fn hover(&self, pos: Position) -> Option<Hover> {
        let word = self.word_at(pos)?;

        // 1. Keywords documentation
        let doc = match word.as_str() {
            "let" => Some(
                "### `let` declaration\nDeclares a mutable variable or function.\n\n```parts\nlet x = 42;\nlet add(a, b) = a + b;\n```",
            ),
            "fun" => Some(
                "### `fun` anonymous function\nDefines a lambda / closure expression.\n\n```parts\nlet square = fun(x) = x * x;\n```",
            ),
            "enum" => Some(
                "### `enum` definition\nDefines an algebraic data type with tagged variants.\n\n```parts\nenum Direction {\n    North(power),\n    South(power),\n    Stationary\n}\n```",
            ),
            "match" => Some(
                "### `match` expression\nPattern matches on enum variants, values, and wildcards.\n\n```parts\nmatch result {\n    Ok(v) => v,\n    Err(e) => 0,\n    _ => 0\n}\n```",
            ),
            "if" => Some(
                "### `if` conditional\nEvaluates a branch based on boolean truthiness.\n\n```parts\nif score >= 90 {\n    println(\"A\");\n} else {\n    println(\"B\");\n}\n```",
            ),
            "else" => Some("### `else` branch\nFallback branch for `if` statement."),
            "for" => Some(
                "### `for` loop\nActs as a while-loop or collection iterator.\n\n```parts\nfor i < 10 { i = i + 1; }\nfor item in collection { ... }\n```",
            ),
            "in" => Some("### `in` iterator clause\nIterates over strings, objects, or arrays."),
            "return" => {
                Some("### `return` statement\nReturns a value from the current function frame.")
            }
            "break" => Some("### `break` statement\nTerminates the nearest enclosing `for` loop."),
            "continue" => {
                Some("### `continue` statement\nSkips to the next iteration of the nearest loop.")
            }
            "part" => Some(
                "### `part` macro declaration\nDefines a hygienic syntactic macro.\n\n```parts\npart log {\n    (@msg) => { println(@msg); }\n}\n```",
            ),
            "import" => Some(
                "### `import` statement\nImports standard library modules or local files.\n\n```parts\nlet std = import `@std/std.pts`;\n```",
            ),
            "true" | "false" => Some("### Boolean literal"),
            "Int" => Some("### `Int` primitive type\n64-bit signed integer (`i64`)."),
            "Double" => {
                Some("### `Double` primitive type\n64-bit IEEE 754 floating point number (`f64`).")
            }
            "Bool" => Some("### `Bool` primitive type\nBoolean truth value (`true` or `false`)."),
            "String" => Some("### `String` primitive type\nReference-counted UTF-8 string."),
            "Bytes" => Some("### `Bytes` buffer type\nMutable byte buffer (`Vec<u8>`)."),
            "Result" => Some(
                "### `Result` enum\nRepresents success (`Result::Ok(val)`) or failure (`Result::Err(err)`).",
            ),
            "Option" => Some(
                "### `Option` enum\nRepresents an optional value (`Option::Some(val)`) or absence (`Option::None`).",
            ),
            "Ok" => Some("### `Result::Ok` variant\nWraps a successful value: `Result::Ok(val)`"),
            "Err" => Some("### `Result::Err` variant\nWraps an error value: `Result::Err(err)`"),
            "Some" => {
                Some("### `Option::Some` variant\nWraps an existing value: `Option::Some(val)`")
            }
            "None" => Some("### `Option::None` variant\nRepresents no value: `Option::None`"),
            "println" => Some(
                "### `println(...)`\nPrints arguments to standard output followed by a newline.",
            ),
            "print" => Some(
                "### `print(...)`\nPrints arguments to standard output without trailing newline.",
            ),
            "input" => Some("### `input()`\nReads a line from standard input as a `String`."),
            "exec" => Some(
                "### `std.sys.exec(cmd)`\nExecutes a system shell command and returns the output.",
            ),
            "env" => Some("### `std.sys.env(key)`\nReads an environment variable."),
            "len" => Some("### `.len()`\nReturns the length of a string, object, or byte buffer."),
            "unwrap" => Some(
                "### `.unwrap()`\nUnwraps `Result::Ok` or `Option::Some`, panicking on failure.",
            ),
            "unwrap_or" => Some(
                "### `.unwrap_or(default)`\nUnwraps `Result` / `Option` or returns the fallback default value.",
            ),
            "is_ok" => Some("### `.is_ok()`\nReturns `true` if the Result is `Ok`."),
            "is_err" => Some("### `.is_err()`\nReturns `true` if the Result is `Err`."),
            "is_some" => Some("### `.is_some()`\nReturns `true` if the Option is `Some`."),
            "is_none" => Some("### `.is_none()`\nReturns `true` if the Option is `None`."),
            _ => None,
        };

        if let Some(doc_text) = doc {
            return Some(Hover {
                contents: MarkupContent::markdown(doc_text),
                range: None,
            });
        }

        // 2. Check AST for user-defined symbols
        let mut parser = Parser::new(self.text.clone());
        if let Ok(ast) = parser.parse_all() {
            for node in &ast {
                match node {
                    Ast::Declare { name, value } => {
                        if name == &word {
                            let detail = match value.as_ref() {
                                Ast::Value(ParserValue::Fun { args, .. }) => {
                                    format!("let {}({})", name, args.join(", "))
                                }
                                _ => format!("let {}", name),
                            };
                            return Some(Hover {
                                contents: MarkupContent::markdown(format!(
                                    "```parts\n{}\n```\nUser-defined declaration.",
                                    detail
                                )),
                                range: None,
                            });
                        }
                    }
                    Ast::EnumDef { name, variants } => {
                        if name == &word {
                            let variant_strs: Vec<String> = variants
                                .iter()
                                .map(|v| {
                                    if v.fields.is_empty() {
                                        v.name.clone()
                                    } else {
                                        format!("{}({})", v.name, v.fields.join(", "))
                                    }
                                })
                                .collect();
                            return Some(Hover {
                                contents: MarkupContent::markdown(format!(
                                    "```parts\nenum {} {{\n    {}\n}}\n```",
                                    name,
                                    variant_strs.join(",\n    ")
                                )),
                                range: None,
                            });
                        }
                        for variant in variants {
                            if variant.name == word {
                                let sig = if variant.fields.is_empty() {
                                    format!("{}::{}", name, variant.name)
                                } else {
                                    format!(
                                        "{}::{}({})",
                                        name,
                                        variant.name,
                                        variant.fields.join(", ")
                                    )
                                };
                                return Some(Hover {
                                    contents: MarkupContent::markdown(format!(
                                        "```parts\n{}\n```\nVariant of enum `{}`.",
                                        sig, name
                                    )),
                                    range: None,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        None
    }

    pub fn completion(&self, _pos: Position) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        // 1. Keywords
        let keywords = [
            ("let", "let declaration", "let ${1:name} = ${2:value};"),
            ("fun", "anonymous function", "fun(${1:args}) = ${2:body}"),
            ("if", "if statement", "if ${1:condition} {\n    $0\n}"),
            (
                "if else",
                "if-else statement",
                "if ${1:condition} {\n    $2\n} else {\n    $0\n}",
            ),
            ("for", "for loop", "for ${1:condition} {\n    $0\n}"),
            (
                "for in",
                "for iterator",
                "for ${1:item} in ${2:collection} {\n    $0\n}",
            ),
            (
                "match",
                "match expression",
                "match ${1:target} {\n    ${2:pattern} => ${3:result},\n    _ => $0\n}",
            ),
            (
                "enum",
                "enum definition",
                "enum ${1:Name} {\n    ${2:Variant}\n}",
            ),
            ("return", "return statement", "return ${1:value};"),
            ("break", "break statement", "break;"),
            ("continue", "continue statement", "continue;"),
            (
                "import",
                "import statement",
                "let ${1:std} = import `@std/${2:std}.pts`;",
            ),
            (
                "part",
                "macro definition",
                "part ${1:name} {\n    (${2:@arg}) => {\n        $0\n    }\n}",
            ),
        ];

        for (kw, detail, snippet) in keywords {
            items.push(CompletionItem {
                label: kw.to_string(),
                kind: Some(CompletionItemKind::KEYWORD),
                detail: Some(detail.to_string()),
                documentation: None,
                insert_text: Some(snippet.to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
            });
        }

        // 2. Builtin types and variants
        let builtins = [
            (
                "Result::Ok",
                CompletionItemKind::ENUM_MEMBER,
                "Result::Ok(${1:val})",
                "Ok variant",
            ),
            (
                "Result::Err",
                CompletionItemKind::ENUM_MEMBER,
                "Result::Err(${1:err})",
                "Err variant",
            ),
            (
                "Option::Some",
                CompletionItemKind::ENUM_MEMBER,
                "Option::Some(${1:val})",
                "Some variant",
            ),
            (
                "Option::None",
                CompletionItemKind::ENUM_MEMBER,
                "Option::None",
                "None variant",
            ),
            (
                "println",
                CompletionItemKind::FUNCTION,
                "println(${1:args});",
                "Print with newline",
            ),
            (
                "print",
                CompletionItemKind::FUNCTION,
                "print(${1:args});",
                "Print without newline",
            ),
            (
                "input",
                CompletionItemKind::FUNCTION,
                "input()",
                "Read from stdin",
            ),
            (
                "is_ok",
                CompletionItemKind::METHOD,
                "is_ok()",
                "Check Result::Ok",
            ),
            (
                "is_err",
                CompletionItemKind::METHOD,
                "is_err()",
                "Check Result::Err",
            ),
            (
                "is_some",
                CompletionItemKind::METHOD,
                "is_some()",
                "Check Option::Some",
            ),
            (
                "is_none",
                CompletionItemKind::METHOD,
                "is_none()",
                "Check Option::None",
            ),
            (
                "unwrap",
                CompletionItemKind::METHOD,
                "unwrap()",
                "Unwrap value or panic",
            ),
            (
                "unwrap_or",
                CompletionItemKind::METHOD,
                "unwrap_or(${1:default})",
                "Unwrap with fallback",
            ),
            (
                "unwrap_err",
                CompletionItemKind::METHOD,
                "unwrap_err()",
                "Unwrap error message",
            ),
            (
                "expect",
                CompletionItemKind::METHOD,
                "expect(${1:\"msg\"})",
                "Unwrap with message",
            ),
            (
                "len",
                CompletionItemKind::METHOD,
                "len()",
                "Length of collection",
            ),
            (
                "join",
                CompletionItemKind::METHOD,
                "join(${1:sep})",
                "Join string",
            ),
            (
                "to_hex",
                CompletionItemKind::METHOD,
                "to_hex()",
                "Hex representation of bytes",
            ),
            (
                "to_uppercase",
                CompletionItemKind::METHOD,
                "to_uppercase()",
                "Uppercase string",
            ),
            (
                "to_lowercase",
                CompletionItemKind::METHOD,
                "to_lowercase()",
                "Lowercase string",
            ),
            (
                "push",
                CompletionItemKind::METHOD,
                "push(${1:byte})",
                "Append byte to buffer",
            ),
        ];

        for (label, kind, snippet, detail) in builtins {
            items.push(CompletionItem {
                label: label.to_string(),
                kind: Some(kind),
                detail: Some(detail.to_string()),
                documentation: None,
                insert_text: Some(snippet.to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
            });
        }

        // 3. Standard Library Module imports
        let std_modules = [
            ("@std/std.pts", "Core standard library"),
            ("@std/net.pts", "TCP & Unix Domain Socket networking"),
            ("@std/ws.pts", "RFC 6455 WebSocket client"),
            ("@std/bytes.pts", "Byte buffer manipulation"),
            ("@std/math.pts", "Math functions (sin, cos, floor, pow)"),
            ("@std/str.pts", "String utility functions"),
            ("@std/sys.pts", "System & process utilities (exec, env)"),
            ("@std/result.pts", "Result utility types"),
            ("@std/option.pts", "Option utility types"),
        ];

        for (mod_path, doc) in std_modules {
            items.push(CompletionItem {
                label: mod_path.to_string(),
                kind: Some(CompletionItemKind::MODULE),
                detail: Some("Standard Library Module".to_string()),
                documentation: Some(MarkupContent::markdown(doc)),
                insert_text: Some(mod_path.to_string()),
                insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
            });
        }

        // 4. Local document symbols
        let mut scanner = Scanner::new(ScannerRule::get_default_rules(), self.text.clone());
        let mut seen = std::collections::HashSet::new();
        while let Ok(tok) = scanner.get_next() {
            if tok.kind == TokenType::Special && tok.lexeme == "EOF" {
                break;
            }
            if tok.kind == TokenType::Identifier && !seen.contains(&tok.lexeme) {
                seen.insert(tok.lexeme.clone());
                items.push(CompletionItem {
                    label: tok.lexeme.clone(),
                    kind: Some(CompletionItemKind::VARIABLE),
                    detail: Some("Document symbol".to_string()),
                    documentation: None,
                    insert_text: Some(tok.lexeme),
                    insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                });
            }
        }

        items
    }

    pub fn document_symbols(&self) -> Vec<DocumentSymbol> {
        let mut symbols = Vec::new();
        let mut parser = Parser::new(self.text.clone());

        if let Ok(ast) = parser.parse_all() {
            for (idx, node) in ast.into_iter().enumerate() {
                let default_range = Range::new(idx as u32, 0, idx as u32, 1);
                match node {
                    Ast::Declare { name, value } => {
                        let kind = match value.as_ref() {
                            Ast::Value(ParserValue::Fun { .. }) => SymbolKind::FUNCTION,
                            _ => SymbolKind::VARIABLE,
                        };
                        symbols.push(DocumentSymbol {
                            name,
                            detail: None,
                            kind,
                            range: default_range,
                            selection_range: default_range,
                            children: None,
                        });
                    }
                    Ast::EnumDef { name, variants } => {
                        let variant_symbols: Vec<DocumentSymbol> = variants
                            .into_iter()
                            .map(|v| DocumentSymbol {
                                name: v.name,
                                detail: None,
                                kind: SymbolKind::ENUM_MEMBER,
                                range: default_range,
                                selection_range: default_range,
                                children: None,
                            })
                            .collect();

                        symbols.push(DocumentSymbol {
                            name,
                            detail: None,
                            kind: SymbolKind::ENUM,
                            range: default_range,
                            selection_range: default_range,
                            children: Some(variant_symbols),
                        });
                    }
                    _ => {}
                }
            }
        }

        symbols
    }

    pub fn definition(&self, pos: Position) -> Option<Location> {
        let word = self.word_at(pos)?;

        // Search lines for declaration
        for (line_idx, line) in self.lines.iter().enumerate() {
            if let Some(col_idx) = line.find(&format!("let {}", word)) {
                let start_char = col_idx as u32 + 4;
                let end_char = start_char + word.len() as u32;
                return Some(Location {
                    uri: self.uri.clone(),
                    range: Range::new(line_idx as u32, start_char, line_idx as u32, end_char),
                });
            }
            if let Some(col_idx) = line.find(&format!("enum {}", word)) {
                let start_char = col_idx as u32 + 5;
                let end_char = start_char + word.len() as u32;
                return Some(Location {
                    uri: self.uri.clone(),
                    range: Range::new(line_idx as u32, start_char, line_idx as u32, end_char),
                });
            }
            if let Some(col_idx) = line.find(&format!("part {}", word)) {
                let start_char = col_idx as u32 + 5;
                let end_char = start_char + word.len() as u32;
                return Some(Location {
                    uri: self.uri.clone(),
                    range: Range::new(line_idx as u32, start_char, line_idx as u32, end_char),
                });
            }
        }

        None
    }

    pub fn format(&self) -> Vec<TextEdit> {
        // Return cleaned up full document edit if formatted
        let total_lines = self.lines.len() as u32;
        let last_line_len = self.lines.last().map(|l| l.len() as u32).unwrap_or(0);
        let whole_range = Range::new(0, 0, total_lines, last_line_len);

        // Normalize trailing whitespace and ensure newline at EOF
        let mut formatted = String::new();
        for line in &self.lines {
            formatted.push_str(line.trim_end());
            formatted.push('\n');
        }

        vec![TextEdit {
            range: whole_range,
            new_text: formatted,
        }]
    }
}

fn is_identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}
