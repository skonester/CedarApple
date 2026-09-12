use std::path::PathBuf;

use crate::{
    compiler::Compiler,
    diagnostic::{Diagnostic, Report},
    emitter::OpCode,
    parser::{Ast, Parser},
    scanner::{Scanner, Token},
    scanner_rules::ScannerRule,
    value::Value,
};

/// Language tools for analyzing, validating, and inspecting Parts code.
#[allow(clippy::result_large_err)]
pub struct LanguageTools;

#[allow(clippy::result_large_err)]
impl LanguageTools {
    /// Tokenize source code and return all tokens or a Diagnostic error.
    pub fn tokenize(source: &str, file: Option<&str>) -> Result<Vec<Token>, Diagnostic> {
        let mut scanner = Scanner::new(ScannerRule::get_default_rules(), source.to_string());
        let mut tokens = Vec::new();

        loop {
            match scanner.get_next() {
                Ok(token) => {
                    let is_eof =
                        token.kind == crate::scanner::TokenType::Special && token.lexeme == "EOF";
                    tokens.push(token);
                    if is_eof {
                        break;
                    }
                }
                Err(err) => {
                    return Err(err.to_diagnostic(Some(source), file));
                }
            }
        }

        Ok(tokens)
    }

    /// Parse source code into an AST or return a Diagnostic error.
    pub fn parse(source: &str, file: Option<&str>) -> Result<Vec<Ast>, Diagnostic> {
        let mut parser = Parser::new(source.to_string());
        parser
            .parse_all()
            .map_err(|err| err.to_diagnostic(Some(source), file))
    }

    /// Check source code for syntax and compilation errors without executing.
    pub fn check(source: &str, file: Option<&str>) -> Report {
        let import_path = std::env::current_dir().unwrap_or_default();
        Self::check_with_import_path(source, file, import_path)
    }

    /// Check source code with a specific import resolution path.
    pub fn check_with_import_path(
        source: &str,
        file: Option<&str>,
        import_path: PathBuf,
    ) -> Report {
        let mut report = Report::new();

        let mut parser = Parser::new(source.to_string());
        let ast = match parser.parse_all() {
            Ok(ast) => ast,
            Err(err) => {
                report.add(err.to_diagnostic(Some(source), file));
                return report;
            }
        };

        let mut compiler = Compiler::new(import_path);
        if let Err(errors) = compiler.compile_all(ast) {
            for err in errors {
                report.add(err.to_diagnostic(Some(source), file));
            }
        }

        report
    }

    /// Pretty print an AST slice into a human-readable indented format.
    pub fn format_ast(ast: &[Ast]) -> String {
        let mut out = String::new();
        for (i, node) in ast.iter().enumerate() {
            Self::format_ast_node(node, 0, &mut out);
            if i + 1 < ast.len() {
                out.push('\n');
            }
        }
        out
    }

    fn format_ast_node(node: &Ast, indent: usize, out: &mut String) {
        let pad = "  ".repeat(indent);
        match node {
            Ast::Declare { name, value } => {
                out.push_str(&format!("{}(let {} = ", pad, name));
                Self::format_ast_node(value, indent + 1, out);
                out.push(')');
            }
            Ast::Set { name, value } => {
                out.push_str(&format!("{}(set ", pad));
                Self::format_ast_node(name, 0, out);
                out.push_str(" = ");
                Self::format_ast_node(value, indent + 1, out);
                out.push(')');
            }
            Ast::Value(val) => {
                out.push_str(&format!("{}{:?}", pad, val));
            }
            Ast::Binary {
                left,
                right,
                operator,
            } => {
                out.push_str(&format!("{}({:?}\n", pad, operator));
                Self::format_ast_node(left, indent + 1, out);
                out.push('\n');
                Self::format_ast_node(right, indent + 1, out);
                out.push(')');
            }
            Ast::Block { code } => {
                out.push_str(&format!("{}{{\n", pad));
                for stmt in code {
                    Self::format_ast_node(stmt, indent + 1, out);
                    out.push('\n');
                }
                out.push_str(&format!("{}}}", pad));
            }
            Ast::Return { value } => {
                out.push_str(&format!("{}return ", pad));
                Self::format_ast_node(value, indent + 1, out);
            }
            Ast::If {
                condition,
                then_branch,
                else_branch,
            } => {
                out.push_str(&format!("{}if ", pad));
                Self::format_ast_node(condition, indent + 1, out);
                out.push_str(" then\n");
                Self::format_ast_node(then_branch, indent + 1, out);
                if let Some(else_b) = else_branch {
                    out.push_str(&format!("\n{}else\n", pad));
                    Self::format_ast_node(else_b, indent + 1, out);
                }
            }
            Ast::Call { what, args } => {
                out.push_str(&format!("{}call ", pad));
                Self::format_ast_node(what, indent + 1, out);
                out.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    Self::format_ast_node(arg, 0, out);
                }
                out.push(')');
            }
            other => {
                out.push_str(&format!("{}{:?}", pad, other));
            }
        }
    }

    /// Disassemble bytecode into a formatted readable string.
    pub fn disassemble(code: &[u8], constants: &[Value]) -> String {
        let mut out = String::new();
        out.push_str("--- Disassembly ---\n");
        let mut offset = 0;
        while offset < code.len() {
            let (next_offset, line) = Self::disassemble_instruction(code, offset, constants);
            out.push_str(&line);
            out.push('\n');
            offset = next_offset;
        }
        out
    }

    fn disassemble_instruction(
        code: &[u8],
        offset: usize,
        _constants: &[Value],
    ) -> (usize, String) {
        let byte = code[offset];
        let opcode = match OpCode::try_from(byte) {
            Ok(op) => op,
            Err(_) => {
                return (
                    offset + 1,
                    format!("{:04} Unknown OpCode: {:#04X}", offset, byte),
                );
            }
        };

        let line = match opcode {
            OpCode::LoadInt => format!("{:04} LOAD_INT", offset),
            OpCode::LoadIntSmall => format!("{:04} LOAD_INT_S", offset),
            OpCode::LoadDouble => format!("{:04} LOAD_DBL", offset),
            OpCode::LoadBool => format!("{:04} LOAD_BOOL", offset),
            OpCode::LoadConst => format!("{:04} LOAD_CONST", offset),
            OpCode::LoadReg => {
                if offset + 2 < code.len() {
                    let dest = code[offset + 1];
                    let src = code[offset + 2];
                    format!("{:04} LOAD_REG r{} r{}", offset, dest, src)
                } else {
                    format!("{:04} LOAD_REG (truncated)", offset)
                }
            }
            OpCode::LoadFun => format!("{:04} LOAD_FUN", offset),
            OpCode::LoadObject => format!("{:04} LOAD_OBJ", offset),
            OpCode::LoadEnum => format!("{:04} LOAD_ENUM", offset),
            OpCode::Return => {
                if offset + 1 < code.len() {
                    format!("{:04} RETURN r{}", offset, code[offset + 1])
                } else {
                    format!("{:04} RETURN", offset)
                }
            }
            OpCode::Binary => {
                if offset + 4 < code.len() {
                    let op_code = code[offset + 1];
                    let dest = code[offset + 2];
                    let left = code[offset + 3];
                    let right = code[offset + 4];
                    format!(
                        "{:04} BINARY op:{} r{} = r{} op r{}",
                        offset, op_code, dest, left, right
                    )
                } else {
                    format!("{:04} BINARY (truncated)", offset)
                }
            }
            _ => format!("{:04} {:?}", offset, opcode),
        };

        // Advance based on opcode byte length if known
        let len = match opcode {
            OpCode::LoadInt | OpCode::LoadDouble => 10,
            OpCode::LoadIntSmall | OpCode::LoadBool | OpCode::LoadReg | OpCode::LoadGlobal => 3,
            OpCode::LoadConst | OpCode::LoadFun | OpCode::LoadObject | OpCode::LoadNative => 4,
            OpCode::Return | OpCode::Inc | OpCode::Dec => 2,
            OpCode::Binary | OpCode::GetProperty | OpCode::SetProperty => 5,
            OpCode::Call => {
                if offset + 3 < code.len() {
                    4 + code[offset + 3] as usize
                } else {
                    4
                }
            }
            OpCode::TailCall => {
                if offset + 2 < code.len() {
                    3 + code[offset + 2] as usize
                } else {
                    3
                }
            }
            OpCode::Jump => 3,
            OpCode::JumpNot | OpCode::GetPropertyDyn | OpCode::SetPropertyDyn => 4,
            OpCode::MatchEnum => 6,
            _ => 1,
        };
        (offset + len.min(code.len() - offset).max(1), line)
    }
}
