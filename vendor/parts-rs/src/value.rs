use crate::impl_binary_op;
use crate::impl_bitwise_op;
use crate::impl_compare_op;

use rustc_hash::{FxHashMap, FxHasher};
use std::cell::RefCell;
use std::fmt::Display;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
    pub arity: u8,
    pub frame_size: u8,
    pub code: Rc<[u8]>,
}

impl Function {
    pub fn new(arity: u8, frame_size: u8, code: Vec<u8>) -> Self {
        Self {
            arity,
            frame_size,
            code: code.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Int(i64),
    Double(f64),
    Bool(bool),
    String(Rc<String>),
    Ref(Rc<String>),
    Fun(Rc<Function>),
    NativeFun(NativeFunction),
    Object(Rc<RefCell<FxHashMap<u64, Value>>>),
    Hash(u64),
    EnumDefinition(u8),
    EnumField {
        const_idx: usize,
        tag: u8,
        args: Rc<[(u64, Value)]>,
    },
    Bytes(Rc<RefCell<Vec<u8>>>),
}

use std::hash::{Hash, Hasher};

impl Hash for Value {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Value::Int(i) => {
                state.write_u8(0);
                i.hash(state);
            }
            Value::Double(d) => {
                state.write_u8(1);
                state.write_u64(d.to_bits());
            }
            Value::Bool(b) => {
                state.write_u8(2);
                b.hash(state);
            }
            Value::String(s) => {
                state.write_u8(3);
                s.as_str().hash(state);
            }
            Value::Ref(s) => {
                state.write_u8(3);
                s.as_str().hash(state);
            }
            Value::Object(obj) => {
                state.write_u8(4);
                let ptr = Rc::as_ptr(obj) as usize;
                ptr.hash(state);
            }
            Value::Fun(fun) => {
                state.write_u8(4);
                let ptr = Rc::as_ptr(fun) as usize;
                ptr.hash(state);
            }
            Value::NativeFun(_) => {
                state.write_u8(4);
                let ptr = self as *const _ as usize;
                ptr.hash(state);
            }
            Value::Hash(v) => {
                state.write_u8(5);
                v.hash(state);
            }
            Value::EnumDefinition(count) => {
                state.write_u8(6);
                count.hash(state);
            }
            Value::EnumField {
                const_idx,
                tag,
                args,
            } => {
                state.write_u8(7);
                state.write_usize(*const_idx);
                state.write_u8(*tag);
                for (k, v) in args.iter() {
                    k.hash(state);
                    v.hash(state);
                }
            }
            Value::Bytes(bytes) => {
                state.write_u8(8);
                let ptr = Rc::as_ptr(bytes) as usize;
                ptr.hash(state);
            }
        }
    }
}

impl Eq for Value {}
use std::ops::Rem;

impl Rem for Value {
    type Output = Result<Value, String>;

    fn rem(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (Value::Int(a), Value::Int(b)) => {
                if b == 0 {
                    Err("remainder by zero".to_string())
                } else {
                    Ok(Value::Int(a % b))
                }
            }
            (l, r) => Err(format!("operation not supported - mod ({:?}, {:?})", l, r)),
        }
    }
}

impl Value {
    pub fn ok(val: impl Into<Value>) -> Value {
        Value::EnumField {
            const_idx: 0,
            tag: 0,
            args: Rc::from([(
                Value::String(std::rc::Rc::new("val".to_string())).get_hash(),
                val.into(),
            )]),
        }
    }

    pub fn err(err: impl Into<Value>) -> Value {
        Value::EnumField {
            const_idx: 0,
            tag: 1,
            args: Rc::from([(
                Value::String(std::rc::Rc::new("err".to_string())).get_hash(),
                err.into(),
            )]),
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(
            self,
            Value::EnumField {
                const_idx: 0,
                tag: 0,
                ..
            }
        )
    }

    pub fn is_err(&self) -> bool {
        matches!(
            self,
            Value::EnumField {
                const_idx: 0,
                tag: 1,
                ..
            }
        )
    }
    pub fn some(val: impl Into<Value>) -> Value {
        Value::EnumField {
            const_idx: 1,
            tag: 0,
            args: Rc::from([(
                Value::String(std::rc::Rc::new("val".to_string())).get_hash(),
                val.into(),
            )]),
        }
    }

    pub fn none() -> Value {
        Value::EnumField {
            const_idx: 1,
            tag: 1,
            args: Rc::from([]),
        }
    }
    pub fn is_some(&self) -> bool {
        matches!(
            self,
            Value::EnumField {
                const_idx: 1,
                tag: 0,
                ..
            }
        )
    }

    pub fn is_none(&self) -> bool {
        matches!(
            self,
            Value::EnumField {
                const_idx: 1,
                tag: 1,
                ..
            }
        )
    }
    pub fn bytes(data: Vec<u8>) -> Value {
        Value::Bytes(Rc::new(RefCell::new(data)))
    }

    pub fn call(&self, args: Vec<Value>, constants: Vec<Value>) -> Result<Option<Value>, String> {
        match self {
            Value::Fun(fun) => {
                if args.len() != fun.arity as usize {
                    return Err(format!(
                        "Expected {} arguments, got {}",
                        fun.arity,
                        args.len()
                    ));
                }

                use crate::vm::{Frame, VM};

                let mut vm = VM::new(vec![], constants);

                for (i, arg) in args.into_iter().enumerate() {
                    vm.stack[i] = arg;
                }

                vm.run_with_frame(Frame {
                    pointer: 0,
                    ip: 0,
                    bytecode: Rc::clone(&fun.code),
                    return_reg: 0,
                })
                .map_err(|e| format!("VM Error: {:?}", e))
            }
            Value::NativeFun(native) => {
                if args.len() != native.arity as usize {
                    return Err(format!(
                        "Expected {} arguments, got {}",
                        native.arity,
                        args.len()
                    ));
                }
                (native.call)(&args).map(Some)
            }
            _ => Err("Value is not callable".to_string()),
        }
    }

    pub fn get_hash(&self) -> u64 {
        match self {
            Value::Int(i) => *i as u64,
            Value::String(s) | Value::Ref(s) => {
                let mut hasher = FxHasher::default();
                s.hash(&mut hasher);
                hasher.finish()
            }
            Value::Bool(b) => {
                if *b {
                    1
                } else {
                    0
                }
            }
            _ => {
                let mut hasher = FxHasher::default();
                self.hash(&mut hasher);
                hasher.finish()
            }
        }
    }

    pub fn encode(&self, buffer: &mut Vec<u8>) {
        match self {
            Value::Int(i) => {
                buffer.push(0);
                buffer.extend_from_slice(&i.to_le_bytes());
            }
            Value::Double(d) => {
                buffer.push(1);
                buffer.extend_from_slice(&d.to_le_bytes());
            }
            Value::Bool(b) => {
                buffer.push(2);
                buffer.push(*b as u8);
            }
            Value::String(s) => {
                buffer.push(3);
                buffer.extend_from_slice(&(s.len() as u64).to_le_bytes());
                buffer.extend_from_slice(s.as_bytes());
            }
            Value::Fun(fun) => {
                buffer.push(4);
                buffer.push(fun.arity);
                buffer.push(fun.frame_size);
                buffer.extend_from_slice(&(fun.code.len() as u64).to_le_bytes());
                buffer.extend_from_slice(&fun.code);
            }
            Value::Object(ref_cell) => {
                buffer.push(5);
                let obj = ref_cell.borrow();
                buffer.push(obj.len() as u8);

                for (key, value) in obj.iter() {
                    buffer.push(6);
                    buffer.extend_from_slice(&key.to_le_bytes());
                    value.encode(buffer);
                }
            }
            Value::Hash(h) => {
                buffer.push(6);
                buffer.extend_from_slice(&h.to_le_bytes());
            }
            Value::NativeFun(f) => {
                buffer.push(7);
                let name_bytes = f.name.as_bytes();

                buffer.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
                buffer.extend_from_slice(name_bytes);
            }
            Value::EnumDefinition(idx) => {
                buffer.push(8);
                buffer.push(*idx);
            }
            Value::EnumField {
                const_idx,
                tag,
                args,
            } => {
                buffer.push(9);
                buffer.extend_from_slice(&(*const_idx as u16).to_le_bytes());
                buffer.push(*tag);

                buffer.push(args.len() as u8);

                for (k, v) in args.iter() {
                    buffer.extend_from_slice(&k.to_le_bytes());
                    v.encode(buffer);
                }
            }
            Value::Bytes(bytes) => {
                buffer.push(10);
                let slice = bytes.borrow();
                buffer.extend_from_slice(&(slice.len() as u64).to_le_bytes());
                buffer.extend_from_slice(&slice);
            }
            _ => unreachable!(),
        }
    }

    pub fn decode(raw: &[u8], once: bool, starting_idx: usize) -> (Vec<Value>, usize) {
        let mut values = vec![];
        let mut idx = starting_idx;

        while idx < raw.len() {
            match raw[idx] {
                0 => {
                    idx += 1;
                    let bytes: [u8; 8] = raw[idx..idx + 8].try_into().unwrap();
                    idx += 8;
                    values.push(Value::Int(i64::from_le_bytes(bytes)));
                }
                1 => {
                    idx += 1;
                    let bytes: [u8; 8] = raw[idx..idx + 8].try_into().unwrap();
                    idx += 8;
                    values.push(Value::Double(f64::from_le_bytes(bytes)));
                }
                2 => {
                    idx += 1;
                    let val = raw[idx];
                    idx += 1;
                    values.push(Value::Bool(val != 0));
                }
                3 => {
                    idx += 1;
                    let len_bytes: [u8; 8] = raw[idx..idx + 8].try_into().unwrap();
                    // Zgodnie z formatem zapisu, wczytujemy u64, potem as usize
                    let len = u64::from_le_bytes(len_bytes) as usize;
                    idx += 8;

                    let raw_s = &raw[idx..idx + len];
                    idx += len;
                    values.push(Value::String(
                        String::from_utf8(raw_s.to_vec()).unwrap().into(),
                    ));
                }
                4 => {
                    idx += 1;
                    let arity = raw[idx];
                    idx += 1;
                    let frame_size = raw[idx];
                    idx += 1;

                    let len_bytes: [u8; 8] = raw[idx..idx + 8].try_into().unwrap();
                    let len = u64::from_le_bytes(len_bytes) as usize;
                    idx += 8;

                    let body = &raw[idx..idx + len];
                    idx += len;

                    values.push(Value::Fun(Rc::new(Function {
                        arity,
                        frame_size,
                        code: body.to_vec().into(),
                    })));
                }
                5 => {
                    idx += 1;
                    let count = raw[idx];
                    idx += 1;

                    let mut entries = vec![];

                    for _ in 0..count * 2 {
                        let (mut temp_vals, temp_idx) = Value::decode(raw, true, idx);
                        idx = temp_idx;
                        entries.append(&mut temp_vals);
                    }

                    let mut obj: FxHashMap<u64, Value> = FxHashMap::default();

                    for chunk in entries.chunks_exact(2) {
                        let key_raw = match chunk[0] {
                            Value::Hash(h) => h,
                            _ => unreachable!(),
                        };
                        obj.insert(key_raw, chunk[1].clone());
                    }

                    values.push(Value::Object(Rc::new(RefCell::new(obj))));
                }
                6 => {
                    idx += 1;
                    let bytes: [u8; 8] = raw[idx..idx + 8].try_into().unwrap();
                    idx += 8;
                    values.push(Value::Hash(u64::from_le_bytes(bytes)));
                }
                7 => {
                    idx += 1;
                    let len_bytes: [u8; 4] = raw[idx..idx + 4].try_into().unwrap();
                    idx += 4;
                    let len = u32::from_le_bytes(len_bytes) as usize;

                    let name_bytes = &raw[idx..idx + len];
                    idx += len;

                    let std = crate::value::StdDefinition::get_core();
                    let native = std
                        .functions
                        .into_iter()
                        .find(|f| f.name.as_bytes() == name_bytes)
                        .unwrap_or_else(|| panic!("Missing native function"));

                    values.push(Value::NativeFun(native));
                }
                8 => {
                    idx += 1;

                    let const_idx = raw[idx];
                    idx += 1;

                    values.push(Value::EnumDefinition(const_idx));
                }
                9 => {
                    idx += 1;
                    let const_bytes: [u8; 2] = raw[idx..idx + 2].try_into().unwrap();
                    let const_idx = u16::from_le_bytes(const_bytes) as usize;
                    idx += 2;
                    let tag = raw[idx];
                    idx += 1;
                    let len = raw[idx];
                    idx += 1;
                    let mut args = Vec::with_capacity(len as usize);
                    for _ in 0..len {
                        let bytes: [u8; 8] = raw[idx..idx + 8].try_into().unwrap();
                        idx += 8;
                        let (vals, next_idx) = Value::decode(raw, true, idx);
                        idx = next_idx;
                        let val = vals.into_iter().next().unwrap_or(Value::Int(0));
                        args.push((u64::from_le_bytes(bytes), val));
                    }
                    values.push(Value::EnumField {
                        const_idx,
                        tag,
                        args: args.into(),
                    });
                }
                10 => {
                    idx += 1;
                    let len_bytes: [u8; 8] = raw[idx..idx + 8].try_into().unwrap();
                    let len = u64::from_le_bytes(len_bytes) as usize;
                    idx += 8;

                    let data = &raw[idx..idx + len];
                    idx += len;

                    values.push(Value::Bytes(Rc::new(RefCell::new(data.to_vec()))));
                }
                _ => panic!("Unexpected value type at index {}", idx),
            }

            if once {
                break;
            }
        }
        (values, idx)
    }
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}

impl From<f64> for Value {
    fn from(v: f64) -> Self {
        Value::Double(v)
    }
}

impl From<bool> for Value {
    fn from(v: bool) -> Self {
        Value::Bool(v)
    }
}

impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::String(v.into())
    }
}

impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::String(v.to_string().into())
    }
}

impl PartialOrd for Value {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => Some(a.cmp(b)),
            (Value::Int(a), Value::Double(b)) => (*a as f64).partial_cmp(b),
            (Value::Double(a), Value::Int(b)) => a.partial_cmp(&(*b as f64)),
            (Value::Double(a), Value::Double(b)) => a.partial_cmp(b),
            (Value::String(a), Value::String(b)) => a.partial_cmp(b),
            _ => Some(std::cmp::Ordering::Equal),
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(i) => write!(f, "{}", i),
            Value::Double(d) => write!(f, "{}", d),
            Value::Bool(b) => write!(f, "{}", b),
            Value::String(s) => write!(f, "{}", s), // Quoted for clarity
            Value::Ref(r) => write!(f, "&{}", r),   // Prefixed with & to show it's a ref
            Value::Fun(fun) => write!(f, "<function/{}>", fun.arity),
            Value::NativeFun(_) => write!(f, "<native fun>"),
            Value::Object(obj) => write!(f, "<object: {} keys>", obj.borrow().len()),
            Value::Hash(h) => write!(f, "#{}", h),
            Value::EnumDefinition(idx) => write!(f, "<enum def #{}>", idx),
            Value::EnumField {
                const_idx,
                tag,
                args,
            } => {
                if *const_idx == 0 && *tag == 0 {
                    if let Some((_, val)) = args.first() {
                        write!(f, "Ok({})", val)
                    } else {
                        write!(f, "Ok")
                    }
                } else if *const_idx == 0 && *tag == 1 {
                    if let Some((_, err)) = args.first() {
                        write!(f, "Err({})", err)
                    } else {
                        write!(f, "Err")
                    }
                } else if *const_idx == 1 && *tag == 0 {
                    if let Some((_, val)) = args.first() {
                        write!(f, "Some({})", val)
                    } else {
                        write!(f, "Some")
                    }
                } else if *const_idx == 1 && *tag == 1 {
                    write!(f, "None")
                } else {
                    write!(f, "EnumField({}, tag: {}, args: [", const_idx, tag)?;
                    for (i, (_, val)) in args.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", val)?;
                    }
                    write!(f, "])")
                }
            }
            Value::Bytes(bytes) => {
                let b = bytes.borrow();
                write!(f, "<bytes: {} bytes>", b.len())
            }
        }
    }
}

impl_binary_op!(Add, add, +);
impl_binary_op!(Sub, sub, -);
impl_binary_op!(Mul, mul, *);
impl_binary_op!(Div, div, /);

impl_bitwise_op!(BitAnd, bitand, &);
impl_bitwise_op!(BitOr, bitor, |);
impl_bitwise_op!(BitXor, bitxor, ^);
impl_bitwise_op!(Shl, shl, <<);
impl_bitwise_op!(Shr, shr, >>);

impl_compare_op!(op_gt, >);
impl_compare_op!(op_lt, <);

pub trait FromValue: Sized {
    fn from_value(val: &Value) -> Result<Self, String>;
}

pub trait IntoValue {
    fn into_value(self) -> Value;
}

impl FromValue for i64 {
    fn from_value(val: &Value) -> Result<i64, String> {
        match val {
            Value::Int(i) => Ok(*i),
            Value::Double(d) => Ok(*d as i64),
            _ => Err("Expected an integer".into()),
        }
    }
}

impl FromValue for i32 {
    fn from_value(val: &Value) -> Result<i32, String> {
        match val {
            Value::Int(i) => Ok(*i as i32),
            Value::Double(d) => Ok(*d as i32),
            _ => Err("Expected an integer".into()),
        }
    }
}

impl FromValue for String {
    fn from_value(val: &Value) -> Result<String, String> {
        match val {
            Value::String(s) | Value::Ref(s) => Ok(s.to_string()),
            _ => Err("Expected a string".into()),
        }
    }
}

impl<T: FromValue> FromValue for Vec<T> {
    fn from_value(val: &Value) -> Result<Self, String> {
        if let Value::Object(obj_ref) = val {
            let obj = obj_ref.borrow();

            let len = obj.len();

            let mut vec = Vec::with_capacity(len);
            for i in 0..(len as i64) {
                let int_key = Value::Int(i).get_hash();

                if let Some(item_val) = obj.get(&int_key) {
                    vec.push(T::from_value(item_val)?);
                } else {
                    return Err(format!("Expected property at key: {}, got none", i));
                }
            }

            return Ok(vec);
        }
        Err("Expected an object".into())
    }
}

impl IntoValue for String {
    fn into_value(self) -> Value {
        Value::String(self.into())
    }
}

impl IntoValue for bool {
    fn into_value(self) -> Value {
        Value::Bool(self)
    }
}

impl<T: FromValue> FromValue for Option<T> {
    fn from_value(val: &Value) -> Result<Option<T>, String> {
        match val {
            Value::Bool(false) => Ok(None),
            _ => T::from_value(val).map(Some),
        }
    }
}

impl IntoValue for i64 {
    fn into_value(self) -> Value {
        Value::Int(self)
    }
}

impl IntoValue for i32 {
    fn into_value(self) -> Value {
        Value::Int(self as i64)
    }
}

impl IntoValue for f64 {
    fn into_value(self) -> Value {
        Value::Double(self)
    }
}

impl<T: IntoValue> IntoValue for Option<T> {
    fn into_value(self) -> Value {
        match self {
            Some(val) => val.into_value(),
            None => Value::Bool(false),
        }
    }
}

impl<T: IntoValue> IntoValue for Vec<T> {
    fn into_value(self) -> Value {
        let mut map = FxHashMap::default();

        for (i, item) in self.into_iter().enumerate() {
            map.insert(Value::Int(i as i64).get_hash(), item.into_value());
        }

        Value::Object(Rc::new(RefCell::new(map)))
    }
}

impl FromValue for bool {
    fn from_value(val: &Value) -> Result<Self, String> {
        match val {
            Value::Bool(b) => Ok(*b),
            _ => Err("Expected a boolen".into()),
        }
    }
}

impl FromValue for u32 {
    fn from_value(val: &Value) -> Result<Self, String> {
        match val {
            Value::Int(i) => Ok(*i as u32),
            Value::Double(d) => Ok(*d as u32),
            _ => Err("Expected an integer".into()),
        }
    }
}

impl FromValue for u8 {
    fn from_value(val: &Value) -> Result<Self, String> {
        match val {
            Value::Int(i) => Ok(*i as u8),
            Value::Double(d) => Ok(*d as u8),
            _ => Err("Expected an integer".into()),
        }
    }
}

impl FromValue for Value {
    fn from_value(val: &Value) -> Result<Self, String> {
        Ok(val.clone())
    }
}

impl IntoValue for Value {
    fn into_value(self) -> Value {
        self
    }
}

pub use parts_macros::{FromPartsObject, IntoPartsObject, parts_native};

// Native

pub type NativeFn = std::sync::Arc<dyn Fn(&[Value]) -> Result<Value, String> + Send + Sync>;

#[derive(Clone)]
pub struct NativeFunction {
    pub name: &'static str,
    pub arity: u8,
    pub call: NativeFn,
}

impl PartialEq for NativeFunction {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.arity == other.arity
    }
}

impl Eq for NativeFunction {}

impl std::fmt::Debug for NativeFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "NativeFunction - {}", self.name)
    }
}

#[derive(Clone)]
pub struct StdDefinition {
    pub functions: Vec<NativeFunction>,
}

impl StdDefinition {
    pub fn get_core() -> Self {
        use crate::std::StdModule;

        StdDefinition {
            functions: StdModule::get_core().functions,
        }
    }
}
