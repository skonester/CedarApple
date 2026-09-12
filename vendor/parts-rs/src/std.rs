use rustc_hash::FxHashMap;
use std::{
    cell::RefCell,
    io::{Read, Write},
    net::TcpStream,
    process::Command,
    rc::Rc,
    sync::Mutex,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
#[cfg(unix)]
use std::os::unix::net::UnixStream;

enum SocketStream {
    Tcp(TcpStream),
    Tls(native_tls::TlsStream<TcpStream>),
    #[cfg(unix)]
    Unix(UnixStream),
}

static NEXT_SOCKET_ID: AtomicU64 = AtomicU64::new(1);
static SOCKETS: std::sync::LazyLock<Mutex<FxHashMap<u64, SocketStream>>> =
    std::sync::LazyLock::new(|| Mutex::new(FxHashMap::default()));

fn get_sockets() -> &'static Mutex<FxHashMap<u64, SocketStream>> {
    &SOCKETS
}
use crate::value::{NativeFunction, Value};
use parts_macros::native_function;

#[native_function]
pub fn println(args: &[Value]) -> Result<Value, String> {
    for arg in args {
        print!("{}", arg)
    }
    println!();

    Ok(Value::Bool(true))
}

#[native_function]
pub fn print(args: &[Value]) -> Result<Value, String> {
    for arg in args {
        print!("{}", arg)
    }

    Ok(Value::Bool(true))
}

#[native_function]
pub fn __timestamp() -> Result<Value, String> {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros();

    Ok(Value::Int(ts as i64))
}

#[native_function]
pub fn __iter_of(val: Value) -> Result<Value, String> {
    return match val {
        Value::String(s) => {
            let mut hash_map = FxHashMap::default();

            hash_map.insert(
                Value::String("data".to_string().into()).get_hash(),
                Value::String(s.clone()),
            );

            hash_map.insert(
                Value::String("index".to_string().into()).get_hash(),
                Value::Int(0),
            );

            Ok(Value::Object(Rc::new(RefCell::new(hash_map))))
        }
        Value::Object(map) => {
            let mut hash_map = FxHashMap::default();

            hash_map.insert(
                Value::String("data".to_string().into()).get_hash(),
                Value::Object(map.clone()),
            );

            hash_map.insert(
                Value::String("index".to_string().into()).get_hash(),
                Value::Int(0),
            );

            Ok(Value::Object(Rc::new(RefCell::new(hash_map))))
        }
        _ => Err("UnexpectedType, only object/strings".to_string()),
    };
}

#[native_function]
pub fn __get_next(obj: Value) -> Result<Value, String> {
    if let Value::Object(obj_ref) = obj {
        let mut map = obj_ref.borrow_mut();

        let data_hash = Value::String("data".to_string().into()).get_hash();
        let index_hash = Value::String("index".to_string().into()).get_hash();

        let data = map.get(&data_hash).cloned().ok_or("Missing data")?;
        let index = if let Some(Value::Int(i)) = map.get(&index_hash) {
            *i
        } else {
            0
        };

        match data {
            Value::String(s) => {
                if s.len() < index as usize {
                    return Ok(Value::Bool(false));
                }

                let ch = s.chars().nth(index as usize).expect("Shouldn't be empty");

                map.insert(index_hash, Value::Int(index + 1));

                return Ok(Value::String(ch.to_string().into()));
            }
            Value::Object(items_ref) => {
                let items = items_ref.borrow();

                let entry_hash = items.keys().nth(index as usize);

                if entry_hash.is_none() {
                    return Ok(Value::Bool(false));
                }

                if let Some(val) = items.get(entry_hash.unwrap()) {
                    map.insert(index_hash, Value::Int(index + 1));

                    return Ok(val.clone());
                }
            }
            _ => (),
        }

        Ok(Value::Bool(false))
    } else {
        Err("UnexpectedType, expected object".to_string())
    }
}

#[native_function]
pub fn __has_next(obj: Value) -> Result<Value, String> {
    if let Value::Object(obj_ref) = obj {
        let map = obj_ref.borrow();

        let data_hash = Value::String("data".to_string().into()).get_hash();
        let index_hash = Value::String("index".to_string().into()).get_hash();

        let data = map.get(&data_hash).cloned().ok_or("Missing data")?;
        let index = if let Some(Value::Int(i)) = map.get(&index_hash) {
            *i
        } else {
            0
        };

        match data {
            Value::String(s) => return Ok(Value::Bool(s.len() > index as usize)),
            Value::Object(items_ref) => {
                let items = items_ref.borrow();

                let entry_hash = items.keys().nth(index as usize);

                if entry_hash.is_none() {
                    return Ok(Value::Bool(false));
                }

                return Ok(Value::Bool(true));
            }
            _ => (),
        }

        Ok(Value::Bool(false))
    } else {
        Err("UnexpectedType, expected object".to_string())
    }
}

#[cfg(unix)]
fn get_os_random_bytes(buffer: &mut [u8]) -> std::io::Result<()> {
    use std::fs::File;
    use std::io::Read;

    let mut file = File::open("/dev/urandom")?;
    file.read_exact(buffer)?;
    Ok(())
}

#[cfg(windows)]
fn get_os_random_bytes(buffer: &mut [u8]) -> std::io::Result<()> {
    #[link(name = "bcrypt")]
    unsafe extern "system" {
        fn BCryptGenRandom(
            h_algorithm: *mut core::ffi::c_void,
            pb_buffer: *mut u8,
            cb_buffer: u32,
            dw_flags: u32,
        ) -> i32;
    }

    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x00000002;

    let status = unsafe {
        BCryptGenRandom(
            core::ptr::null_mut(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };

    if status == 0 {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "BCryptGenRandom failed",
        ))
    }
}

static RAND_STATE: Mutex<[u64; 2]> = Mutex::new([0, 0]);

#[native_function]
pub fn __rand() -> Result<Value, String> {
    let mut state = RAND_STATE.lock().unwrap();

    if state[0] == 0 && state[1] == 0 {
        let mut buffer = [0u8; 8];

        let seed = match get_os_random_bytes(&mut buffer) {
            Ok(_) => u64::from_ne_bytes(buffer),
            Err(e) => return Err(format!("Failed to fetch OS entropy: {}", e)),
        };

        let negated = seed.wrapping_neg();

        fn fmix64(mut k: u64) -> u64 {
            k ^= k >> 33;
            k = k.wrapping_mul(0xff51afd7ed558ccd);

            k ^= k >> 33;
            k = k.wrapping_mul(0xc4ceb9fe1a85ec53);

            k ^= k >> 33;

            k
        }

        state[0] = fmix64(seed);
        state[0] = fmix64(negated);
    }

    let mut s1 = state[0];
    let s0 = state[1];

    state[0] = s0;

    s1 ^= s1 << 23;
    state[1] = s1 ^ s0 ^ (s1 >> 18) ^ (s0 >> 5);

    let mantissa = state[0].wrapping_add(state[1]) >> 11;

    let result = (mantissa as f64) * (1.0 / (1u64 << 53) as f64);

    Ok(Value::Double(result))
}

#[native_function(arity = 1)]
pub fn __exec(args: &[Value]) -> Result<Value, String> {
    let raw_cmd = match args.first() {
        Some(Value::String(s)) => s,
        _ => return Ok(Value::err("Expected string command for exec")),
    };

    let mut cmd = Command::new(raw_cmd.as_str());

    for arg in args.iter().skip(1) {
        let raw_arg = match arg {
            Value::String(s) => s,
            _ => return Ok(Value::err("Expected string argument for exec")),
        };

        cmd.arg(raw_arg.as_str());
    }

    match cmd.output() {
        Ok(output) => {
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout);
                Ok(Value::ok(Value::String(s.to_string().into())))
            } else {
                let err_s = String::from_utf8_lossy(&output.stderr);
                let err_msg = if !err_s.is_empty() {
                    err_s.to_string()
                } else {
                    format!(
                        "Command exited with status code: {:?}",
                        output.status.code()
                    )
                };
                Ok(Value::err(Value::String(err_msg.into())))
            }
        }
        Err(e) => Ok(Value::err(Value::String(
            format!("Command execution failed: {}", e).into(),
        ))),
    }
}

#[native_function]
pub fn __env(name: Value) -> Result<Value, String> {
    let name = match name {
        Value::String(s) => s,
        _ => return Ok(Value::err("Expected string argument for env")),
    };

    match std::env::var(name.as_str()) {
        Ok(val) => Ok(Value::ok(Value::String(val.into()))),
        Err(e) => Ok(Value::err(Value::String(
            format!("Environment variable not found: {}", e).into(),
        ))),
    }
}

#[native_function]
pub fn __joinStr(arg_one: Value, arg_two: Value) -> Result<Value, String> {
    let s1 = match arg_one {
        Value::String(s) => s,
        _ => return Err("Expected string lol".to_string()),
    };

    let s2 = match arg_two {
        Value::String(s) => s,
        _ => return Err("Expected string lol".to_string()),
    };

    Ok(Value::String(format!("{}{}", s1, s2).into()))
}

#[native_function]
pub fn __math_sin(val: Value) -> Result<Value, String> {
    match val {
        Value::Double(d) => Ok(Value::Double(d.sin())),
        Value::Int(i) => Ok(Value::Double((i as f64).sin())),
        _ => Err("Expected number".to_string()),
    }
}

#[native_function]
pub fn __math_cos(val: Value) -> Result<Value, String> {
    match val {
        Value::Double(d) => Ok(Value::Double(d.cos())),
        Value::Int(i) => Ok(Value::Double((i as f64).cos())),
        _ => Err("Expected number".to_string()),
    }
}

#[native_function]
pub fn __math_floor(val: Value) -> Result<Value, String> {
    if let Value::Double(d) = val {
        Ok(Value::Int(d.floor() as i64))
    } else if let Value::Int(i) = val {
        Ok(Value::Int(i))
    } else {
        Err("Expected double or int for math.floor".into())
    }
}

#[native_function]
pub fn __math_pow(exp: Value, pow: Value) -> Result<Value, String> {
    let i1 = match exp {
        Value::Int(i) => i,
        _ => return Err("Expected int lol".to_string()),
    };

    let i2 = match pow {
        Value::Int(i) => i,
        _ => return Err("Expected int lol".to_string()),
    };

    return Ok(Value::Int(i1.pow(i2 as u32)));
}

#[native_function]
pub fn __str_len(val: Value) -> Result<Value, String> {
    match val {
        Value::String(s) => Ok(Value::Int(s.len() as i64)),
        _ => Err("Expected string".to_string()),
    }
}

fn raw_str_strip_prefix(str: Value, prefix: Value) -> Result<Value, String> {
    let s1 = match str {
        Value::String(s) => s,
        _ => return Ok(Value::none()),
    };

    let s2 = match prefix {
        Value::String(s) => s,
        _ => return Ok(Value::none()),
    };

    if s1.starts_with(&*s2) {
        Ok(Value::some(Value::String(
            s1.strip_prefix(&*s2).unwrap().to_string().into(),
        )))
    } else {
        Ok(Value::none())
    }
}

#[native_function]
pub fn __str_strip_prefix(str: Value, prefix: Value) -> Result<Value, String> {
    raw_str_strip_prefix(str, prefix)
}

#[native_function]
pub fn strip_prefix(str: Value, prefix: Value) -> Result<Value, String> {
    raw_str_strip_prefix(str, prefix)
}

fn raw_str_strip_suffix(str: Value, suffix: Value) -> Result<Value, String> {
    let s1 = match str {
        Value::String(s) => s,
        _ => return Ok(Value::none()),
    };

    let s2 = match suffix {
        Value::String(s) => s,
        _ => return Ok(Value::none()),
    };

    if s1.ends_with(&*s2) {
        Ok(Value::some(Value::String(
            s1.strip_suffix(&*s2).unwrap().to_string().into(),
        )))
    } else {
        Ok(Value::none())
    }
}

#[native_function]
pub fn __str_strip_suffix(str: Value, suffix: Value) -> Result<Value, String> {
    raw_str_strip_suffix(str, suffix)
}

#[native_function]
pub fn strip_suffix(str: Value, suffix: Value) -> Result<Value, String> {
    raw_str_strip_suffix(str, suffix)
}

#[native_function]
pub fn __str_upper(val: Value) -> Result<Value, String> {
    match val {
        Value::String(s) => Ok(Value::String(s.to_uppercase().into())),
        _ => Err("Expected string".to_string()),
    }
}

#[native_function]
pub fn __str_lower(val: Value) -> Result<Value, String> {
    match val {
        Value::String(s) => Ok(Value::String(s.to_lowercase().into())),
        _ => Err("Expected string".to_string()),
    }
}

#[native_function]
pub fn __byte_rotate_left(left: Value, right: Value, mask: Value) -> Result<Value, String> {
    let i1 = match left {
        Value::Int(i) => i,
        _ => return Err("Expected int lol".to_string()),
    };

    let i2 = match right {
        Value::Int(i) => i,
        _ => return Err("Expected int lol".to_string()),
    };

    let mask_raw = match mask {
        Value::Int(i) => i,
        _ => return Err("Expected int lol".to_string()),
    };

    let x = match mask_raw {
        0 => (i1 as u8).rotate_left(i2 as u32) as i64,
        1 => (i1 as u16).rotate_left(i2 as u32) as i64,
        2 => (i1 as u32).rotate_left(i2 as u32) as i64,
        3 => (i1 as u64).rotate_left(i2 as u32) as i64,
        _ => return Err("Value out of expected range".to_string()),
    };

    return Ok(Value::Int(x));
}

fn raw_object_len(obj: Value) -> Result<Value, String> {
    if let Value::Object(obj_ref) = &obj {
        Ok(Value::Int(obj_ref.borrow().len() as i64))
    } else if let Value::Bytes(bytes_ref) = &obj {
        Ok(Value::Int(bytes_ref.borrow().len() as i64))
    } else if let Value::String(s) = &obj {
        Ok(Value::Int(s.len() as i64))
    } else {
        Err("UnexpectedType, expected object, bytes or string".to_string())
    }
}

#[native_function]
pub fn __object_len(obj: Value) -> Result<Value, String> {
    raw_object_len(obj)
}

#[native_function]
pub fn len(obj: Value) -> Result<Value, String> {
    raw_object_len(obj)
}

#[native_function]
pub fn __str_byte_at(str: Value, idx: Value) -> Result<Value, String> {
    let s = match str {
        Value::String(s) => s,
        _ => return Ok(Value::err("Expected string for byte_at")),
    };

    let index = match idx {
        Value::Int(i) => i,
        _ => return Ok(Value::err("Expected integer for byte_at index")),
    };

    if index < 0 {
        return Ok(Value::err("Negative index in byte_at"));
    }

    if let Some(ch) = s.chars().nth(index as usize) {
        Ok(Value::ok(Value::Int(ch as i64)))
    } else {
        Ok(Value::err("Index out of bounds in byte_at"))
    }
}

fn result_is_ok(val: Value) -> Result<Value, String> {
    Ok(Value::Bool(val.is_ok()))
}

fn result_is_err(val: Value) -> Result<Value, String> {
    Ok(Value::Bool(val.is_err()))
}

fn option_is_some(val: Value) -> Result<Value, String> {
    Ok(Value::Bool(val.is_some()))
}

fn option_is_none(val: Value) -> Result<Value, String> {
    Ok(Value::Bool(val.is_none()))
}

fn unwrap_val(val: Value) -> Result<Value, String> {
    match val {
        Value::EnumField {
            const_idx: 0,
            tag: 0,
            args,
        } => {
            if let Some((_, v)) = args.first() {
                Ok(v.clone())
            } else {
                Err("Result::Ok had no value".to_string())
            }
        }
        Value::EnumField {
            const_idx: 0,
            tag: 1,
            args,
        } => {
            let err_msg = args
                .first()
                .map(|(_, v)| v.to_string())
                .unwrap_or_else(|| "Error".to_string());
            Err(format!("called `unwrap()` on an `Err` value: {}", err_msg))
        }
        Value::EnumField {
            const_idx: 1,
            tag: 0,
            args,
        } => {
            if let Some((_, v)) = args.first() {
                Ok(v.clone())
            } else {
                Err("Option::Some had no value".to_string())
            }
        }
        Value::EnumField {
            const_idx: 1,
            tag: 1,
            ..
        } => Err("called `unwrap()` on a `None` value".to_string()),
        _ => Err("called `unwrap()` on non-Result/non-Option value".to_string()),
    }
}

fn unwrap_or_val(val: Value, default: Value) -> Result<Value, String> {
    match val {
        Value::EnumField {
            const_idx: 0,
            tag: 0,
            args,
        }
        | Value::EnumField {
            const_idx: 1,
            tag: 0,
            args,
        } => {
            if let Some((_, v)) = args.first() {
                Ok(v.clone())
            } else {
                Ok(default)
            }
        }
        _ => Ok(default),
    }
}

fn expect_val(val: Value, msg: Value) -> Result<Value, String> {
    let msg_str = match &msg {
        Value::String(s) => s.to_string(),
        _ => format!("{}", msg),
    };
    match val {
        Value::EnumField {
            const_idx: 0,
            tag: 0,
            args,
        }
        | Value::EnumField {
            const_idx: 1,
            tag: 0,
            args,
        } => {
            if let Some((_, v)) = args.first() {
                Ok(v.clone())
            } else {
                Err(format!("{}: value was empty", msg_str))
            }
        }
        Value::EnumField {
            const_idx: 0,
            tag: 1,
            args,
        } => {
            let err_msg = args
                .first()
                .map(|(_, v)| v.to_string())
                .unwrap_or_else(|| "Error".to_string());
            Err(format!("{}: {}", msg_str, err_msg))
        }
        Value::EnumField {
            const_idx: 1,
            tag: 1,
            ..
        } => Err(format!("{}: None", msg_str)),
        _ => Err(format!(
            "{}: called expect on non-Result/non-Option value",
            msg_str
        )),
    }
}

fn unwrap_err_val(val: Value) -> Result<Value, String> {
    match val {
        Value::EnumField {
            const_idx: 0,
            tag: 1,
            args,
        } => {
            if let Some((_, v)) = args.first() {
                Ok(v.clone())
            } else {
                Err("Result::Err had no error content".to_string())
            }
        }
        Value::EnumField {
            const_idx: 0,
            tag: 0,
            args,
        } => {
            let val_str = args
                .first()
                .map(|(_, v)| v.to_string())
                .unwrap_or_else(|| "".to_string());
            Err(format!(
                "called `unwrap_err()` on an `Ok` value: {}",
                val_str
            ))
        }
        _ => Err("called `unwrap_err()` on non-Result value".to_string()),
    }
}

fn option_ok_or(val: Value, err: Value) -> Result<Value, String> {
    match val {
        Value::EnumField {
            const_idx: 1,
            tag: 0,
            args,
        } => {
            if let Some((_, v)) = args.first() {
                Ok(Value::ok(v.clone()))
            } else {
                Ok(Value::err(err))
            }
        }
        _ => Ok(Value::err(err)),
    }
}

fn result_to_ok(val: Value) -> Result<Value, String> {
    match val {
        Value::EnumField {
            const_idx: 0,
            tag: 0,
            args,
        } => {
            if let Some((_, v)) = args.first() {
                Ok(Value::some(v.clone()))
            } else {
                Ok(Value::none())
            }
        }
        Value::EnumField {
            const_idx: 0,
            tag: 1,
            ..
        } => Ok(Value::none()),
        _ => Ok(Value::ok(val)),
    }
}

fn result_to_err(val: Value) -> Result<Value, String> {
    match val {
        Value::EnumField {
            const_idx: 0,
            tag: 1,
            args,
        } => {
            if let Some((_, e)) = args.first() {
                Ok(Value::some(e.clone()))
            } else {
                Ok(Value::none())
            }
        }
        Value::EnumField {
            const_idx: 0,
            tag: 0,
            ..
        } => Ok(Value::none()),
        _ => Ok(Value::err(val)),
    }
}

#[native_function]
pub fn __result_is_ok(val: Value) -> Result<Value, String> {
    result_is_ok(val)
}

#[native_function]
pub fn is_ok(val: Value) -> Result<Value, String> {
    result_is_ok(val)
}

#[native_function]
pub fn __result_is_err(val: Value) -> Result<Value, String> {
    result_is_err(val)
}

#[native_function]
pub fn is_err(val: Value) -> Result<Value, String> {
    result_is_err(val)
}

#[native_function]
pub fn __option_is_some(val: Value) -> Result<Value, String> {
    option_is_some(val)
}

#[native_function]
pub fn is_some(val: Value) -> Result<Value, String> {
    option_is_some(val)
}

#[native_function]
pub fn __option_is_none(val: Value) -> Result<Value, String> {
    option_is_none(val)
}

#[native_function]
pub fn is_none(val: Value) -> Result<Value, String> {
    option_is_none(val)
}

#[native_function]
pub fn __result_unwrap(val: Value) -> Result<Value, String> {
    unwrap_val(val)
}

#[native_function]
pub fn unwrap(val: Value) -> Result<Value, String> {
    unwrap_val(val)
}

#[native_function]
pub fn __result_unwrap_or(val: Value, default: Value) -> Result<Value, String> {
    unwrap_or_val(val, default)
}

#[native_function]
pub fn unwrap_or(val: Value, default: Value) -> Result<Value, String> {
    unwrap_or_val(val, default)
}

#[native_function]
pub fn __result_expect(val: Value, msg: Value) -> Result<Value, String> {
    expect_val(val, msg)
}

#[native_function]
pub fn expect(val: Value, msg: Value) -> Result<Value, String> {
    expect_val(val, msg)
}

#[native_function]
pub fn __result_unwrap_err(val: Value) -> Result<Value, String> {
    unwrap_err_val(val)
}

#[native_function]
pub fn unwrap_err(val: Value) -> Result<Value, String> {
    unwrap_err_val(val)
}

#[native_function]
pub fn __option_ok_or(val: Value, err: Value) -> Result<Value, String> {
    option_ok_or(val, err)
}

#[native_function]
pub fn ok_or(val: Value, err: Value) -> Result<Value, String> {
    option_ok_or(val, err)
}

#[native_function]
pub fn __result_ok(val: Value) -> Result<Value, String> {
    result_to_ok(val)
}

#[native_function]
pub fn ok(val: Value) -> Result<Value, String> {
    result_to_ok(val)
}

#[native_function]
pub fn __result_err(err: Value) -> Result<Value, String> {
    result_to_err(err)
}

#[native_function]
pub fn err(err: Value) -> Result<Value, String> {
    result_to_err(err)
}

#[native_function]
pub fn __option_some(val: Value) -> Result<Value, String> {
    Ok(Value::some(val))
}

#[native_function]
pub fn some(val: Value) -> Result<Value, String> {
    Ok(Value::some(val))
}

#[native_function]
pub fn __option_none() -> Result<Value, String> {
    Ok(Value::none())
}

#[native_function]
pub fn none() -> Result<Value, String> {
    Ok(Value::none())
}
fn raw_bytes_new(size: Value) -> Result<Value, String> {
    let s = match size {
        Value::Int(i) if i >= 0 => i as usize,
        _ => return Ok(Value::err("Expected non-negative integer for bytes length")),
    };
    Ok(Value::ok(Value::bytes(vec![0u8; s])))
}

fn raw_bytes_from_string(str_val: Value) -> Result<Value, String> {
    match str_val {
        Value::String(s) => Ok(Value::bytes(s.as_bytes().to_vec())),
        _ => Ok(Value::err("Expected string for bytes_from_string")),
    }
}

fn raw_bytes_from_array(arr: Value) -> Result<Value, String> {
    match arr {
        Value::Object(map_ref) => {
            let map = map_ref.borrow();
            let mut vec = Vec::with_capacity(map.len());
            for i in 0..(map.len() as i64) {
                let key = Value::Int(i).get_hash();
                if let Some(val) = map.get(&key) {
                    if let Value::Int(b) = val {
                        vec.push((b & 0xFF) as u8);
                    } else {
                        vec.push(0);
                    }
                }
            }
            Ok(Value::bytes(vec))
        }
        Value::Bytes(b_ref) => Ok(Value::bytes(b_ref.borrow().clone())),
        _ => Ok(Value::err("Expected object array or bytes")),
    }
}

fn raw_bytes_from_hex(hex_val: Value) -> Result<Value, String> {
    let s = match hex_val {
        Value::String(s) => s,
        _ => return Ok(Value::err("Expected string for bytes_from_hex")),
    };
    let clean = s.trim_start_matches("0x").trim_start_matches("0X");
    if clean.len() % 2 != 0 {
        return Ok(Value::err("Hex string must have even length"));
    }
    let mut bytes = Vec::with_capacity(clean.len() / 2);
    for chunk in clean.as_bytes().chunks(2) {
        let chunk_str = match std::str::from_utf8(chunk) {
            Ok(c) => c,
            Err(_) => return Ok(Value::err("Invalid UTF-8 in hex string")),
        };
        match u8::from_str_radix(chunk_str, 16) {
            Ok(b) => bytes.push(b),
            Err(e) => return Ok(Value::err(format!("Invalid hex character: {}", e))),
        }
    }
    Ok(Value::ok(Value::bytes(bytes)))
}

fn raw_bytes_len(buf: Value) -> Result<Value, String> {
    match buf {
        Value::Bytes(b) => Ok(Value::Int(b.borrow().len() as i64)),
        _ => Err("Expected Bytes for bytes_len".to_string()),
    }
}

fn raw_bytes_get(buf: Value, idx: Value) -> Result<Value, String> {
    let index = match idx {
        Value::Int(i) if i >= 0 => i as usize,
        _ => {
            return Ok(Value::err(
                "Expected non-negative integer index for bytes_get",
            ));
        }
    };
    match buf {
        Value::Bytes(b) => {
            let slice = b.borrow();
            if index < slice.len() {
                Ok(Value::ok(Value::Int(slice[index] as i64)))
            } else {
                Ok(Value::err("Index out of bounds"))
            }
        }
        _ => Ok(Value::err("Expected Bytes")),
    }
}

fn raw_bytes_set(buf: Value, idx: Value, val: Value) -> Result<Value, String> {
    let index = match idx {
        Value::Int(i) if i >= 0 => i as usize,
        _ => {
            return Ok(Value::err(
                "Expected non-negative integer index for bytes_set",
            ));
        }
    };
    let byte_val = match val {
        Value::Int(i) => (i & 0xFF) as u8,
        _ => return Ok(Value::err("Expected integer byte value")),
    };
    match buf {
        Value::Bytes(b) => {
            let mut slice = b.borrow_mut();
            if index < slice.len() {
                slice[index] = byte_val;
                Ok(Value::ok(Value::Bool(true)))
            } else {
                Ok(Value::err("Index out of bounds"))
            }
        }
        _ => Ok(Value::err("Expected Bytes")),
    }
}

fn raw_bytes_push(buf: Value, val: Value) -> Result<Value, String> {
    let byte_val = match val {
        Value::Int(i) => (i & 0xFF) as u8,
        _ => return Err("Expected integer byte for bytes_push".to_string()),
    };
    match buf {
        Value::Bytes(b) => {
            b.borrow_mut().push(byte_val);
            Ok(Value::Bool(true))
        }
        _ => Err("Expected Bytes for bytes_push".to_string()),
    }
}

fn raw_bytes_extend(buf: Value, other: Value) -> Result<Value, String> {
    match (&buf, &other) {
        (Value::Bytes(b1), Value::Bytes(b2)) => {
            let other_slice = b2.borrow().clone();
            b1.borrow_mut().extend_from_slice(&other_slice);
            Ok(Value::Bool(true))
        }
        (Value::Bytes(b1), Value::String(s)) => {
            b1.borrow_mut().extend_from_slice(s.as_bytes());
            Ok(Value::Bool(true))
        }
        _ => Err("Expected Bytes and Bytes/String for bytes_extend".to_string()),
    }
}

fn raw_bytes_slice(buf: Value, start: Value, end: Value) -> Result<Value, String> {
    let s_idx = match start {
        Value::Int(i) if i >= 0 => i as usize,
        _ => return Ok(Value::err("Expected non-negative integer for slice start")),
    };
    match buf {
        Value::Bytes(b) => {
            let slice = b.borrow();
            let e_idx = match end {
                Value::Int(i) if i >= 0 => std::cmp::min(i as usize, slice.len()),
                _ => slice.len(),
            };
            if s_idx <= e_idx && s_idx <= slice.len() {
                Ok(Value::ok(Value::bytes(slice[s_idx..e_idx].to_vec())))
            } else {
                Ok(Value::err("Invalid slice range"))
            }
        }
        _ => Ok(Value::err("Expected Bytes for bytes_slice")),
    }
}

fn raw_bytes_resize(buf: Value, new_len: Value, fill: Value) -> Result<Value, String> {
    let len = match new_len {
        Value::Int(i) if i >= 0 => i as usize,
        _ => return Err("Expected non-negative integer for new length".to_string()),
    };
    let fill_byte = match fill {
        Value::Int(i) => (i & 0xFF) as u8,
        _ => 0u8,
    };
    match buf {
        Value::Bytes(b) => {
            b.borrow_mut().resize(len, fill_byte);
            Ok(Value::Bool(true))
        }
        _ => Err("Expected Bytes for bytes_resize".to_string()),
    }
}

fn raw_bytes_fill(buf: Value, byte_val: Value) -> Result<Value, String> {
    let b_val = match byte_val {
        Value::Int(i) => (i & 0xFF) as u8,
        _ => return Err("Expected integer byte for bytes_fill".to_string()),
    };
    match buf {
        Value::Bytes(b) => {
            b.borrow_mut().fill(b_val);
            Ok(Value::Bool(true))
        }
        _ => Err("Expected Bytes for bytes_fill".to_string()),
    }
}

fn raw_bytes_to_string(buf: Value) -> Result<Value, String> {
    match buf {
        Value::Bytes(b) => {
            let slice = b.borrow();
            match std::str::from_utf8(&slice) {
                Ok(s) => Ok(Value::ok(Value::String(s.to_string().into()))),
                Err(e) => Ok(Value::err(format!("Invalid UTF-8: {}", e))),
            }
        }
        _ => Ok(Value::err("Expected Bytes for bytes_to_string")),
    }
}

fn raw_bytes_to_hex(buf: Value) -> Result<Value, String> {
    match buf {
        Value::Bytes(b) => {
            let slice = b.borrow();
            let mut hex_str = String::with_capacity(slice.len() * 2);
            for byte in slice.iter() {
                use std::fmt::Write;
                let _ = write!(hex_str, "{:02x}", byte);
            }
            Ok(Value::String(hex_str.into()))
        }
        _ => Err("Expected Bytes for bytes_to_hex".to_string()),
    }
}

fn raw_bytes_get_u16_be(buf: Value, offset: Value) -> Result<Value, String> {
    let off = match offset {
        Value::Int(i) if i >= 0 => i as usize,
        _ => return Ok(Value::err("Expected non-negative integer offset")),
    };
    match buf {
        Value::Bytes(b) => {
            let slice = b.borrow();
            if off + 2 <= slice.len() {
                let num = u16::from_be_bytes([slice[off], slice[off + 1]]);
                Ok(Value::ok(Value::Int(num as i64)))
            } else {
                Ok(Value::err("Offset out of bounds for get_u16_be"))
            }
        }
        _ => Ok(Value::err("Expected Bytes")),
    }
}

fn raw_bytes_get_u16_le(buf: Value, offset: Value) -> Result<Value, String> {
    let off = match offset {
        Value::Int(i) if i >= 0 => i as usize,
        _ => return Ok(Value::err("Expected non-negative integer offset")),
    };
    match buf {
        Value::Bytes(b) => {
            let slice = b.borrow();
            if off + 2 <= slice.len() {
                let num = u16::from_le_bytes([slice[off], slice[off + 1]]);
                Ok(Value::ok(Value::Int(num as i64)))
            } else {
                Ok(Value::err("Offset out of bounds for get_u16_le"))
            }
        }
        _ => Ok(Value::err("Expected Bytes")),
    }
}

fn raw_bytes_set_u16_be(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    let off = match offset {
        Value::Int(i) if i >= 0 => i as usize,
        _ => return Ok(Value::err("Expected non-negative integer offset")),
    };
    let num = match val {
        Value::Int(i) => i as u16,
        _ => return Ok(Value::err("Expected integer value")),
    };
    match buf {
        Value::Bytes(b) => {
            let mut slice = b.borrow_mut();
            if off + 2 <= slice.len() {
                let bytes = num.to_be_bytes();
                slice[off..off + 2].copy_from_slice(&bytes);
                Ok(Value::ok(Value::Bool(true)))
            } else {
                Ok(Value::err("Offset out of bounds for set_u16_be"))
            }
        }
        _ => Ok(Value::err("Expected Bytes")),
    }
}

fn raw_bytes_set_u16_le(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    let off = match offset {
        Value::Int(i) if i >= 0 => i as usize,
        _ => return Ok(Value::err("Expected non-negative integer offset")),
    };
    let num = match val {
        Value::Int(i) => i as u16,
        _ => return Ok(Value::err("Expected integer value")),
    };
    match buf {
        Value::Bytes(b) => {
            let mut slice = b.borrow_mut();
            if off + 2 <= slice.len() {
                let bytes = num.to_le_bytes();
                slice[off..off + 2].copy_from_slice(&bytes);
                Ok(Value::ok(Value::Bool(true)))
            } else {
                Ok(Value::err("Offset out of bounds for set_u16_le"))
            }
        }
        _ => Ok(Value::err("Expected Bytes")),
    }
}

fn raw_bytes_get_u32_be(buf: Value, offset: Value) -> Result<Value, String> {
    let off = match offset {
        Value::Int(i) if i >= 0 => i as usize,
        _ => return Ok(Value::err("Expected non-negative integer offset")),
    };
    match buf {
        Value::Bytes(b) => {
            let slice = b.borrow();
            if off + 4 <= slice.len() {
                let bytes: [u8; 4] = slice[off..off + 4].try_into().unwrap();
                Ok(Value::ok(Value::Int(u32::from_be_bytes(bytes) as i64)))
            } else {
                Ok(Value::err("Offset out of bounds for get_u32_be"))
            }
        }
        _ => Ok(Value::err("Expected Bytes")),
    }
}

fn raw_bytes_get_u32_le(buf: Value, offset: Value) -> Result<Value, String> {
    let off = match offset {
        Value::Int(i) if i >= 0 => i as usize,
        _ => return Ok(Value::err("Expected non-negative integer offset")),
    };
    match buf {
        Value::Bytes(b) => {
            let slice = b.borrow();
            if off + 4 <= slice.len() {
                let bytes: [u8; 4] = slice[off..off + 4].try_into().unwrap();
                Ok(Value::ok(Value::Int(u32::from_le_bytes(bytes) as i64)))
            } else {
                Ok(Value::err("Offset out of bounds for get_u32_le"))
            }
        }
        _ => Ok(Value::err("Expected Bytes")),
    }
}

fn raw_bytes_set_u32_be(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    let off = match offset {
        Value::Int(i) if i >= 0 => i as usize,
        _ => return Ok(Value::err("Expected non-negative integer offset")),
    };
    let num = match val {
        Value::Int(i) => i as u32,
        _ => return Ok(Value::err("Expected integer value")),
    };
    match buf {
        Value::Bytes(b) => {
            let mut slice = b.borrow_mut();
            if off + 4 <= slice.len() {
                slice[off..off + 4].copy_from_slice(&num.to_be_bytes());
                Ok(Value::ok(Value::Bool(true)))
            } else {
                Ok(Value::err("Offset out of bounds for set_u32_be"))
            }
        }
        _ => Ok(Value::err("Expected Bytes")),
    }
}

fn raw_bytes_set_u32_le(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    let off = match offset {
        Value::Int(i) if i >= 0 => i as usize,
        _ => return Ok(Value::err("Expected non-negative integer offset")),
    };
    let num = match val {
        Value::Int(i) => i as u32,
        _ => return Ok(Value::err("Expected integer value")),
    };
    match buf {
        Value::Bytes(b) => {
            let mut slice = b.borrow_mut();
            if off + 4 <= slice.len() {
                slice[off..off + 4].copy_from_slice(&num.to_le_bytes());
                Ok(Value::ok(Value::Bool(true)))
            } else {
                Ok(Value::err("Offset out of bounds for set_u32_le"))
            }
        }
        _ => Ok(Value::err("Expected Bytes")),
    }
}

fn raw_bytes_get_u64_be(buf: Value, offset: Value) -> Result<Value, String> {
    let off = match offset {
        Value::Int(i) if i >= 0 => i as usize,
        _ => return Ok(Value::err("Expected non-negative integer offset")),
    };
    match buf {
        Value::Bytes(b) => {
            let slice = b.borrow();
            if off + 8 <= slice.len() {
                let bytes: [u8; 8] = slice[off..off + 8].try_into().unwrap();
                Ok(Value::ok(Value::Int(u64::from_be_bytes(bytes) as i64)))
            } else {
                Ok(Value::err("Offset out of bounds for get_u64_be"))
            }
        }
        _ => Ok(Value::err("Expected Bytes")),
    }
}

fn raw_bytes_get_u64_le(buf: Value, offset: Value) -> Result<Value, String> {
    let off = match offset {
        Value::Int(i) if i >= 0 => i as usize,
        _ => return Ok(Value::err("Expected non-negative integer offset")),
    };
    match buf {
        Value::Bytes(b) => {
            let slice = b.borrow();
            if off + 8 <= slice.len() {
                let bytes: [u8; 8] = slice[off..off + 8].try_into().unwrap();
                Ok(Value::ok(Value::Int(u64::from_le_bytes(bytes) as i64)))
            } else {
                Ok(Value::err("Offset out of bounds for get_u64_le"))
            }
        }
        _ => Ok(Value::err("Expected Bytes")),
    }
}

fn raw_bytes_set_u64_be(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    let off = match offset {
        Value::Int(i) if i >= 0 => i as usize,
        _ => return Ok(Value::err("Expected non-negative integer offset")),
    };
    let num = match val {
        Value::Int(i) => i as u64,
        _ => return Ok(Value::err("Expected integer value")),
    };
    match buf {
        Value::Bytes(b) => {
            let mut slice = b.borrow_mut();
            if off + 8 <= slice.len() {
                slice[off..off + 8].copy_from_slice(&num.to_be_bytes());
                Ok(Value::ok(Value::Bool(true)))
            } else {
                Ok(Value::err("Offset out of bounds for set_u64_be"))
            }
        }
        _ => Ok(Value::err("Expected Bytes")),
    }
}

fn raw_bytes_set_u64_le(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    let off = match offset {
        Value::Int(i) if i >= 0 => i as usize,
        _ => return Ok(Value::err("Expected non-negative integer offset")),
    };
    let num = match val {
        Value::Int(i) => i as u64,
        _ => return Ok(Value::err("Expected integer value")),
    };
    match buf {
        Value::Bytes(b) => {
            let mut slice = b.borrow_mut();
            if off + 8 <= slice.len() {
                slice[off..off + 8].copy_from_slice(&num.to_le_bytes());
                Ok(Value::ok(Value::Bool(true)))
            } else {
                Ok(Value::err("Offset out of bounds for set_u64_le"))
            }
        }
        _ => Ok(Value::err("Expected Bytes")),
    }
}

#[native_function]
pub fn __bytes_new(size: Value) -> Result<Value, String> {
    raw_bytes_new(size)
}

#[native_function]
pub fn bytes_new(size: Value) -> Result<Value, String> {
    raw_bytes_new(size)
}

#[native_function]
pub fn bytes(size: Value) -> Result<Value, String> {
    raw_bytes_new(size)
}

#[native_function]
pub fn __bytes_from_string(str_val: Value) -> Result<Value, String> {
    raw_bytes_from_string(str_val)
}

#[native_function]
pub fn bytes_from_string(str_val: Value) -> Result<Value, String> {
    raw_bytes_from_string(str_val)
}

#[native_function]
pub fn __bytes_from_array(arr: Value) -> Result<Value, String> {
    raw_bytes_from_array(arr)
}

#[native_function]
pub fn bytes_from_array(arr: Value) -> Result<Value, String> {
    raw_bytes_from_array(arr)
}

#[native_function]
pub fn __bytes_from_hex(hex_val: Value) -> Result<Value, String> {
    raw_bytes_from_hex(hex_val)
}

#[native_function]
pub fn bytes_from_hex(hex_val: Value) -> Result<Value, String> {
    raw_bytes_from_hex(hex_val)
}

#[native_function]
pub fn __bytes_len(buf: Value) -> Result<Value, String> {
    raw_bytes_len(buf)
}

#[native_function]
pub fn bytes_len(buf: Value) -> Result<Value, String> {
    raw_bytes_len(buf)
}

#[native_function]
pub fn __bytes_get(buf: Value, idx: Value) -> Result<Value, String> {
    raw_bytes_get(buf, idx)
}

#[native_function]
pub fn bytes_get(buf: Value, idx: Value) -> Result<Value, String> {
    raw_bytes_get(buf, idx)
}

#[native_function]
pub fn __bytes_set(buf: Value, idx: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set(buf, idx, val)
}

#[native_function]
pub fn bytes_set(buf: Value, idx: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set(buf, idx, val)
}

#[native_function]
pub fn __bytes_push(buf: Value, val: Value) -> Result<Value, String> {
    raw_bytes_push(buf, val)
}

#[native_function]
pub fn bytes_push(buf: Value, val: Value) -> Result<Value, String> {
    raw_bytes_push(buf, val)
}

#[native_function]
pub fn __bytes_extend(buf: Value, other: Value) -> Result<Value, String> {
    raw_bytes_extend(buf, other)
}

#[native_function]
pub fn bytes_extend(buf: Value, other: Value) -> Result<Value, String> {
    raw_bytes_extend(buf, other)
}

#[native_function]
pub fn __bytes_slice(buf: Value, start: Value, end: Value) -> Result<Value, String> {
    raw_bytes_slice(buf, start, end)
}

#[native_function]
pub fn bytes_slice(buf: Value, start: Value, end: Value) -> Result<Value, String> {
    raw_bytes_slice(buf, start, end)
}

#[native_function]
pub fn __bytes_resize(buf: Value, new_len: Value, fill: Value) -> Result<Value, String> {
    raw_bytes_resize(buf, new_len, fill)
}

#[native_function]
pub fn bytes_resize(buf: Value, new_len: Value, fill: Value) -> Result<Value, String> {
    raw_bytes_resize(buf, new_len, fill)
}

#[native_function]
pub fn __bytes_fill(buf: Value, byte_val: Value) -> Result<Value, String> {
    raw_bytes_fill(buf, byte_val)
}

#[native_function]
pub fn bytes_fill(buf: Value, byte_val: Value) -> Result<Value, String> {
    raw_bytes_fill(buf, byte_val)
}

#[native_function]
pub fn __bytes_to_string(buf: Value) -> Result<Value, String> {
    raw_bytes_to_string(buf)
}

#[native_function]
pub fn bytes_to_string(buf: Value) -> Result<Value, String> {
    raw_bytes_to_string(buf)
}

#[native_function]
pub fn __bytes_to_hex(buf: Value) -> Result<Value, String> {
    raw_bytes_to_hex(buf)
}

#[native_function]
pub fn bytes_to_hex(buf: Value) -> Result<Value, String> {
    raw_bytes_to_hex(buf)
}

#[native_function]
pub fn __bytes_get_u8(buf: Value, offset: Value) -> Result<Value, String> {
    raw_bytes_get(buf, offset)
}
#[native_function]
pub fn get_u8(buf: Value, offset: Value) -> Result<Value, String> {
    raw_bytes_get(buf, offset)
}

#[native_function]
pub fn __bytes_set_u8(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set(buf, offset, val)
}
#[native_function]
pub fn set_u8(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set(buf, offset, val)
}

#[native_function]
pub fn __bytes_get_u16_be(buf: Value, offset: Value) -> Result<Value, String> {
    raw_bytes_get_u16_be(buf, offset)
}
#[native_function]
pub fn get_u16_be(buf: Value, offset: Value) -> Result<Value, String> {
    raw_bytes_get_u16_be(buf, offset)
}

#[native_function]
pub fn __bytes_get_u16_le(buf: Value, offset: Value) -> Result<Value, String> {
    raw_bytes_get_u16_le(buf, offset)
}
#[native_function]
pub fn get_u16_le(buf: Value, offset: Value) -> Result<Value, String> {
    raw_bytes_get_u16_le(buf, offset)
}

#[native_function]
pub fn __bytes_set_u16_be(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set_u16_be(buf, offset, val)
}
#[native_function]
pub fn set_u16_be(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set_u16_be(buf, offset, val)
}

#[native_function]
pub fn __bytes_set_u16_le(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set_u16_le(buf, offset, val)
}
#[native_function]
pub fn set_u16_le(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set_u16_le(buf, offset, val)
}

#[native_function]
pub fn __bytes_get_u32_be(buf: Value, offset: Value) -> Result<Value, String> {
    raw_bytes_get_u32_be(buf, offset)
}
#[native_function]
pub fn get_u32_be(buf: Value, offset: Value) -> Result<Value, String> {
    raw_bytes_get_u32_be(buf, offset)
}

#[native_function]
pub fn __bytes_get_u32_le(buf: Value, offset: Value) -> Result<Value, String> {
    raw_bytes_get_u32_le(buf, offset)
}
#[native_function]
pub fn get_u32_le(buf: Value, offset: Value) -> Result<Value, String> {
    raw_bytes_get_u32_le(buf, offset)
}

#[native_function]
pub fn __bytes_set_u32_be(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set_u32_be(buf, offset, val)
}
#[native_function]
pub fn set_u32_be(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set_u32_be(buf, offset, val)
}

#[native_function]
pub fn __bytes_set_u32_le(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set_u32_le(buf, offset, val)
}
#[native_function]
pub fn set_u32_le(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set_u32_le(buf, offset, val)
}

#[native_function]
pub fn __bytes_get_u64_be(buf: Value, offset: Value) -> Result<Value, String> {
    raw_bytes_get_u64_be(buf, offset)
}
#[native_function]
pub fn get_u64_be(buf: Value, offset: Value) -> Result<Value, String> {
    raw_bytes_get_u64_be(buf, offset)
}

#[native_function]
pub fn __bytes_get_u64_le(buf: Value, offset: Value) -> Result<Value, String> {
    raw_bytes_get_u64_le(buf, offset)
}
#[native_function]
pub fn get_u64_le(buf: Value, offset: Value) -> Result<Value, String> {
    raw_bytes_get_u64_le(buf, offset)
}

#[native_function]
pub fn __bytes_set_u64_be(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set_u64_be(buf, offset, val)
}
#[native_function]
pub fn set_u64_be(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set_u64_be(buf, offset, val)
}

#[native_function]
pub fn __bytes_set_u64_le(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set_u64_le(buf, offset, val)
}
#[native_function]
pub fn set_u64_le(buf: Value, offset: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set_u64_le(buf, offset, val)
}
#[native_function]
pub fn to_string(buf: Value) -> Result<Value, String> {
    raw_bytes_to_string(buf)
}

#[native_function]
pub fn to_hex(buf: Value) -> Result<Value, String> {
    raw_bytes_to_hex(buf)
}

#[native_function]
pub fn slice(buf: Value, start: Value, end: Value) -> Result<Value, String> {
    raw_bytes_slice(buf, start, end)
}

#[native_function]
pub fn push(buf: Value, val: Value) -> Result<Value, String> {
    raw_bytes_push(buf, val)
}

#[native_function]
pub fn extend(buf: Value, other: Value) -> Result<Value, String> {
    raw_bytes_extend(buf, other)
}

#[native_function]
pub fn fill(buf: Value, val: Value) -> Result<Value, String> {
    raw_bytes_fill(buf, val)
}

#[native_function]
pub fn resize(buf: Value, new_len: Value, fill: Value) -> Result<Value, String> {
    raw_bytes_resize(buf, new_len, fill)
}

#[native_function]
pub fn get(buf: Value, idx: Value) -> Result<Value, String> {
    raw_bytes_get(buf, idx)
}

#[native_function]
pub fn set(buf: Value, idx: Value, val: Value) -> Result<Value, String> {
    raw_bytes_set(buf, idx, val)
}
fn compute_sha1(data: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let len_bits = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0x00);
    }
    msg.extend_from_slice(&len_bits.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            let bytes: [u8; 4] = chunk[i * 4..(i + 1) * 4].try_into().unwrap();
            w[i] = u32::from_be_bytes(bytes);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for (i, item) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };

            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*item);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut out = [0u8; 20];
    out[0..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

const BASE64_CHARS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode_base64(data: &[u8]) -> String {
    let mut result = String::with_capacity((data.len() + 2).div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        let i0 = (b0 >> 2) as usize;
        let i1 = (((b0 & 0x03) << 4) | (b1 >> 4)) as usize;
        let i2 = (((b1 & 0x0F) << 2) | (b2 >> 6)) as usize;
        let i3 = (b2 & 0x3F) as usize;

        result.push(BASE64_CHARS[i0] as char);
        result.push(BASE64_CHARS[i1] as char);
        if chunk.len() > 1 {
            result.push(BASE64_CHARS[i2] as char);
        } else {
            result.push('=');
        }
        if chunk.len() > 2 {
            result.push(BASE64_CHARS[i3] as char);
        } else {
            result.push('=');
        }
    }
    result
}

fn decode_base64(s: &str) -> Result<Vec<u8>, String> {
    let clean: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    if !clean.len().is_multiple_of(4) {
        return Err("Invalid Base64 length".to_string());
    }
    let mut out = Vec::with_capacity(clean.len() / 4 * 3);
    for chunk in clean.chunks(4) {
        let mut indices = [0u8; 4];
        let mut pad_count = 0;
        for i in 0..4 {
            let b = chunk[i];
            if b == b'=' {
                pad_count += 1;
                indices[i] = 0;
            } else if let Some(idx) = BASE64_CHARS.iter().position(|&c| c == b) {
                indices[i] = idx as u8;
            } else {
                return Err(format!("Invalid Base64 character: {}", b as char));
            }
        }
        let b0 = (indices[0] << 2) | (indices[1] >> 4);
        let b1 = ((indices[1] & 0x0F) << 4) | (indices[2] >> 2);
        let b2 = ((indices[2] & 0x03) << 6) | indices[3];

        out.push(b0);
        if pad_count < 2 {
            out.push(b1);
        }
        if pad_count < 1 {
            out.push(b2);
        }
    }
    Ok(out)
}

fn raw_sha1(val: Value) -> Result<Value, String> {
    let bytes = match &val {
        Value::String(s) => s.as_bytes().to_vec(),
        Value::Bytes(b) => b.borrow().clone(),
        _ => return Ok(Value::err("Expected String or Bytes for sha1")),
    };
    let hash = compute_sha1(&bytes);
    Ok(Value::ok(Value::bytes(hash.to_vec())))
}

#[native_function]
pub fn __sha1(val: Value) -> Result<Value, String> {
    raw_sha1(val)
}

#[native_function]
pub fn sha1(val: Value) -> Result<Value, String> {
    raw_sha1(val)
}

fn raw_base64_encode(val: Value) -> Result<Value, String> {
    let bytes = match &val {
        Value::String(s) => s.as_bytes().to_vec(),
        Value::Bytes(b) => b.borrow().clone(),
        _ => return Ok(Value::err("Expected String or Bytes for base64_encode")),
    };
    Ok(Value::String(encode_base64(&bytes).into()))
}

#[native_function]
pub fn __base64_encode(val: Value) -> Result<Value, String> {
    raw_base64_encode(val)
}

#[native_function]
pub fn base64_encode(val: Value) -> Result<Value, String> {
    raw_base64_encode(val)
}

fn raw_base64_decode(val: Value) -> Result<Value, String> {
    let s = match val {
        Value::String(s) => s,
        _ => return Ok(Value::err("Expected String for base64_decode")),
    };
    match decode_base64(&s) {
        Ok(bytes) => Ok(Value::ok(Value::bytes(bytes))),
        Err(e) => Ok(Value::err(Value::String(e.into()))),
    }
}

#[native_function]
pub fn __base64_decode(val: Value) -> Result<Value, String> {
    raw_base64_decode(val)
}

#[native_function]
pub fn base64_decode(val: Value) -> Result<Value, String> {
    raw_base64_decode(val)
}

fn raw_sec_websocket_accept(key: Value) -> Result<Value, String> {
    let k = match key {
        Value::String(s) => s,
        _ => return Ok(Value::err("Expected String for sec_websocket_accept key")),
    };
    let combined = format!("{}258EAFA5-E914-47DA-95CA-C5AB0DC85B11", k);
    let hash = compute_sha1(combined.as_bytes());
    Ok(Value::String(encode_base64(&hash).into()))
}

#[native_function]
pub fn __sec_websocket_accept(key: Value) -> Result<Value, String> {
    raw_sec_websocket_accept(key)
}

#[native_function]
pub fn sec_websocket_accept(key: Value) -> Result<Value, String> {
    raw_sec_websocket_accept(key)
}

pub static EXTRA_NATIVES: std::sync::OnceLock<std::sync::Mutex<Vec<NativeFunction>>> =
    std::sync::OnceLock::new();

pub fn register_extra_native(
    name: &'static str,
    arity: u8,
    call: fn(args: &[Value]) -> Result<Value, String>,
) {
    let native = NativeFunction {
        name,
        arity,
        call: std::sync::Arc::new(call),
    };
    EXTRA_NATIVES
        .get_or_init(|| std::sync::Mutex::new(Vec::new()))
        .lock()
        .unwrap()
        .push(native);
}

fn raw_net_tcp_connect(addr: Value) -> Result<Value, String> {
    let addr_str = match addr {
        Value::String(s) => s.to_string(),
        _ => return Ok(Value::err("Expected string address for tcp_connect")),
    };

    match TcpStream::connect(&addr_str) {
        Ok(stream) => {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
            let id = NEXT_SOCKET_ID.fetch_add(1, Ordering::SeqCst);
            get_sockets()
                .lock()
                .unwrap()
                .insert(id, SocketStream::Tcp(stream));
            Ok(Value::ok(Value::Int(id as i64)))
        }
        Err(e) => Ok(Value::err(Value::String(
            format!("Failed to connect to {}: {}", addr_str, e).into(),
        ))),
    }
}

#[native_function]
pub fn __net_tcp_connect(addr: Value) -> Result<Value, String> {
    raw_net_tcp_connect(addr)
}

#[native_function]
pub fn tcp_connect(addr: Value) -> Result<Value, String> {
    raw_net_tcp_connect(addr)
}

fn raw_net_tls_connect(host: Value, port: Value) -> Result<Value, String> {
    let host_str = match host {
        Value::String(s) => s.to_string(),
        _ => return Ok(Value::err("Expected string host for tls_connect")),
    };
    let port_num = match port {
        Value::Int(i) => i as u16,
        _ => 443u16,
    };

    let addr = format!("{}:{}", host_str, port_num);
    let tcp_stream = match TcpStream::connect(&addr) {
        Ok(s) => s,
        Err(e) => {
            return Ok(Value::err(Value::String(
                format!("TCP connection to {} failed: {}", addr, e).into(),
            )));
        }
    };

    let _ = tcp_stream.set_read_timeout(Some(Duration::from_secs(10)));
    let _ = tcp_stream.set_write_timeout(Some(Duration::from_secs(10)));

    let connector = match native_tls::TlsConnector::new() {
        Ok(c) => c,
        Err(e) => {
            return Ok(Value::err(Value::String(
                format!("Failed to initialize TLS connector: {}", e).into(),
            )));
        }
    };

    match connector.connect(&host_str, tcp_stream) {
        Ok(tls_stream) => {
            let id = NEXT_SOCKET_ID.fetch_add(1, Ordering::SeqCst);
            get_sockets()
                .lock()
                .unwrap()
                .insert(id, SocketStream::Tls(tls_stream));
            Ok(Value::ok(Value::Int(id as i64)))
        }
        Err(e) => Ok(Value::err(Value::String(
            format!("TLS handshake with {} failed: {}", host_str, e).into(),
        ))),
    }
}

#[native_function]
pub fn __net_tls_connect(host: Value, port: Value) -> Result<Value, String> {
    raw_net_tls_connect(host, port)
}

#[native_function]
pub fn tls_connect(host: Value, port: Value) -> Result<Value, String> {
    raw_net_tls_connect(host, port)
}

#[cfg(unix)]
#[native_function]
pub fn __net_unix_connect(path: Value) -> Result<Value, String> {
    let path_str = match path {
        Value::String(s) => s.to_string(),
        _ => return Ok(Value::err("Expected string path for unix_connect")),
    };

    match UnixStream::connect(&path_str) {
        Ok(stream) => {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(10)));
            let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
            let id = NEXT_SOCKET_ID.fetch_add(1, Ordering::SeqCst);
            get_sockets()
                .lock()
                .unwrap()
                .insert(id, SocketStream::Unix(stream));
            Ok(Value::ok(Value::Int(id as i64)))
        }
        Err(e) => Ok(Value::err(Value::String(
            format!("Failed to connect to UNIX socket {}: {}", path_str, e).into(),
        ))),
    }
}

#[cfg(not(unix))]
#[native_function]
pub fn __net_unix_connect(path: Value) -> Result<Value, String> {
    let _ = path;
    Ok(Value::err(
        "UNIX domain sockets are not supported on this platform",
    ))
}

fn raw_net_read(socket_id: Value, max_len: Value) -> Result<Value, String> {
    let id = match socket_id {
        Value::Int(i) if i > 0 => i as u64,
        _ => return Ok(Value::err("Expected integer socket id for net_read")),
    };
    let len = match max_len {
        Value::Int(i) if i > 0 => i as usize,
        _ => 4096usize,
    };

    let mut sockets = get_sockets().lock().unwrap();
    let stream = match sockets.get_mut(&id) {
        Some(s) => s,
        None => return Ok(Value::err("Socket not found or already closed")),
    };

    let mut buf = vec![0u8; len];
    let read_res = match stream {
        SocketStream::Tcp(s) => s.read(&mut buf),
        SocketStream::Tls(s) => s.read(&mut buf),
        #[cfg(unix)]
        SocketStream::Unix(s) => s.read(&mut buf),
    };

    match read_res {
        Ok(n) => {
            buf.truncate(n);
            Ok(Value::ok(Value::bytes(buf)))
        }
        Err(e) => Ok(Value::err(Value::String(
            format!("Read error: {}", e).into(),
        ))),
    }
}

#[native_function]
pub fn __net_read(socket_id: Value, max_len: Value) -> Result<Value, String> {
    raw_net_read(socket_id, max_len)
}

#[native_function]
pub fn net_read(socket_id: Value, max_len: Value) -> Result<Value, String> {
    raw_net_read(socket_id, max_len)
}

#[native_function]
pub fn __net_write(socket_id: Value, data: Value) -> Result<Value, String> {
    let id = match socket_id {
        Value::Int(i) if i > 0 => i as u64,
        _ => return Ok(Value::err("Expected integer socket id for net_write")),
    };

    let bytes = match &data {
        Value::Bytes(b) => b.borrow().clone(),
        Value::String(s) => s.as_bytes().to_vec(),
        _ => return Ok(Value::err("Expected Bytes or String data for net_write")),
    };

    let mut sockets = get_sockets().lock().unwrap();
    let stream = match sockets.get_mut(&id) {
        Some(s) => s,
        None => return Ok(Value::err("Socket not found or already closed")),
    };

    let write_res = match stream {
        SocketStream::Tcp(s) => s.write_all(&bytes).and_then(|_| s.flush()),
        SocketStream::Tls(s) => s.write_all(&bytes).and_then(|_| s.flush()),
        #[cfg(unix)]
        SocketStream::Unix(s) => s.write_all(&bytes).and_then(|_| s.flush()),
    };

    match write_res {
        Ok(()) => Ok(Value::ok(Value::Int(bytes.len() as i64))),
        Err(e) => Ok(Value::err(Value::String(
            format!("Write error: {}", e).into(),
        ))),
    }
}

#[native_function]
pub fn __net_close(socket_id: Value) -> Result<Value, String> {
    let id = match socket_id {
        Value::Int(i) if i > 0 => i as u64,
        _ => return Ok(Value::err("Expected integer socket id for net_close")),
    };

    let mut sockets = get_sockets().lock().unwrap();
    if let Some(stream) = sockets.remove(&id) {
        match stream {
            SocketStream::Tcp(s) => {
                let _ = s.shutdown(std::net::Shutdown::Both);
            }
            SocketStream::Tls(mut s) => {
                let _ = s.shutdown();
            }
            #[cfg(unix)]
            SocketStream::Unix(s) => {
                let _ = s.shutdown(std::net::Shutdown::Both);
            }
        }
        Ok(Value::ok(Value::Bool(true)))
    } else {
        Ok(Value::ok(Value::Bool(false)))
    }
}

#[derive(Clone)]
pub struct StdModule {
    pub functions: Vec<NativeFunction>,
}

static CORE_FUNCTIONS: std::sync::LazyLock<Vec<NativeFunction>> = std::sync::LazyLock::new(|| {
    vec![
        println(),
        print(),
        __timestamp(),
        __iter_of(),
        __get_next(),
        __has_next(),
        __rand(),
        __exec(),
        __env(),
        __joinStr(),
        __math_sin(),
        __math_cos(),
        __math_floor(),
        __math_pow(),
        __str_len(),
        __str_strip_prefix(),
        __str_strip_suffix(),
        strip_prefix(),
        strip_suffix(),
        __str_lower(),
        __str_upper(),
        __str_byte_at(),
        __byte_rotate_left(),
        __object_len(),
        __result_is_ok(),
        __result_is_err(),
        __option_is_some(),
        __option_is_none(),
        __result_unwrap(),
        __result_unwrap_or(),
        __result_expect(),
        __result_unwrap_err(),
        __option_ok_or(),
        __result_ok(),
        __result_err(),
        __option_some(),
        __option_none(),
        __bytes_new(),
        __bytes_from_string(),
        __bytes_from_array(),
        __bytes_from_hex(),
        __bytes_len(),
        __bytes_get(),
        __bytes_set(),
        __bytes_push(),
        __bytes_extend(),
        __bytes_slice(),
        __bytes_resize(),
        __bytes_fill(),
        __bytes_to_string(),
        __bytes_to_hex(),
        __bytes_get_u8(),
        __bytes_set_u8(),
        __bytes_get_u16_be(),
        __bytes_get_u16_le(),
        __bytes_set_u16_be(),
        __bytes_set_u16_le(),
        __bytes_get_u32_be(),
        __bytes_get_u32_le(),
        __bytes_set_u32_be(),
        __bytes_set_u32_le(),
        __bytes_get_u64_be(),
        __bytes_get_u64_le(),
        __bytes_set_u64_be(),
        __bytes_set_u64_le(),
        is_ok(),
        is_err(),
        is_some(),
        is_none(),
        unwrap(),
        unwrap_or(),
        expect(),
        unwrap_err(),
        ok_or(),
        len(),
        ok(),
        err(),
        some(),
        none(),
        bytes_new(),
        bytes(),
        bytes_len(),
        bytes_get(),
        bytes_set(),
        bytes_push(),
        bytes_extend(),
        bytes_slice(),
        bytes_resize(),
        bytes_fill(),
        bytes_to_string(),
        bytes_to_hex(),
        get_u8(),
        set_u8(),
        get_u16_be(),
        get_u16_le(),
        set_u16_be(),
        set_u16_le(),
        get_u32_be(),
        get_u32_le(),
        set_u32_be(),
        set_u32_le(),
        get_u64_be(),
        get_u64_le(),
        set_u64_be(),
        set_u64_le(),
        to_string(),
        to_hex(),
        slice(),
        push(),
        extend(),
        fill(),
        resize(),
        get(),
        set(),
        __sha1(),
        __base64_encode(),
        __base64_decode(),
        __sec_websocket_accept(),
        sha1(),
        base64_encode(),
        base64_decode(),
        sec_websocket_accept(),
        __net_tcp_connect(),
        __net_tls_connect(),
        __net_unix_connect(),
        __net_read(),
        __net_write(),
        __net_close(),
        tcp_connect(),
        tls_connect(),
        net_read(),
    ]
});

impl StdModule {
    pub fn get_core() -> Self {
        let mut functions = CORE_FUNCTIONS.clone();
        if let Some(extra) = EXTRA_NATIVES.get()
            && let Ok(guard) = extra.lock()
        {
            functions.extend((*guard).clone());
        }

        Self { functions }
    }
}
