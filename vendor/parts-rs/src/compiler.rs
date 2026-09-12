use crate::emitter::Emitter;
use crate::emitter::IrOp;
use crate::optimize::IrOptimizer;
use crate::parser::BinaryOperator;
use crate::parser::EnumVariant;
use crate::parser::ImportType;
use crate::parser::Parser;
use crate::parser::Value as ParserValue;
use crate::value::{Function, NativeFunction, StdDefinition, Value};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{LazyLock, Mutex};

use crate::parser::Ast;

static STD_AST_CACHE: LazyLock<Mutex<FxHashMap<String, Vec<Ast>>>> =
    LazyLock::new(|| Mutex::new(FxHashMap::default()));

#[derive(Clone, Debug)]
pub struct Error {
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub level: ErrorLevel,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line > 0 {
            write!(f, "{}:{}: {}", self.line, self.column, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

impl std::error::Error for Error {}

impl Error {
    pub fn to_diagnostic(
        &self,
        source: Option<&str>,
        file: Option<&str>,
    ) -> crate::diagnostic::Diagnostic {
        let level = match self.level {
            ErrorLevel::Error => crate::diagnostic::DiagnosticLevel::Error,
            ErrorLevel::Warning => crate::diagnostic::DiagnosticLevel::Warning,
        };

        let mut diag = crate::diagnostic::Diagnostic::new(level, &self.message);

        if self.line > 0 {
            diag = diag.with_location(self.line, self.column);
        }

        if let Some(src) = source {
            diag = diag.with_source(src);
        }

        if let Some(f) = file {
            diag = diag.with_file(f);
        }

        diag
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorLevel {
    Error,
    Warning,
}

struct LoopContext {
    start: usize,
    end: usize,
}

struct Context {
    ir_buff: Vec<IrOp>,

    scopes: Vec<FxHashMap<String, u8>>,
    top_level_names: FxHashMap<String, u8>,
    next_free_register: u8,

    loop_stack: Vec<LoopContext>,
    scope_bases: Vec<u8>,
    local_count: u8,
}

impl Context {
    fn new() -> Self {
        Self {
            scopes: vec![FxHashMap::default()],
            scope_bases: vec![0],
            top_level_names: FxHashMap::default(),
            next_free_register: 0,
            loop_stack: vec![],
            local_count: 0,

            ir_buff: vec![],
        }
    }

    fn resolve_local(&mut self, name: &str) -> Option<u8> {
        for scope in self.scopes.iter().rev() {
            if let Some(&reg) = scope.get(name) {
                return Some(reg);
            }
        }

        None
    }

    fn begin_scope(&mut self) {
        self.scopes.push(FxHashMap::default());
        self.scope_bases.push(self.local_count);
    }

    fn end_scope(&mut self) {
        self.scopes.pop();
        if let Some(base) = self.scope_bases.pop() {
            self.local_count = base;
            self.next_free_register = base;
        }
    }
    fn add_local(&mut self, name: String) -> u8 {
        let reg = self.local_count;

        if self.scopes.len() == 1 {
            self.top_level_names.insert(name.clone(), reg);
        }

        if let Some(current) = self.scopes.last_mut() {
            current.insert(name, reg);
        }

        self.local_count += 1;
        self.next_free_register = self.local_count;

        reg
    }
}

pub struct Compiler {
    pub errors: Vec<Error>,
    pub had_error: bool,

    pub constant_pool: Vec<Value>,
    constant_map: FxHashMap<Value, usize>,
    pub enums: Vec<EnumDef>,
    contexts: Vec<Context>,
    std: StdDefinition,
    source: PathBuf,
    next_label_id: usize,
    pub optimize: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumDef {
    name: String,
    variants: Vec<EnumVariant>,
    idx: usize,
}

impl Compiler {
    pub fn new(source: PathBuf) -> Self {
        let mut compiler = Self {
            contexts: vec![Context::new()],
            constant_pool: vec![],
            constant_map: FxHashMap::default(),
            errors: vec![],
            had_error: false,
            std: StdDefinition::get_core(),
            next_label_id: 0,
            source,
            enums: vec![],
            optimize: false,
        };
        compiler.enums.push(EnumDef {
            name: "Result".to_string(),
            variants: vec![
                EnumVariant {
                    name: "Ok".to_string(),
                    fields: vec!["val".to_string()],
                },
                EnumVariant {
                    name: "Err".to_string(),
                    fields: vec!["err".to_string()],
                },
            ],
            idx: 0,
        });
        compiler.enums.push(EnumDef {
            name: "Option".to_string(),
            variants: vec![
                EnumVariant {
                    name: "Some".to_string(),
                    fields: vec!["val".to_string()],
                },
                EnumVariant {
                    name: "None".to_string(),
                    fields: vec![],
                },
            ],
            idx: 1,
        });

        compiler.optimize = false;
        compiler
    }

    pub fn with_natives(source: PathBuf, natives: Vec<NativeFunction>) -> Self {
        let mut compiler = Self::new(source);
        compiler.std.functions.extend(natives);
        compiler
    }

    pub fn error(&mut self, line: usize, column: usize, message: impl Into<String>) {
        self.errors.push(Error {
            line,
            column,
            message: message.into(),
            level: ErrorLevel::Error,
        });
        self.had_error = true;
    }

    pub fn warning(&mut self, line: usize, column: usize, message: impl Into<String>) {
        self.errors.push(Error {
            line,
            column,
            message: message.into(),
            level: ErrorLevel::Warning,
        });
    }

    fn current(&mut self) -> &mut Context {
        self.contexts
            .last_mut()
            .expect("Compiler stack empty (no main stack)")
    }

    fn next_free_address(&mut self) -> u8 {
        let address = self.current().next_free_register;
        self.current().next_free_register += 1;
        address
    }

    fn get_or_add_constant(&mut self, val: Value) -> usize {
        if let Some(&idx) = self.constant_map.get(&val) {
            idx
        } else {
            let idx = self.constant_pool.len();
            self.constant_pool.push(val.clone());
            self.constant_map.insert(val, idx);
            idx
        }
    }

    fn new_label(&mut self) -> usize {
        let id = self.next_label_id;
        self.next_label_id += 1;
        id
    }

    fn compile_module(&mut self, source: String) -> Value {
        let is_std = source.starts_with("@std/") || self.source.starts_with("@std");
        let (raw_source, new_source_path, internal_path_opt) = if is_std {
            let virtual_path = if let Some(stripped) = source.strip_prefix("@std/") {
                PathBuf::from(stripped)
            } else {
                self.source.join(&source)
            };

            let mut internal_path = virtual_path.clone();
            if internal_path.starts_with("@std") {
                internal_path = internal_path.strip_prefix("@std").unwrap().to_path_buf();
            }

            let internal_path_str = internal_path.to_str().unwrap().trim_start_matches('/');

            let file = crate::Assets::get(internal_path_str).unwrap_or_else(|| {
                panic!(
                    "Standard library module not found: {} (original: {})",
                    internal_path_str, source
                )
            });
            let content =
                String::from_utf8(file.data.to_vec()).expect("Invalid UTF-8 in embedded asset");

            (
                content,
                PathBuf::from("@std")
                    .join(internal_path_str)
                    .parent()
                    .unwrap()
                    .to_path_buf(),
                Some(internal_path_str.to_string()),
            )
        } else {
            let full_path = self.source.join(&source);
            let canonical_path = fs::canonicalize(&full_path).unwrap_or(full_path);
            let path_str = canonical_path.to_str().unwrap().to_string();
            let content = match fs::read_to_string(&canonical_path) {
                Ok(c) => c,
                Err(e) => {
                    self.error(0, 0, format!("failed to read import '{}': {}", path_str, e));
                    return Value::Fun(Rc::new(Function {
                        arity: 0,
                        frame_size: 1,
                        code: Rc::from([]),
                    }));
                }
            };
            (
                content,
                canonical_path.parent().unwrap().to_path_buf(),
                None,
            )
        };

        let import_ast = if let Some(ref path_key) = internal_path_opt {
            let mut cache = STD_AST_CACHE.lock().unwrap();
            if let Some(cached) = cache.get(path_key) {
                cached.clone()
            } else {
                let mut p = Parser::new(raw_source);
                let ast = match p.parse_all() {
                    Ok(ast) => ast,
                    Err(e) => {
                        self.error(
                            0,
                            0,
                            format!("syntax error in imported module '{}': {}", source, e),
                        );
                        return Value::Fun(Rc::new(Function {
                            arity: 0,
                            frame_size: 1,
                            code: Rc::from([]),
                        }));
                    }
                };
                cache.insert(path_key.clone(), ast.clone());
                ast
            }
        } else {
            let mut p = Parser::new(raw_source);
            match p.parse_all() {
                Ok(ast) => ast,
                Err(e) => {
                    self.error(
                        0,
                        0,
                        format!("syntax error in imported module '{}': {}", source, e),
                    );
                    return Value::Fun(Rc::new(Function {
                        arity: 0,
                        frame_size: 1,
                        code: Rc::from([]),
                    }));
                }
            }
        };

        let old_source = self.source.clone();
        self.source = new_source_path;

        let mut mod_ctx = Context::new();

        for item in &import_ast {
            if let Ast::Declare { name, .. } = item {
                let reg = mod_ctx.add_local(name.clone());
                mod_ctx.top_level_names.insert(name.clone(), reg);
            }
        }

        self.contexts.push(mod_ctx);

        for item in import_ast {
            self.compile(item);

            self.current().next_free_register = self.current().local_count;
        }

        let obj_reg = self.next_free_address();
        let obj_val = Value::Object(Rc::new(RefCell::new(FxHashMap::default())));
        let obj_idx = self.get_or_add_constant(obj_val);

        self.add_inst(IrOp::LoadObject {
            dest: obj_reg,
            src: obj_idx,
        });

        let top_level = self.current().top_level_names.clone();
        for (name, reg) in top_level {
            let hash = Value::Hash(Value::String(name.into()).get_hash());
            let const_idx = self.get_or_add_constant(hash);

            self.add_inst(IrOp::SetProperty {
                obj: obj_reg,
                key: const_idx,
                val: reg,
            });
        }

        self.add_inst(IrOp::Return { value: obj_reg });

        let ctx = self.contexts.pop().expect("Empty contexts");

        let fun_ir = if self.optimize {
            IrOptimizer::optimize(ctx.ir_buff)
        } else {
            ctx.ir_buff
        };

        let frame_size = IrOp::compute_frame_size(0, &fun_ir);
        let code = Emitter {}.emit(fun_ir);
        let fun = Value::Fun(Rc::new(Function {
            arity: 0,
            frame_size,
            code: code.into(),
        }));
        self.source = old_source;
        fun
    }

    pub fn compile_all(&mut self, ast: Vec<Ast>) -> Result<Vec<IrOp>, Vec<Error>> {
        for item in &ast {
            if let Ast::EnumDef { name, variants } = item {
                if let Some(pos) = self.enums.iter().position(|e| e.name == *name) {
                    let idx = self.enums[pos].idx;
                    self.enums[pos] = EnumDef {
                        name: name.clone(),
                        variants: variants.clone(),
                        idx,
                    };
                } else {
                    let enum_val = Value::EnumDefinition(self.enums.len().try_into().unwrap());
                    let idx = self.get_or_add_constant(enum_val);
                    self.enums.push(EnumDef {
                        name: name.clone(),
                        variants: variants.clone(),
                        idx,
                    });
                }
            }
        }

        for item in ast {
            if item != Ast::Ignore {
                self.compile(item);

                self.current().next_free_register = self.current().local_count;
            }
        }

        if self.had_error {
            Err(self.errors.clone())
        } else {
            Ok(self.current().ir_buff.clone())
        }
    }

    fn add_inst(&mut self, i: IrOp) {
        self.current().ir_buff.push(i);
    }

    fn compile(&mut self, ast: Ast) -> u8 {
        match ast {
            Ast::Declare { name, value } => {
                let address = self
                    .current()
                    .resolve_local(&name)
                    .unwrap_or_else(|| self.current().add_local(name));
                match *value {
                    Ast::Value(ParserValue::EnumField { name, tag, fields }) => {
                        let (const_idx, tag_idx, args) = self.compile_enum_field(name, tag, fields);
                        self.add_inst(IrOp::LoadEnumField {
                            dest: address,
                            enum_idx: const_idx,
                            tag: tag_idx,
                            args,
                        });
                    }
                    Ast::Value(raw) => {
                        let v = self.convert_const(raw.clone());

                        match v {
                            Value::Int(i) => self.add_inst(IrOp::LoadInt {
                                dest: address,
                                val: i,
                            }),
                            Value::Double(d) => self.add_inst(IrOp::LoadDouble {
                                dest: address,
                                val: d,
                            }),
                            Value::Bool(b) => self.add_inst(IrOp::LoadBool {
                                dest: address,
                                val: b,
                            }),
                            Value::Ref(r) => {
                                if let Some(reg) = self.current().resolve_local(&r) {
                                    self.add_inst(IrOp::LoadReg {
                                        dest: address,
                                        src: reg,
                                    });
                                    return address;
                                }

                                for ctx in self.contexts.iter().rev().skip(1) {
                                    if let Some(&global_reg) = ctx.top_level_names.get(r.as_str()) {
                                        self.add_inst(IrOp::LoadGlobal {
                                            dest: address,
                                            src: global_reg,
                                        });
                                        return address;
                                    }
                                }

                                let hash = Value::Hash(Value::String(r.clone()).get_hash());
                                let const_idx = self.get_or_add_constant(hash);

                                self.add_inst(IrOp::LoadConst {
                                    dest: address,
                                    idx: const_idx,
                                });
                            }
                            Value::Fun(_)
                            | Value::Object(_)
                            | Value::String(_)
                            | Value::NativeFun(_)
                            | Value::Hash(_)
                            | Value::EnumDefinition(_)
                            | Value::EnumField { .. }
                            | Value::Bytes(_) => {
                                let idx = self.get_or_add_constant(v);
                                self.add_inst(IrOp::LoadConst { dest: address, idx });
                            }
                        }
                    }
                    _ => {
                        let res = self.compile(*value);

                        self.add_inst(IrOp::LoadReg {
                            dest: address,
                            src: res,
                        });
                    }
                };

                self.current().next_free_register = self.current().local_count;

                address
            }
            Ast::Value(val) => {
                if let ParserValue::EnumField { name, tag, fields } = val {
                    let reg = self.next_free_address();
                    let (const_idx, tag_idx, args) = self.compile_enum_field(name, tag, fields);
                    self.add_inst(IrOp::LoadEnumField {
                        dest: reg,
                        enum_idx: const_idx,
                        tag: tag_idx,
                        args,
                    });
                    return reg;
                }

                if let ParserValue::Ref(ref_val) = &val {
                    if let Some(reg) = self.current().resolve_local(ref_val) {
                        return reg;
                    }
                    for ctx in self.contexts.iter().rev().skip(1) {
                        if let Some(&global_reg) = ctx.top_level_names.get(ref_val) {
                            let dest_reg = self.next_free_address();

                            self.add_inst(IrOp::LoadGlobal {
                                dest: dest_reg,
                                src: global_reg,
                            });

                            return dest_reg;
                        }
                    }

                    if let Some(_std_func) = self.std.functions.iter().find(|f| f.name == ref_val) {
                        let reg = self.next_free_address();
                        let hash =
                            Value::Hash(Value::String(ref_val.to_string().into()).get_hash());
                        let idx = self.get_or_add_constant(hash);

                        self.add_inst(IrOp::LoadNative {
                            dest: reg,
                            src: idx,
                        });

                        return reg;
                    }
                }

                let reg = self.next_free_address();
                let v = self.convert_const(val.clone());

                match v {
                    Value::Int(i) => self.add_inst(IrOp::LoadInt { dest: reg, val: i }),
                    Value::Double(d) => self.add_inst(IrOp::LoadDouble { dest: reg, val: d }),
                    Value::Bool(b) => self.add_inst(IrOp::LoadBool { dest: reg, val: b }),
                    Value::Ref(r) => {
                        if let Some(reg) = self.current().resolve_local(&r) {
                            self.add_inst(IrOp::LoadReg {
                                dest: reg,
                                src: reg,
                            });
                            return reg;
                        }

                        let hash = Value::Hash(Value::String(r.clone()).get_hash());
                        let const_idx = self.get_or_add_constant(hash);

                        self.add_inst(IrOp::LoadConst {
                            dest: reg,
                            idx: const_idx,
                        });
                    }
                    Value::Fun(_)
                    | Value::Object(_)
                    | Value::String(_)
                    | Value::NativeFun(_)
                    | Value::Hash(_)
                    | Value::EnumDefinition(_)
                    | Value::EnumField { .. }
                    | Value::Bytes(_) => {
                        let idx = self.get_or_add_constant(v);
                        self.add_inst(IrOp::LoadConst { dest: reg, idx });
                    }
                }

                reg
            }
            Ast::Object(entries) => {
                let obj_reg = self.next_free_address();

                let is_fully_static = entries.iter().all(|(key_ast, val_ast)| {
                    matches!(
                        key_ast,
                        Ast::Value(
                            ParserValue::Int(_)
                                | ParserValue::Double(_)
                                | ParserValue::Bool(_)
                                | ParserValue::String(_)
                        )
                    ) && matches!(
                        val_ast,
                        Ast::Value(
                            ParserValue::Int(_)
                                | ParserValue::Double(_)
                                | ParserValue::Bool(_)
                                | ParserValue::String(_)
                        )
                    )
                });
                if is_fully_static {
                    let mut static_map = FxHashMap::default();

                    for (key_ast, val_ast) in entries {
                        let hash_key = if let Ast::Value(val) = key_ast {
                            match val {
                                ParserValue::Ref(ref name) => {
                                    Value::String(Rc::new(name.clone())).get_hash()
                                }
                                _ => {
                                    let converted = self.convert_const(val);
                                    converted.get_hash()
                                }
                            }
                        } else {
                            unreachable!()
                        };

                        let static_val = if let Ast::Value(val) = val_ast {
                            self.convert_const(val)
                        } else {
                            unreachable!()
                        };

                        static_map.insert(hash_key, static_val);
                    }

                    let obj_val = Value::Object(Rc::new(RefCell::new(static_map)));
                    let obj_idx = self.get_or_add_constant(obj_val);

                    self.add_inst(IrOp::LoadObject {
                        dest: obj_reg,
                        src: obj_idx,
                    });
                } else {
                    let obj_val = Value::Object(Rc::new(RefCell::new(FxHashMap::default())));
                    let obj_idx = self.get_or_add_constant(obj_val);

                    self.add_inst(IrOp::LoadObject {
                        dest: obj_reg,
                        src: obj_idx,
                    });

                    for (key_ast, val_ast) in entries {
                        if let Ast::Value(val) = key_ast {
                            let hash = match val {
                                ParserValue::Ref(ref name) => {
                                    Value::Hash(Value::String(Rc::new(name.clone())).get_hash())
                                }
                                _ => {
                                    let converted = self.convert_const(val);
                                    Value::Hash(converted.get_hash())
                                }
                            };
                            let hash_idx = self.get_or_add_constant(hash);

                            let val_reg = self.compile(val_ast);

                            self.add_inst(IrOp::SetProperty {
                                obj: obj_reg,
                                key: hash_idx,
                                val: val_reg,
                            });
                        } else {
                            let key_reg = self.compile(key_ast);
                            let val_reg = self.compile(val_ast);

                            self.add_inst(IrOp::SetPropertyDyn {
                                obj: obj_reg,
                                key: key_reg,
                                val: val_reg,
                            });
                        }
                    }
                }

                obj_reg
            }
            Ast::Return { value } => {
                if self.contexts.len() > 1
                    && let Ast::Call { what, args } = *value
                {
                    let (caller, final_args) = if let Ast::Dot {
                        accessor,
                        access,
                        resolve: _,
                    } = *what.clone()
                    {
                        let is_ufcs = if let Ast::Value(ParserValue::Ref(ref name)) = *access {
                            self.current().resolve_local(name).is_some()
                                || self.std.functions.iter().any(|f| f.name == name)
                        } else {
                            false
                        };

                        if is_ufcs {
                            let mut new_args = vec![*accessor];
                            new_args.extend(args);
                            (self.compile(*access), new_args)
                        } else {
                            (self.compile(*what), args)
                        }
                    } else {
                        (self.compile(*what), args)
                    };

                    let mut compiled_args = vec![];
                    for arg in &final_args {
                        compiled_args.push(self.compile(arg.clone()));
                    }

                    self.add_inst(IrOp::TailCall {
                        what: caller,
                        args: compiled_args,
                    });
                    return 0;
                }

                let reg = self.compile(*value);
                self.add_inst(IrOp::Return { value: reg });
                0
            }
            Ast::Call { what, args } => {
                let (caller, final_args) = if let Ast::Dot {
                    accessor,
                    access,
                    resolve: _,
                } = *what.clone()
                {
                    let is_ufcs = if let Ast::Value(ParserValue::Ref(ref name)) = *access {
                        self.current().resolve_local(name).is_some()
                            || self.std.functions.iter().any(|f| f.name == name)
                    } else {
                        false
                    };

                    if is_ufcs {
                        let mut new_args = vec![*accessor];
                        new_args.extend(args);
                        (self.compile(*access), new_args)
                    } else {
                        (self.compile(*what), args)
                    }
                } else {
                    (self.compile(*what), args)
                };

                let mut compiled_args = vec![];
                for arg in &final_args {
                    compiled_args.push(self.compile(arg.clone()));
                }

                let return_adr = self.next_free_address();

                self.add_inst(IrOp::Call {
                    dest: return_adr,
                    what: caller,
                    args: compiled_args,
                });

                self.current().next_free_register = return_adr + 1;
                return_adr
            }
            Ast::Binary {
                left,
                right,
                operator,
            } => {
                let compiled_left = self.compile(*left);
                let compiled_right = self.compile(*right);

                let reg = self.next_free_address();

                self.add_inst(IrOp::Binary {
                    dest: reg,
                    op: operator,
                    left: compiled_left,
                    right: compiled_right,
                });

                self.current().next_free_register = reg + 1;
                reg
            }
            Ast::If {
                then_branch,
                else_branch,
                condition,
            } => {
                let cond = self.compile(*condition);
                let else_label = self.new_label();
                let end_label = self.new_label();

                self.add_inst(IrOp::JumpNot {
                    condition: cond,
                    target: else_label,
                });

                self.compile(*then_branch);

                if let Some(else_body) = else_branch {
                    self.add_inst(IrOp::Jump { target: end_label });
                    self.add_inst(IrOp::Label(else_label));

                    self.compile(*else_body);

                    self.add_inst(IrOp::Label(end_label));
                } else {
                    self.add_inst(IrOp::Label(else_label));
                }

                0
            }
            Ast::ContinueCode => self.compile_continue(),
            Ast::BreakCode => self.compile_break(),
            Ast::Ignore => 0,
            Ast::For { condition, body } => {
                let start_label = self.new_label();
                let end_label = self.new_label();

                self.add_inst(IrOp::Label(start_label));

                let cond_reg = self.compile(*condition);
                self.add_inst(IrOp::JumpNot {
                    target: end_label,
                    condition: cond_reg,
                });

                self.current().loop_stack.push(LoopContext {
                    start: start_label,
                    end: end_label,
                });

                self.compile(*body);

                self.add_inst(IrOp::Jump {
                    target: start_label,
                });

                self.add_inst(IrOp::Label(end_label));

                0
            }
            Ast::ForEach {
                iterable,
                var_name,
                body,
            } => {
                let iter = Ast::Declare {
                    name: "@iter".to_string(),
                    value: Box::new(Ast::Call {
                        what: Box::new(Ast::Value(ParserValue::Ref("__iter_of".to_string()))),
                        args: vec![*iterable],
                    }),
                };

                let iter_ref = Ast::Value(ParserValue::Ref("@iter".to_string()));

                let has_next = Ast::Call {
                    what: Box::new(Ast::Value(ParserValue::Ref("__has_next".to_string()))),
                    args: vec![iter_ref.clone()],
                };

                let decl = Ast::Declare {
                    name: var_name,
                    value: Box::new(Ast::Call {
                        what: Box::new(Ast::Value(ParserValue::Ref("__get_next".to_string()))),
                        args: vec![iter_ref],
                    }),
                };

                let new_body = match *body {
                    Ast::Block { mut code } => {
                        let mut codes = vec![decl];
                        codes.append(&mut code);
                        Ast::Block { code: codes }
                    }
                    _ => Ast::Block {
                        code: vec![decl, *body],
                    },
                };

                self.compile(iter);

                self.compile(Ast::For {
                    condition: Box::new(has_next),
                    body: Box::new(new_body),
                })
            }
            Ast::Block { code } => {
                self.current().begin_scope();

                let mut last_reg = 0;
                for ast in code {
                    last_reg = self.compile(ast);
                    self.current().next_free_register = self.current().local_count;
                }

                self.current().end_scope();

                last_reg
            }

            Ast::Dot {
                accessor,
                access,
                resolve,
            } => {
                if let Ast::Set { name, value } = *access {
                    return self.compile(Ast::Set {
                        name: Box::new(Ast::Dot {
                            accessor,
                            access: name,
                            resolve,
                        }),
                        value,
                    });
                }

                let compiled_accessor = self.compile(*accessor);

                if resolve {
                    let compiled_access = self.compile(*access);
                    let reg = self.next_free_address();

                    self.add_inst(IrOp::GetPropertyDyn {
                        dest: reg,
                        obj: compiled_accessor,
                        key: compiled_access,
                    });
                    self.current().next_free_register = reg + 1;
                    reg
                } else if let Ast::Value(val) = *access {
                    let access_hash = match val {
                        ParserValue::Ref(ref name) => {
                            Value::String(Rc::new(name.clone())).get_hash()
                        }
                        _ => {
                            let converted = self.convert_const(val);
                            converted.get_hash()
                        }
                    };

                    let hash_val = Value::Hash(access_hash);
                    let hash_idx = self.get_or_add_constant(hash_val);

                    let reg = self.next_free_address();

                    self.add_inst(IrOp::GetProperty {
                        dest: reg,
                        obj: compiled_accessor,
                        key: hash_idx,
                    });
                    self.current().next_free_register = reg + 1;
                    reg
                } else {
                    panic!("AST Error: Static property access requires Ast::Value");
                }
            }

            Ast::Set { name, value } => match *name {
                Ast::Value(ParserValue::Ref(name)) => {
                    let dest = self.current().resolve_local(&name);

                    let lhs_reg = dest.unwrap_or_else(|| self.current().add_local(name));

                    if let Ast::Value(ParserValue::EnumField { name, tag, fields }) = *value {
                        let (const_idx, tag_idx, args) = self.compile_enum_field(name, tag, fields);
                        self.add_inst(IrOp::LoadEnumField {
                            dest: lhs_reg,
                            enum_idx: const_idx,
                            tag: tag_idx,
                            args,
                        });
                    } else if let Ast::Value(raw) = *value {
                        let v = self.convert_const(raw.clone());

                        match v {
                            Value::Int(i) => self.add_inst(IrOp::LoadInt {
                                dest: lhs_reg,
                                val: i,
                            }),
                            Value::Double(d) => self.add_inst(IrOp::LoadDouble {
                                dest: lhs_reg,
                                val: d,
                            }),
                            Value::Bool(b) => self.add_inst(IrOp::LoadBool {
                                dest: lhs_reg,
                                val: b,
                            }),
                            Value::Ref(r) => {
                                if let Some(rhs_reg) = self.current().resolve_local(&r) {
                                    self.add_inst(IrOp::LoadReg {
                                        dest: lhs_reg,
                                        src: rhs_reg,
                                    });
                                    return lhs_reg;
                                }

                                let hash = Value::Hash(Value::String(r.clone()).get_hash());
                                let hash_const = self.get_or_add_constant(hash);

                                self.add_inst(IrOp::LoadConst {
                                    dest: lhs_reg,
                                    idx: hash_const,
                                });
                            }
                            Value::Fun(_)
                            | Value::Object(_)
                            | Value::String(_)
                            | Value::NativeFun(_)
                            | Value::Hash(_)
                            | Value::EnumDefinition(_)
                            | Value::EnumField { .. }
                            | Value::Bytes(_) => {
                                let idx = self.get_or_add_constant(v);
                                self.add_inst(IrOp::LoadConst { dest: lhs_reg, idx });
                            }
                        }
                    } else {
                        let res = self.compile(*value);

                        self.add_inst(IrOp::LoadReg {
                            dest: lhs_reg,
                            src: res,
                        });
                    }
                    0
                }
                Ast::Dot {
                    accessor,
                    access,
                    resolve,
                } => {
                    let compiled_accessor = self.compile(*accessor);
                    let compiled_value = self.compile(*value);

                    if resolve {
                        let compiled_access = self.compile(*access);

                        self.add_inst(IrOp::SetPropertyDyn {
                            obj: compiled_accessor,
                            key: compiled_access,
                            val: compiled_value,
                        });
                    } else if let Ast::Value(val) = *access {
                        let hash_access = match val {
                            ParserValue::Ref(ref name) => {
                                Value::String(Rc::new(name.clone())).get_hash()
                            }
                            _ => {
                                let converted = self.convert_const(val);
                                converted.get_hash()
                            }
                        };

                        let hash_val = Value::Hash(hash_access);
                        let hash_idx = self.get_or_add_constant(hash_val);

                        self.add_inst(IrOp::SetProperty {
                            obj: compiled_accessor,
                            key: hash_idx,
                            val: compiled_value,
                        });
                    } else {
                        panic!("AST Error: Static property assignment requires Ast::Value");
                    }

                    0
                }
                _ => panic!("WrongType"),
            },

            Ast::Import {
                import_type,
                import_map,
                source,
                alias: _,
            } => {
                if import_type == ImportType::Syntax || import_type == ImportType::Translation {
                    return 0;
                }

                let module_fun = self.compile_module(source);

                let fun_idx = self.get_or_add_constant(module_fun);

                let module_reg = self.next_free_address();

                self.add_inst(IrOp::LoadFun {
                    dest: module_reg,
                    src: fun_idx,
                });

                let res_reg = self.next_free_address();

                self.add_inst(IrOp::Call {
                    dest: res_reg,
                    what: module_reg,
                    args: vec![],
                });

                if let Some(map) = import_map {
                    for name in map {
                        let reg = self.current().add_local(name.clone());
                        let hash = Value::Hash(Value::String(name.into()).get_hash());
                        let hash_idx = self.get_or_add_constant(hash);

                        self.add_inst(IrOp::GetProperty {
                            dest: reg,
                            obj: res_reg,
                            key: hash_idx,
                        });
                    }
                }

                res_reg
            }
            Ast::EnumDef { name, variants } => {
                if let Some(pos) = self.enums.iter().position(|e| e.name == name) {
                    let idx = self.enums[pos].idx;
                    self.enums[pos] = EnumDef {
                        name,
                        variants,
                        idx,
                    };
                } else {
                    let enum_val = Value::EnumDefinition(self.enums.len().try_into().unwrap());
                    let idx = self.get_or_add_constant(enum_val);
                    self.enums.push(EnumDef {
                        name,
                        variants,
                        idx,
                    });
                }

                0
            }
            Ast::Match { target, arms } => {
                use crate::parser::MatchPattern;
                let target_reg = self.compile(*target);
                let result_reg = self.next_free_address();
                let end_match_label = self.new_label();
                let match_base = self.current().next_free_register;

                for arm in arms {
                    let next_arm_label = self.new_label();
                    self.current().next_free_register = match_base;
                    self.current().local_count = match_base;
                    self.current().begin_scope();
                    match arm.pattern {
                        MatchPattern::Enum { name, tag, fields } => {
                            let enum_idx = match self.enums.iter().position(|e| e.name == *name) {
                                Some(idx) => idx,
                                None => {
                                    self.error(
                                        0,
                                        0,
                                        format!(
                                            "referencing enum definition that doesn't exist: '{}'",
                                            name
                                        ),
                                    );
                                    continue;
                                }
                            };
                            let enum_def = self.enums[enum_idx].clone();
                            let tag_idx = enum_def.variants.iter().position(|v| v.name == *tag);
                            let tag_idx = if let Some(idx) = tag_idx {
                                idx
                            } else {
                                self.error(
                                    0,
                                    0,
                                    format!(
                                        "referencing enum field that doesn't exist: '{}::{}'",
                                        name, tag
                                    ),
                                );
                                continue;
                            };
                            let tag_info = &enum_def.variants[tag_idx];

                            if fields.len() != tag_info.fields.len() {
                                self.error(
                                    0,
                                    0,
                                    format!(
                                        "pattern for '{}::{}' expected {} fields, got {}",
                                        name,
                                        tag,
                                        tag_info.fields.len(),
                                        fields.len()
                                    ),
                                );
                                continue;
                            }

                            let check_reg = self.next_free_address();
                            self.add_inst(IrOp::MatchEnum {
                                dest: check_reg,
                                src: target_reg,
                                enum_idx,
                                tag: tag_idx as u8,
                            });

                            self.add_inst(IrOp::JumpNot {
                                target: next_arm_label,
                                condition: check_reg,
                            });

                            for (i, field_binding) in fields.into_iter().enumerate() {
                                if field_binding != "_" {
                                    let field_def_name = &tag_info.fields[i];
                                    let hash = Value::Hash(
                                        Value::String(Rc::new(field_def_name.clone())).get_hash(),
                                    );
                                    let hash_idx = self.get_or_add_constant(hash);

                                    let field_reg = self.current().add_local(field_binding);
                                    self.add_inst(IrOp::GetProperty {
                                        dest: field_reg,
                                        obj: target_reg,
                                        key: hash_idx,
                                    });
                                }
                            }
                        }
                        MatchPattern::Value(val) => {
                            let val_reg = self.next_free_address();
                            let v = self.convert_const(val);
                            match v {
                                Value::Int(i) => self.add_inst(IrOp::LoadInt {
                                    dest: val_reg,
                                    val: i,
                                }),
                                Value::Double(d) => self.add_inst(IrOp::LoadDouble {
                                    dest: val_reg,
                                    val: d,
                                }),
                                Value::Bool(b) => self.add_inst(IrOp::LoadBool {
                                    dest: val_reg,
                                    val: b,
                                }),
                                _ => {
                                    let idx = self.get_or_add_constant(v);
                                    self.add_inst(IrOp::LoadConst { dest: val_reg, idx });
                                }
                            }

                            let cmp_reg = self.next_free_address();
                            self.add_inst(IrOp::Binary {
                                dest: cmp_reg,
                                op: BinaryOperator::Equals,
                                left: target_reg,
                                right: val_reg,
                            });

                            self.add_inst(IrOp::JumpNot {
                                target: next_arm_label,
                                condition: cmp_reg,
                            });
                        }
                        MatchPattern::Wildcard(binding) => {
                            if let Some(var_name) = binding
                                && var_name != "_"
                            {
                                let bound_reg = self.current().add_local(var_name);
                                self.add_inst(IrOp::LoadReg {
                                    dest: bound_reg,
                                    src: target_reg,
                                });
                            }
                        }
                    }

                    let body_reg = self.compile(*arm.body);
                    self.add_inst(IrOp::LoadReg {
                        dest: result_reg,
                        src: body_reg,
                    });

                    self.current().end_scope();

                    self.add_inst(IrOp::Jump {
                        target: end_match_label,
                    });
                    self.add_inst(IrOp::Label(next_arm_label));
                }

                self.add_inst(IrOp::Label(end_match_label));
                result_reg
            }
        }
    }

    fn compile_continue(&mut self) -> u8 {
        if let Some(loop_ctx) = self.current().loop_stack.last() {
            let start = loop_ctx.start;
            self.add_inst(IrOp::Jump { target: start });
        } else {
            self.error(0, 0, "cannot use 'continue' outside of a loop");
        }

        0
    }

    fn compile_break(&mut self) -> u8 {
        if let Some(loop_ctx) = self.current().loop_stack.last() {
            let end = loop_ctx.end;
            self.add_inst(IrOp::Jump { target: end });
        } else {
            self.error(0, 0, "cannot use 'break' outside of a loop");
        }

        0
    }

    fn convert_const(&mut self, val: ParserValue) -> Value {
        match val {
            ParserValue::Int(int) => Value::Int(int),
            ParserValue::Double(dbl) => Value::Double(dbl),
            ParserValue::Bool(bol) => Value::Bool(bol),
            ParserValue::String(string) => Value::String(string.into()),
            ParserValue::Ref(reference) => {
                if let Some(native) = self.std.functions.iter().find(|f| f.name == reference) {
                    return Value::NativeFun(native.clone());
                }
                Value::Ref(reference.into())
            }
            ParserValue::Fun { args, body } => {
                self.contexts.push(Context::new());
                let arity = args.len().try_into().unwrap();

                for arg in args {
                    self.current().add_local(arg);
                }

                self.compile(*body);

                let fun = self.contexts.pop().unwrap();
                let fun_ir = if self.optimize {
                    IrOptimizer::optimize(fun.ir_buff)
                } else {
                    fun.ir_buff
                };
                let frame_size = IrOp::compute_frame_size(arity, &fun_ir);
                let code = Emitter {}.emit(fun_ir);

                Value::Fun(Rc::new(Function {
                    arity,
                    frame_size,
                    code: code.into(),
                }))
            }
            ParserValue::Object(entries) => {
                self.contexts.push(Context::new());

                let mut entries_compiled = FxHashMap::default();

                for (key, value) in entries {
                    let converted = self.convert_const(key);
                    let val = self.convert_const(value);

                    entries_compiled.insert(converted.get_hash(), val);
                }

                self.contexts.pop();

                Value::Object(Rc::new(RefCell::new(entries_compiled)))
            }
            ParserValue::EnumField { .. } => {
                unreachable!("EnumField is compiled directly and not via convert_const")
            }
        }
    }

    fn compile_enum_field(
        &mut self,
        name: String,
        tag: String,
        fields: Vec<Ast>,
    ) -> (usize, u8, Vec<(u64, u8)>) {
        let idx = self.enums.iter().position(|e| e.name == name);

        if idx.is_none() {
            self.error(
                0,
                0,
                format!("referencing enum definition that doesn't exist: '{}'", name),
            );
            return (0, 0, vec![]);
        }

        let enum_def = self.enums[idx.unwrap()].clone();

        let tag_idx = enum_def.variants.iter().position(|v| v.name == tag);

        if tag_idx.is_none() {
            self.error(
                0,
                0,
                format!(
                    "referencing enum field that doesn't exist: '{}::{}'",
                    name, tag
                ),
            );
            return (enum_def.idx, 0, vec![]);
        }

        let tag_info = enum_def.variants[tag_idx.unwrap()].clone();

        if tag_info.fields.len() != fields.len() {
            self.error(
                0,
                0,
                format!(
                    "expected {} values, got {} in {}::{}",
                    tag_info.fields.len(),
                    fields.len(),
                    name,
                    tag
                ),
            );
        }

        let mut arg_map = vec![];

        for (i, field) in fields.iter().enumerate() {
            let field_def = Rc::new(tag_info.fields[i].clone());

            let reg = self.compile(field.clone());
            arg_map.push((Value::String(field_def).get_hash(), reg));
        }

        (idx.unwrap(), tag_idx.unwrap() as u8, arg_map)
    }
}
