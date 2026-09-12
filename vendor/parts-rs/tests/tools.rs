use parts::{emitter::Emitter, tools::LanguageTools};

#[test]
fn test_tools_tokenize() {
    let source = "let x = 42 + 5;";
    let tokens = LanguageTools::tokenize(source, Some("test.pts")).unwrap();
    assert!(!tokens.is_empty());
    assert_eq!(tokens[0].kind, parts::scanner::TokenType::Keyword);
    assert_eq!(tokens[0].lexeme, "LET");
}

#[test]
fn test_tools_tokenize_error() {
    let source = "let x = \"unterminated";
    let err = LanguageTools::tokenize(source, Some("test.pts")).unwrap_err();
    assert_eq!(err.level, parts::diagnostic::DiagnosticLevel::Error);
    assert!(err.message.contains("unterminated string"));
}

#[test]
fn test_tools_parse() {
    let source = "let x = 10; return x;";
    let ast = LanguageTools::parse(source, Some("test.pts")).unwrap();
    assert_eq!(ast.len(), 2);
}

#[test]
fn test_tools_parse_error() {
    let source = "let = 10;";
    let err = LanguageTools::parse(source, Some("test.pts")).unwrap_err();
    assert_eq!(err.level, parts::diagnostic::DiagnosticLevel::Error);
    assert!(err.line.is_some());
}

#[test]
fn test_tools_format_ast() {
    let source = "let x = 10; return x;";
    let ast = LanguageTools::parse(source, None).unwrap();
    let formatted = LanguageTools::format_ast(&ast);
    assert!(formatted.contains("let x"));
    assert!(formatted.contains("return"));
}

#[test]
fn test_tools_disassemble() {
    let mut compiler = parts::compiler::Compiler::new("./".into());
    let ast = LanguageTools::parse("return 42;", None).unwrap();
    let ir = compiler.compile_all(ast).unwrap();
    let bytecode = Emitter {}.emit(ir);
    let disasm = LanguageTools::disassemble(&bytecode, &compiler.constant_pool);
    assert!(disasm.contains("--- Disassembly ---"));
    assert!(disasm.contains("LOAD") || disasm.contains("RETURN"));
}
