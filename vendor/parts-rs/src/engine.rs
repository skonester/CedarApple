use crate::compiler::Compiler;
use crate::emitter::Emitter;
use crate::optimize::{AstOptimizer, IrOptimizer};
use crate::parser::Parser;
use crate::value::{NativeFunction, Value};
use crate::vm::VM;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub value: Option<Value>,
    pub constants: Vec<Value>,
}

pub struct Engine {
    import_path: PathBuf,
    natives: Vec<NativeFunction>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine {
    pub fn new() -> Self {
        Engine {
            import_path: std::env::current_dir().unwrap_or_default(),
            natives: vec![],
        }
    }

    pub fn with_import_path(path: PathBuf) -> Self {
        Engine {
            import_path: path,
            natives: vec![],
        }
    }

    pub fn register_native(
        &mut self,
        name: &'static str,
        arity: u8,
        call: impl Fn(&[Value]) -> Result<Value, String> + Send + Sync + 'static,
    ) {
        self.natives.push(NativeFunction {
            name,
            arity,
            call: std::sync::Arc::new(call),
        });
    }
    pub fn check(&self, source: &str) -> crate::diagnostic::Report {
        crate::tools::LanguageTools::check_with_import_path(source, None, self.import_path.clone())
    }

    pub fn execute(&self, source: &str) -> Result<Option<Value>, String> {
        self.run(source).map(|res| res.value)
    }

    pub fn run(&self, source: &str) -> Result<ExecutionResult, String> {
        let mut parser = Parser::new(source.to_string());
        let mut ast = parser
            .parse_all()
            .map_err(|e| format!("Parser error: {}", e))?;

        let mut optimizer = AstOptimizer::new();
        optimizer.collect_all(&ast);
        let mut changed = true;
        while changed {
            changed = optimizer.optimize_all(&mut ast);
        }

        let mut compiler = Compiler::with_natives(self.import_path.clone(), self.natives.clone());
        compiler.optimize = true;
        let ir = compiler.compile_all(ast).map_err(|e| {
            format!(
                "Compiler error:\n{}",
                e.iter()
                    .map(|err| {
                        if err.line > 0 {
                            format!("Line {}:{} - {}", err.line, err.column, err.message)
                        } else {
                            format!("- {}", err.message)
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })?;
        let ir = IrOptimizer::optimize(ir);
        let bytecode = Emitter {}.emit(ir);

        let mut vm = VM::with_natives(bytecode, compiler.constant_pool, self.natives.clone());
        let value = vm.run().map_err(|e| format!("VM error: {}", e))?;

        Ok(ExecutionResult {
            value,
            constants: vm.constants,
        })
    }
}
