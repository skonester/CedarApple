use crate::emitter::OpCode;
use crate::std::StdModule;
use crate::value::{NativeFunction, Value};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct Frame {
    pub pointer: usize,
    pub ip: usize,
    pub bytecode: Rc<[u8]>,
    pub return_reg: u8,
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Error {
    FrameUnderflow,
    UnexpectedTypeLoad(OpCode),
    UnexpectedTypeCall,
    UnexpectedType,
    PropertyNotFound(u64),
    NativeFunctionFailed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::FrameUnderflow => write!(f, "call stack underflow"),
            Error::UnexpectedTypeLoad(op) => {
                write!(f, "unexpected value type for load operation ({:?})", op)
            }
            Error::UnexpectedTypeCall => write!(f, "attempted to call a non-callable value"),
            Error::UnexpectedType => write!(f, "unexpected value type for operation"),
            Error::PropertyNotFound(hash) => {
                write!(f, "property with hash {:#018x} not found", hash)
            }
            Error::NativeFunctionFailed(msg) => write!(f, "native function failed: {}", msg),
        }
    }
}

impl std::error::Error for Error {}

impl Error {
    pub fn to_diagnostic(&self, file: Option<&str>) -> crate::diagnostic::Diagnostic {
        let mut diag = crate::diagnostic::Diagnostic::error(format!("runtime error: {}", self));
        if let Some(f) = file {
            diag = diag.with_file(f);
        }
        diag
    }
}
#[derive(Clone)]
pub struct VM {
    pub stack: Vec<Value>,
    pub frames: Vec<Frame>,
    pub constants: Vec<Value>,
    exit_value: Option<Value>,
    pub patch_table: FxHashMap<u64, FxHashMap<u64, Value>>,
    pub native_functions: Vec<NativeFunction>,
    pub native_map: FxHashMap<u64, NativeFunction>,
}

impl VM {
    pub fn new(code: Vec<u8>, constants: Vec<Value>) -> Self {
        let mut stack = Vec::with_capacity(8192);

        stack.resize(256, Value::Int(0));

        Self {
            frames: vec![Frame {
                pointer: 0,
                ip: 0,
                bytecode: code.into(),
                return_reg: 0,
            }],
            stack,
            constants,
            exit_value: None,
            patch_table: FxHashMap::default(),
            native_map: StdModule::get_core()
                .functions
                .into_iter()
                .map(|f| {
                    let hash = Value::String(f.name.to_string().into()).get_hash();
                    (hash, f)
                })
                .collect(),
            native_functions: StdModule::get_core().functions,
        }
    }

    pub fn with_natives(
        code: Vec<u8>,
        constants: Vec<Value>,
        natives: Vec<NativeFunction>,
    ) -> Self {
        let mut vm = Self::new(code, constants);
        for f in natives {
            let hash = Value::String(f.name.to_string().into()).get_hash();
            vm.native_map.insert(hash, f.clone());
            vm.native_functions.push(f);
        }
        vm
    }

    pub fn run_with_frame(&mut self, frame: Frame) -> Result<Option<Value>, Error> {
        self.frames = vec![frame];
        self.run()
    }

    pub fn run(&mut self) -> Result<Option<Value>, Error> {
        if self.frames.is_empty() {
            return Ok(self.exit_value.clone());
        }

        let mut frame_idx = self.frames.len() - 1;
        let mut fp = self.frames[frame_idx].pointer;
        let mut ip = self.frames[frame_idx].ip;
        let mut code = Rc::clone(&self.frames[frame_idx].bytecode);

        loop {
            if ip >= code.len() {
                break;
            }

            let opcode = code[ip];
            ip += 1;
            match opcode {
                0x00 => {
                    // LoadInt
                    let dest = code[ip] as usize;
                    let raw_bytes: [u8; 8] = code[ip + 1..ip + 9].try_into().unwrap();
                    ip += 9;
                    self.stack[fp + dest] = Value::Int(i64::from_le_bytes(raw_bytes));
                }
                0x08 => {
                    // LoadIntSmall
                    let dest = code[ip] as usize;
                    let val = code[ip + 1] as i8 as i64;
                    ip += 2;
                    self.stack[fp + dest] = Value::Int(val);
                }
                0x01 => {
                    // LoadDouble
                    let dest = code[ip] as usize;
                    let raw_bytes: [u8; 8] = code[ip + 1..ip + 9].try_into().unwrap();
                    ip += 9;
                    self.stack[fp + dest] = Value::Double(f64::from_le_bytes(raw_bytes));
                }
                0x02 => {
                    // LoadBool
                    let dest = code[ip] as usize;
                    let b = code[ip + 1];
                    ip += 2;
                    self.stack[fp + dest] = Value::Bool(b != 0);
                }
                0x03 | 0x05 => {
                    // LoadConst | LoadFun
                    let dest = code[ip] as usize;
                    let bytes: [u8; 2] = code[ip + 1..ip + 3].try_into().unwrap();
                    ip += 3;
                    let idx = u16::from_le_bytes(bytes) as usize;
                    self.stack[fp + dest] = self.constants[idx].clone();
                }
                0x04 => {
                    // LoadReg
                    let dest = code[ip] as usize;
                    let src = code[ip + 1] as usize;
                    ip += 2;
                    self.stack[fp + dest] = self.stack[fp + src].clone();
                }
                0x06 => {
                    // LoadObject
                    let dest = code[ip] as usize;
                    let bytes: [u8; 2] = code[ip + 1..ip + 3].try_into().unwrap();
                    ip += 3;
                    let idx = u16::from_le_bytes(bytes) as usize;
                    if let Some(Value::Object(ref_cell)) = self.constants.get(idx) {
                        let cloned_map = ref_cell.borrow().clone();
                        self.stack[fp + dest] = Value::Object(Rc::new(RefCell::new(cloned_map)));
                    } else {
                        self.stack[fp + dest] =
                            Value::Object(Rc::new(RefCell::new(FxHashMap::default())));
                    }
                }
                0x07 => {
                    // LoadEnum
                    let dest = code[ip] as usize;
                    let enum_idx = u16::from_le_bytes([code[ip + 1], code[ip + 2]]);
                    let tag = code[ip + 3];
                    let count = code[ip + 4];
                    ip += 5;

                    let mut args = Vec::with_capacity(count as usize);
                    for _ in 0..count {
                        let raw_key_bytes: [u8; 8] = code[ip..ip + 8].try_into().unwrap();
                        let hash = u64::from_le_bytes(raw_key_bytes);
                        let arg_reg = code[ip + 8] as usize;
                        ip += 9;

                        let arg_val = self.stack[fp + arg_reg].clone();
                        args.push((hash, arg_val));
                    }

                    self.stack[fp + dest] = Value::EnumField {
                        const_idx: enum_idx as usize,
                        tag,
                        args: args.into(),
                    };
                }
                0x10 => {
                    // Return
                    let src_reg = code[ip] as usize;

                    let return_value = self.stack[fp + src_reg].clone();

                    if self.frames.len() <= 1 {
                        self.exit_value = Some(return_value);
                        self.frames.pop();
                        break;
                    }

                    let frame = self.frames.pop().unwrap();
                    self.stack.truncate(frame.pointer);

                    frame_idx = self.frames.len() - 1;
                    fp = self.frames[frame_idx].pointer;
                    ip = self.frames[frame_idx].ip;
                    code = Rc::clone(&self.frames[frame_idx].bytecode);

                    self.stack[fp + frame.return_reg as usize] = return_value;
                }
                0x11 => {
                    // Call
                    let dest_reg = code[ip];
                    let fun_reg = code[ip + 1];
                    let arg_count = code[ip + 2];
                    ip += 3;

                    match &self.stack[fp + fun_reg as usize] {
                        Value::Fun(fun) => {
                            let fun = Rc::clone(fun);
                            let new_fp = self.stack.len();

                            for _ in 0..arg_count {
                                let arg_reg_idx = code[ip] as usize;
                                ip += 1;
                                let arg_val = self.stack[fp + arg_reg_idx].clone();
                                self.stack.push(arg_val);
                            }

                            let padding =
                                (fun.frame_size as usize).saturating_sub(arg_count as usize);
                            self.stack.resize(self.stack.len() + padding, Value::Int(0));

                            self.frames[frame_idx].ip = ip;
                            self.frames.push(Frame {
                                ip: 0,
                                pointer: new_fp,
                                return_reg: dest_reg,
                                bytecode: Rc::clone(&fun.code),
                            });
                            frame_idx = self.frames.len() - 1;
                            fp = new_fp;
                            ip = 0;
                            code = Rc::clone(&self.frames[frame_idx].bytecode);
                        }
                        Value::NativeFun(native_fn) => {
                            let native_fn = native_fn.clone();
                            let result = if (arg_count as usize) <= 8 {
                                let mut small_args = [const { Value::Int(0) }; 8];
                                for slot in small_args.iter_mut().take(arg_count as usize) {
                                    let arg_reg = code[ip] as usize;
                                    ip += 1;
                                    *slot = self.stack[fp + arg_reg].clone();
                                }
                                self.frames[frame_idx].ip = ip;
                                (native_fn.call)(&small_args[..arg_count as usize])
                            } else {
                                let mut args = Vec::with_capacity(arg_count as usize);
                                for _ in 0..arg_count {
                                    let arg_reg = code[ip] as usize;
                                    ip += 1;
                                    args.push(self.stack[fp + arg_reg].clone());
                                }
                                self.frames[frame_idx].ip = ip;
                                (native_fn.call)(&args)
                            }
                            .map_err(|e| {
                                Error::NativeFunctionFailed(format!(
                                    "Error in native function '{}': {}, IP: {}",
                                    native_fn.name, e, ip
                                ))
                            })?;

                            self.stack[fp + dest_reg as usize] = result;
                        }
                        _ => return Err(Error::UnexpectedTypeCall),
                    }
                }
                0x13 => {
                    // Jump
                    let offset = u16::from_le_bytes([code[ip], code[ip + 1]]) as usize;
                    ip = offset;
                }
                0x16 => {
                    // JumpNot
                    let jump_condition = code[ip] as usize;
                    let offset = u16::from_le_bytes([code[ip + 1], code[ip + 2]]) as usize;
                    ip += 3;

                    if !Self::is_truthy_ref(&self.stack[fp + jump_condition]) {
                        ip = offset;
                    }
                }
                0x17 => {
                    // TailCall (TCO in-place frame reuse)
                    let fun_reg = code[ip];
                    let arg_count = code[ip + 1] as usize;
                    ip += 2;

                    match &self.stack[fp + fun_reg as usize] {
                        Value::Fun(fun) => {
                            let fun = Rc::clone(fun);
                            let mut temp_args = Vec::with_capacity(arg_count);
                            for _ in 0..arg_count {
                                let arg_reg_idx = code[ip] as usize;
                                ip += 1;
                                temp_args.push(self.stack[fp + arg_reg_idx].clone());
                            }

                            for (i, arg) in temp_args.into_iter().enumerate() {
                                self.stack[fp + i] = arg;
                            }

                            let target_len = fp + fun.frame_size as usize;
                            if self.stack.len() < target_len {
                                self.stack.resize(target_len, Value::Int(0));
                            } else {
                                self.stack.truncate(target_len);
                            }

                            ip = 0;
                            code = Rc::clone(&fun.code);
                            self.frames[frame_idx].bytecode = Rc::clone(&code);
                            self.frames[frame_idx].ip = 0;
                        }
                        Value::NativeFun(native_fn) => {
                            let native_fn = native_fn.clone();
                            let result = if arg_count <= 8 {
                                let mut small_args = [const { Value::Int(0) }; 8];
                                for slot in small_args.iter_mut().take(arg_count) {
                                    let arg_reg = code[ip] as usize;
                                    ip += 1;
                                    *slot = self.stack[fp + arg_reg].clone();
                                }
                                (native_fn.call)(&small_args[..arg_count])
                            } else {
                                let mut args = Vec::with_capacity(arg_count);
                                for _ in 0..arg_count {
                                    let arg_reg = code[ip] as usize;
                                    ip += 1;
                                    args.push(self.stack[fp + arg_reg].clone());
                                }
                                (native_fn.call)(&args)
                            }
                            .map_err(|e| {
                                Error::NativeFunctionFailed(format!(
                                    "Error in native function '{}': {}, IP: {}",
                                    native_fn.name, e, ip
                                ))
                            })?;

                            if self.frames.len() <= 1 {
                                self.exit_value = Some(result);
                                self.frames.pop();
                                break;
                            }

                            let frame = self.frames.pop().unwrap();
                            self.stack.truncate(frame.pointer);

                            frame_idx = self.frames.len() - 1;
                            fp = self.frames[frame_idx].pointer;
                            ip = self.frames[frame_idx].ip;
                            code = Rc::clone(&self.frames[frame_idx].bytecode);

                            self.stack[fp + frame.return_reg as usize] = result;
                        }
                        _ => return Err(Error::UnexpectedTypeCall),
                    }
                }
                0x12 => {
                    // Binary
                    let op = code[ip];
                    let dest = code[ip + 1] as usize;
                    let left_reg = code[ip + 2] as usize;
                    let right_reg = code[ip + 3] as usize;
                    ip += 4;

                    Self::binary_fast(
                        op,
                        fp + dest,
                        fp + left_reg,
                        fp + right_reg,
                        &mut self.stack,
                    );
                }
                0x21 => {
                    // GetProperty
                    let dest = code[ip] as usize;
                    let src_idx = code[ip + 1] as usize;
                    let idx = u16::from_le_bytes([code[ip + 2], code[ip + 3]]) as usize;
                    ip += 4;

                    let hash = match self.constants.get(idx) {
                        Some(Value::Hash(h)) => *h,
                        _ => panic!("Expected hash constant at index {}", idx),
                    };
                    let val = match &self.stack[fp + src_idx] {
                        Value::Object(obj_ref) => {
                            let map = obj_ref.borrow();
                            if let Some(v) = map.get(&hash) {
                                Some(v.clone())
                            } else if let Some(patched) =
                                self.patch_table.get(&0).and_then(|p| p.get(&hash))
                            {
                                Some(patched.clone())
                            } else {
                                return Err(Error::PropertyNotFound(hash));
                            }
                        }
                        Value::EnumField { args, .. } => {
                            if let Some((_, val)) = args.iter().find(|(h, _)| *h == hash) {
                                Some(val.clone())
                            } else {
                                return Err(Error::PropertyNotFound(hash));
                            }
                        }
                        _ => return Err(Error::UnexpectedType),
                    };

                    if let Some(v) = val {
                        self.stack[fp + dest] = v;
                    }
                }
                0x22 => {
                    // SetProperty
                    let obj_idx = code[ip] as usize;
                    let const_idx = u16::from_le_bytes([code[ip + 1], code[ip + 2]]) as usize;
                    let val_idx = code[ip + 3] as usize;
                    ip += 4;

                    let hash = match self.constants.get(const_idx) {
                        Some(Value::Hash(h)) => *h,
                        _ => panic!("Expected hash constant"),
                    };
                    let new_val = self.stack[fp + val_idx].clone();

                    if let Value::Object(obj_ref) = &self.stack[fp + obj_idx] {
                        obj_ref.borrow_mut().insert(hash, new_val);
                    }
                }
                0x24 => {
                    // SetPropertyDyn
                    let obj_reg = code[ip] as usize;
                    let key_reg = code[ip + 1] as usize;
                    let val_reg = code[ip + 2] as usize;
                    ip += 3;

                    let key_val = &self.stack[fp + key_reg];
                    let runtime_hash = key_val.get_hash();
                    let val = self.stack[fp + val_reg].clone();

                    if let Value::Object(rc_map) = &self.stack[fp + obj_reg] {
                        rc_map.borrow_mut().insert(runtime_hash, val);
                    } else if let Value::Bytes(bytes_ref) = &self.stack[fp + obj_reg] {
                        let index = match key_val {
                            Value::Int(i) => *i,
                            _ => panic!("Runtime Error: Bytes index must be integer"),
                        };
                        let byte_val = match val {
                            Value::Int(i) => (i & 0xFF) as u8,
                            _ => panic!(
                                "Runtime Error: Value assigned to Bytes index must be integer byte"
                            ),
                        };
                        let mut b = bytes_ref.borrow_mut();
                        if index < 0 || (index as usize) >= b.len() {
                            panic!(
                                "Runtime Error: Bytes index out of bounds: {} (len {})",
                                index,
                                b.len()
                            );
                        }
                        b[index as usize] = byte_val;
                    } else {
                        panic!("Attempted to set property on non-object");
                    }
                }

                0x23 => {
                    // GetPropertyDyn
                    let target_reg = code[ip] as usize;
                    let obj_reg = code[ip + 1] as usize;
                    let key_reg = code[ip + 2] as usize;
                    ip += 3;

                    let key_val = &self.stack[fp + key_reg];
                    let runtime_hash = key_val.get_hash();

                    let val = match &self.stack[fp + obj_reg] {
                        Value::Object(rc_map) => {
                            let map = rc_map.borrow();
                            map.get(&runtime_hash)
                                .cloned()
                                .ok_or(Error::PropertyNotFound(runtime_hash))?
                        }
                        Value::EnumField { args, .. } => {
                            if let Some((_, val)) = args.iter().find(|(h, _)| *h == runtime_hash) {
                                val.clone()
                            } else {
                                return Err(Error::PropertyNotFound(runtime_hash));
                            }
                        }
                        Value::Bytes(bytes_ref) => {
                            let index = match key_val {
                                Value::Int(i) => *i,
                                _ => panic!("Runtime Error: Bytes index must be integer"),
                            };
                            let b = bytes_ref.borrow();
                            if index < 0 || (index as usize) >= b.len() {
                                panic!(
                                    "Runtime Error: Bytes index out of bounds: {} (len {})",
                                    index,
                                    b.len()
                                );
                            }
                            Value::Int(b[index as usize] as i64)
                        }
                        _ => panic!("Runtime Error: Attempted to get property from non-object"),
                    };

                    self.stack[fp + target_reg] = val;
                }
                0x25 => {
                    // LoadNative
                    let dest = code[ip] as usize;
                    let hash_idx = u16::from_le_bytes([code[ip + 1], code[ip + 2]]) as usize;
                    ip += 3;

                    let hash_val = match self.constants.get(hash_idx) {
                        Some(Value::Hash(h)) => *h,
                        _ => panic!("Expected hash constant"),
                    };
                    let found = match self.native_map.get(&hash_val) {
                        Some(f) => f.clone(),
                        None => self
                            .native_functions
                            .iter()
                            .find(|f| {
                                Value::Hash(Value::String(f.name.to_string().into()).get_hash())
                                    == Value::Hash(hash_val)
                            })
                            .expect("Native function not found")
                            .clone(),
                    };
                    self.stack[fp + dest] = Value::NativeFun(found);
                }
                0x26 => {
                    // LoadGlobal
                    let dest = code[ip] as usize;
                    let global_reg = code[ip + 1] as usize;
                    ip += 2;

                    let global_val = self.stack[global_reg].clone();
                    self.stack[fp + dest] = global_val;
                }
                0x18 => {
                    // Inc
                    let src = code[ip] as usize;
                    ip += 1;

                    if let Value::Int(ref mut i) = self.stack[fp + src] {
                        *i += 1;
                    } else {
                        let left = self.stack[fp + src].clone();
                        self.stack[fp + src] = Self::binary(0, left, Value::Int(1));
                    }
                }
                0x19 => {
                    // Dec
                    let src = code[ip] as usize;
                    ip += 1;

                    if let Value::Int(ref mut i) = self.stack[fp + src] {
                        *i -= 1;
                    } else {
                        let left = self.stack[fp + src].clone();
                        self.stack[fp + src] = Self::binary(1, left, Value::Int(1));
                    }
                }
                0x27 => {
                    // MatchEnum
                    let dest = code[ip] as usize;
                    let src = code[ip + 1] as usize;
                    let enum_idx = u16::from_le_bytes([code[ip + 2], code[ip + 3]]);
                    let tag = code[ip + 4];
                    ip += 5;

                    let matched = match &self.stack[fp + src] {
                        Value::EnumField {
                            const_idx: c,
                            tag: t,
                            ..
                        } => *c == enum_idx as usize && *t == tag,
                        _ => false,
                    };

                    self.stack[fp + dest] = Value::Bool(matched);
                }
                _ => return Err(Error::UnexpectedType),
            }
        }

        Ok(self.exit_value.clone())
    }

    pub fn is_truthy(val: Value) -> bool {
        Self::is_truthy_ref(&val)
    }

    pub fn is_truthy_ref(val: &Value) -> bool {
        match val {
            Value::Int(raw) => raw.abs() > 0,
            Value::Double(raw) => raw.abs() > 0.0,
            Value::Bool(raw) => *raw,
            Value::String(raw) => !raw.is_empty(),
            Value::Ref(_) | Value::Hash(_) => unreachable!(),
            Value::Fun(_) => true,
            Value::NativeFun(_) => true,
            Value::Object(items) => !items.borrow().is_empty(),
            Value::EnumDefinition(_) | Value::EnumField { .. } => true,
            Value::Bytes(bytes) => !bytes.borrow().is_empty(),
        }
    }

    #[inline(always)]
    pub fn binary_fast(
        op_type: u8,
        dest: usize,
        left_idx: usize,
        right_idx: usize,
        stack: &mut [Value],
    ) {
        if let (Value::Int(a), Value::Int(b)) = (&stack[left_idx], &stack[right_idx]) {
            let a = *a;
            let b = *b;
            let res = match op_type {
                0 => Value::Int(a + b),
                1 => Value::Int(a - b),
                2 => Value::Int(a * b),
                3 => Value::Int(if b != 0 {
                    a / b
                } else {
                    panic!("division by zero")
                }),
                4 => Value::Bool(a == b),
                5 => Value::Bool(a > b),
                6 => Value::Bool(a < b),
                7 => Value::Bool(a >= b),
                8 => Value::Bool(a <= b),
                9 => Value::Int(if b != 0 {
                    a % b
                } else {
                    panic!("modulo by zero")
                }),
                10 => Value::Int(a & b),
                11 => Value::Int(a | b),
                12 => Value::Int(a ^ b),
                13 => Value::Int(a << b),
                14 => Value::Int(a >> b),
                _ => panic!("UnexpectedType bin"),
            };
            stack[dest] = res;
        } else {
            let left = stack[left_idx].clone();
            let right = stack[right_idx].clone();
            stack[dest] = Self::binary(op_type as usize, left, right);
        }
    }

    pub fn binary(op_type: usize, left: Value, right: Value) -> Value {
        match op_type {
            0 => (left + right).expect("Unexpected error"),
            1 => (left - right).expect("Unexpected error"),
            2 => (left * right).expect("Unexpected error"),
            3 => (left / right).expect("Unexpected error"),
            4 => Value::Bool(left == right),
            5 => Value::Bool(left > right),
            6 => Value::Bool(left < right),
            7 => Value::Bool(left >= right),
            8 => Value::Bool(left <= right),
            9 => (left % right).expect("Unexpected error"),
            10 => (left & right).expect("Unexpected error"),
            11 => (left | right).expect("Unexpected error"),
            12 => (left ^ right).expect("Unexpected error"),
            13 => (left << right).expect("Unexpected error"),
            14 => (left >> right).expect("Unexpected error"),
            _ => panic!("UnexpectedType bin"),
        }
    }
}
