# Parts Language Guide & Specific Quirks

This document provides a comprehensive guide to the **Parts** (`.pts`) programming language, detailing its syntax, type system, standard library, architectural execution model, and unique language quirks.

---

## Table of Contents

- [1. Comments & Strings](#1-comments--strings)
- [2. Variables & Scope](#2-variables--scope)
- [3. Data Types & Literals](#3-data-types--literals)
  - [Primitives](#primitives)
  - [Objects (`|> ... <|`)](#objects---)
  - [Raw Byte Buffers (`std.bytes`)](#raw-byte-buffers-stdbytes)
- [4. Operators & Precedence](#4-operators--precedence)
- [5. Functions & Closures](#5-functions--closures)
- [6. Enums & Algebraic Data Types](#6-enums--algebraic-data-types)
- [7. Built-in Result and Option](#7-built-in-result-and-option)
- [8. Control Flow](#8-control-flow)
  - [`if` / `else`](#if--else)
  - [`for` Conditional Loops (While Behavior)](#for-conditional-loops-while-behavior)
  - [`for` Collection Iterators](#for-collection-iterators)
- [9. Pattern Matching (`match`)](#9-pattern-matching-match)
- [10. Uniform Function Call Syntax (UFCS)](#10-uniform-function-call-syntax-ufcs)
- [11. Hygienic Macros (`part`)](#11-hygienic-macros-part)
- [12. Modules & Imports](#12-modules--imports)
- [13. Standard Library Reference](#13-standard-library-reference)
  - [`std.math`](#stdmath)
  - [`std.sys`](#stdsys)
  - [`std.str`](#stdstr)
  - [`std.result` & `std.option`](#stdresult--stdoption)
  - [`std.bytes`](#stdbytes)
  - [`std.net`](#stdnet)
  - [`std.ws` (RFC 6455 WebSockets)](#stdws-rfc-6455-websockets)
- [14. Language Quirks & Idiosyncrasies](#14-language-quirks--idiosyncrasies)

---

## 1. Comments & Strings

### String-Based Comments

Parts does not have traditional `//` or `/* */` comment tokens. Instead, **any bare string literal** placed at the statement level is parsed as a standalone expression and discarded as an ignored statement node.

```parts
"This is a comment at the statement level"
`This backtick string is also a valid comment`

let x = 42; "Inline comment after a semicolon"
```

### String Literals and Escapes

Strings can be enclosed in double quotes (`"..."`) or backticks (`` `...` ``).

```parts
let msg1 = "Hello, World!\n";
let msg2 = `Path: /usr/local/bin`;
```

Supported escape sequences:
- `\n` — Line Feed (0x0A)
- `\r` — Carriage Return (0x0D)
- `\t` — Tab (0x09)
- `\0` — Null (0x00)
- `\b` — Backspace (0x08)
- `\f` — Form Feed (0x0C)
- `\"` — Double quote
- `\\` — Backslash

---

## 2. Variables & Scope

Variables are declared using the `let` keyword and are mutable by default. Semicolons `;` are optional statement separators.

```parts
let count = 0;
count = count + 1; "Reassignment"
```

### Block Scoping and Register Recycling

Blocks `{ ... }` introduce a nested scope. When a block exits, all local registers allocated within that block are recycled back to the scope base:

```parts
let a = 10;

{
    let temp = a * 2;
    a = temp + 5;
}
"temp is out of scope here; its register is reused for subsequent allocations"

let b = 30;
```

---

## 3. Data Types & Literals

Parts is dynamically typed with zero-overhead runtime representations:

### Primitives

- **`Int`**: 64-bit signed integers (`i64`), e.g., `0`, `42`, `-100`.
- **`Double`**: 64-bit IEEE 754 floating point numbers (`f64`), e.g., `3.14159`, `-0.001`.
- **`Bool`**: Booleans `true` and `false`.
- **`String`**: Reference-counted UTF-8 strings.

### Objects (`|> ... <|`)

Objects represent associative key-value dictionaries (hash maps) delimited by pipeline brackets `|>` and `<|`:

```parts
"1. Empty object"
let empty = |><|;

"2. Populated object"
let person = |>
    name: "Alice",
    age: 30,
    is_admin: true
<|;

"3. Property access via dot notation (hashed key)"
println("Name: ", person.name);

"4. Dynamic indexing via bracket notation"
let field = "age";
println("Age: ", person[field]);

"5. Property mutation"
person.age = 31;
person["is_admin"] = false;
```

### Raw Byte Buffers (`std.bytes`)

Mutable byte buffers (`Vec<u8>`) are first-class runtime objects supporting indexing and multi-byte numeric operations:

```parts
let std = import `@std/std.pts`;

"Create a 4-byte zeroed buffer"
let buf = std.bytes.new(4).unwrap();

"Direct byte indexing"
buf[0] = 255;
buf[1] = 128;

"Length and inspection"
println("Buffer length: ", buf.len());
```

---

## 4. Operators & Precedence

| Category | Operators | Details |
| :--- | :--- | :--- |
| **Arithmetic** | `+`, `-`, `*`, `/`, `%` | Arithmetic operations on `Int` and `Double`. Modulo `%` requires integers. |
| **Bitwise** | `&`, `|`, `^`, `<<`, `>>` | Bitwise operations on `Int`, `Double` (coerced), and `Bool`. |
| **Logical** | `&`, `|` | Single ampersand/pipe serve as logical and/or. |
| **Comparison** | `==`, `<`, `>`, `<=`, `>=` | Structural comparison across all primitives and tagged enum variants. |
| **Assignment** | `=` | Assigns to variables (`x = 1`) or object properties (`obj.x = 1`). |
| **Accessors** | `.`, `[...]` | Static property dot access or dynamic array/map indexing. |

---

## 5. Functions & Closures

Functions in Parts are first-class citizens. They can be passed into functions, stored in objects, and returned from other functions.

### Function Declarations

```parts
"1. Compact single-expression syntax"
let add(a, b) = a + b;

"2. Multi-line block syntax"
let calculate_tax(subtotal, rate) {
    let tax = subtotal * rate;
    return subtotal + tax;
}

"3. Anonymous lambda syntax"
let square = fun(x) = x * x;
let cube = fun(x) { return x * x * x; };

"4. Invocation"
println("Add: ", add(5, 10));
println("Tax: ", calculate_tax(100.0, 0.08));
println("Square: ", square(4));
```

---

## 6. Enums & Algebraic Data Types

Enums define tagged variants with optional named fields:

```parts
enum Direction {
    North(power),
    East(power),
    South(power),
    West(power),
    Stationary
}

"Constructing enum variants"
let heading = Direction::North(100.0);
let idle = Direction::Stationary;

"Accessing variant fields directly"
println("Power: ", heading.power);

"Structural equality"
if heading == Direction::North(100.0) {
    println("Heading north at full power!");
}
```

---

## 7. Built-in Result and Option

The compiler includes built-in definitions for `Option` and `Result`:

```parts
let std = import `@std/std.pts`;

"Result: Ok(val) and Err(err)"
let success = Result::Ok(42);
let failure = Result::Err("Connection timed out");

println(success.is_ok());          "true"
println(failure.is_err());        "true"
println(success.unwrap());         "42"
println(failure.unwrap_or(0));     "0"

"Option: Some(val) and None"
let found = Option::Some("username");
let missing = Option::None;

println(found.is_some());          "true"
println(missing.is_none());        "true"
println(found.unwrap());           "username"
println(missing.unwrap_or("N/A")); "N/A"
```

---

## 8. Control Flow

### `if` / `else`

```parts
let score = 92;

if score >= 90 {
    println("Grade: A");
} else {
    if score >= 80 {
        println("Grade: B");
    } else {
        println("Grade: C");
    }
}
```

### `for` Conditional Loops (While Behavior)

The syntax `for <condition> { ... }` acts as a while loop:

```parts
let i = 0;
let sum = 0;

for i < 10 {
    sum = sum + i;
    i = i + 1;
}

println("Sum: ", sum);
```

### `for` Collection Iterators

The syntax `for <var> in <iterable> { ... }` iterates over characters in strings or values in objects:

```parts
"1. Iterating over characters in a string"
let text = "PARTS";
for ch in text {
    println("Char: ", ch);
}

"2. Iterating over object values"
let settings = |> host: "127.0.0.1", port: 8080 <|;
for val in settings {
    println("Value: ", val);
}
```

Loop control: `break;` terminates loop execution, and `continue;` jumps to the next iteration.

---

## 9. Pattern Matching (`match`)

`match` expressions match against enum variants, literals, and wildcards:

```parts
let describe_result(res) {
    return match res {
        Ok(val) => val * 2,
        Err(msg) => 0
    };
}

let handle_movement(dir) {
    return match dir {
        Direction::North(p) => p,
        Direction::South(p) => 0 - p,
        Direction::Stationary => 0,
        _ => 0  "Wildcard match"
    };
}

let parse_http_code(code) {
    return match code {
        200 => "OK",
        404 => "Not Found",
        500 => "Internal Server Error",
        @   => "Unknown"  "Wildcard token match"
    };
}
```

---

## 10. Uniform Function Call Syntax (UFCS)

UFCS allows calling any function as a method on its first argument:

$$\text{receiver}.\text{func}(a, b) \iff \text{func}(\text{receiver}, a, b)$$

```parts
let std = import `@std/std.pts`;

let increment(n) = n + 1;
let double(n) = n * 2;

let x = 10;

"Chaining functions as methods on numbers"
let result = x.increment().double();
println("10.increment().double() = ", result); "22"

"Methods on strings"
let greeting = "Hello ".join("World");
println("Length: ", greeting.len());

"Methods on byte buffers"
let buf = std.bytes.from_string("Parts");
buf.push(33);
println("Hex: ", buf.to_hex());
```

---

## 11. Hygienic Macros (`part`)

Syntactic macros operate on token streams during compilation using pattern matching:

```parts
part log_debug {
    (@tag, @msg) => {
        println("[", @tag, "] ", @msg);
    }
    (@msg) => {
        println("[DEBUG] ", @msg);
    }
}

"Calling macro using bang (!) syntax"
log_debug!{ "AUTH", "User authenticated" };
log_debug!{ "Initialization complete" };
```

Macro patterns support identifier capture via `@identifier` and wildcards via `@`.

---

## 12. Modules & Imports

### Embedded Virtual Modules (`@std/...`)

Standard library modules are baked into the binary using `rust-embed` and imported using `@std/` paths:

```parts
let std = import `@std/std.pts`;
let math = std.math;
let sys = std.sys;
```

### Relative File Imports

Scripts can import other `.pts` files using relative paths:

```parts
let config = import `./config.pts`;
let utils = import `./utils/helpers.pts`;
```

### Exporting from Modules

Modules export their members by returning an object at the top level:

```parts
"math_helpers.pts"
let square(x) = x * x;
let cube(x) = x * x * x;

return |>
    square: square,
    cube: cube
<|;
```

---

## 13. Standard Library Reference

### `std.math`

```parts
let math = std.math;
```

| Function / Const | Signature | Description |
| :--- | :--- | :--- |
| `PI` | `Double` | Mathematical constant $\pi \approx 3.141592653589793$ |
| `sin(x)` | `(Double) -> Double` | Sine function (radians) |
| `cos(x)` | `(Double) -> Double` | Cosine function (radians) |
| `abs(x)` | `(Int/Double) -> Int/Double` | Absolute value |
| `pow(base, exp)`| `(Double, Double) -> Double`| Exponentiation ($base^{exp}$) |
| `floor(x)` | `(Double) -> Double` | Mathematical floor rounding |
| `rand()` | `() -> Int` | Pseudo-random integer generator |

---

### `std.sys`

```parts
let sys = std.sys;
```

| Function | Signature | Description |
| :--- | :--- | :--- |
| `time()` | `() -> Int` | Current UNIX timestamp in microseconds |
| `env(name)` | `(String) -> Result<String, String>` | Reads an environment variable |
| `exec(cmd, ...args)`| `(String, ...String) -> Result<String, String>`| Executes a system command |

---

### `std.str`

```parts
let str = std.str;
```

| Function | Signature | Description |
| :--- | :--- | :--- |
| `len(s)` | `(String) -> Int` | Returns string byte count |
| `join(a, b)` | `(String, String) -> String` | Concatenates two strings |
| `strip_prefix(s, p)`| `(String, String) -> Option<String>`| Strips prefix if matched |
| `strip_suffix(s, p)`| `(String, String) -> Option<String>`| Strips suffix if matched |
| `lower(s)` | `(String) -> String` | Converts to lowercase |
| `upper(s)` | `(String) -> String` | Converts to uppercase |
| `byte_at(s, i)` | `(String, Int) -> Result<Int, String>` | Returns byte ($0\dots 255$) at index |

---

### `std.result` & `std.option`

```parts
let result = std.result;
let option = std.option;
```

| Function | Description |
| :--- | :--- |
| `Ok(val)` / `Err(err)` | Constructs `Result` variant |
| `Some(val)` / `None` | Constructs `Option` variant |
| `val.is_ok()` / `val.is_err()` | Checks `Result` status |
| `val.is_some()` / `val.is_none()`| Checks `Option` status |
| `val.unwrap()` | Unwraps inner value or errors |
| `val.unwrap_or(fallback)` | Unwraps inner value or returns fallback |
| `val.expect(msg)` | Unwraps inner value or panics with custom message |
| `val.unwrap_err()` | Unwraps error value from `Result::Err` |
| `opt.ok_or(err_msg)` | Converts `Option` to `Result` |

---

### `std.bytes`

```parts
let bytes = std.bytes;
```

| Function | Signature | Description |
| :--- | :--- | :--- |
| `new(size)` | `(Int) -> Result<Bytes, String>` | Allocates a zeroed byte buffer |
| `from_string(s)` | `(String) -> Bytes` | Creates byte buffer from UTF-8 string |
| `from_hex(hex)` | `(String) -> Result<Bytes, String>` | Decodes hex string into bytes |
| `buf.len()` | `(Bytes) -> Int` | Returns byte length |
| `buf.push(byte)` | `(Bytes, Int) -> Void` | Appends a single byte |
| `buf.extend(other)` | `(Bytes, Bytes) -> Void` | Appends bytes from another buffer |
| `buf.slice(start, end)`| `(Bytes, Int, Int) -> Result<Bytes, String>` | Slices buffer range |
| `buf.fill(val)` | `(Bytes, Int) -> Void` | Fills buffer with byte value |
| `buf.to_string()` | `(Bytes) -> Result<String, String>` | Decodes buffer to UTF-8 string |
| `buf.to_hex()` | `(Bytes) -> String` | Encodes buffer as lowercase hex |
| `buf.get_u16_be(off)` | `(Bytes, Int) -> Result<Int, String>` | Reads 16-bit Big-Endian uint |
| `buf.get_u16_le(off)` | `(Bytes, Int) -> Result<Int, String>` | Reads 16-bit Little-Endian uint |
| `buf.set_u16_be(off, v)`| `(Bytes, Int, Int) -> Result<Void, String>` | Writes 16-bit Big-Endian uint |
| `buf.set_u16_le(off, v)`| `(Bytes, Int, Int) -> Result<Void, String>` | Writes 16-bit Little-Endian uint |
| `buf.get_u32_be(off)` | `(Bytes, Int) -> Result<Int, String>` | Reads 32-bit Big-Endian uint |
| `buf.get_u32_le(off)` | `(Bytes, Int) -> Result<Int, String>` | Reads 32-bit Little-Endian uint |
| `buf.set_u32_be(off, v)`| `(Bytes, Int, Int) -> Result<Void, String>` | Writes 32-bit Big-Endian uint |
| `buf.set_u32_le(off, v)`| `(Bytes, Int, Int) -> Result<Void, String>` | Writes 32-bit Little-Endian uint |
| `buf.get_u64_be(off)` | `(Bytes, Int) -> Result<Int, String>` | Reads 64-bit Big-Endian uint |
| `buf.get_u64_le(off)` | `(Bytes, Int) -> Result<Int, String>` | Reads 64-bit Little-Endian uint |
| `buf.set_u64_be(off, v)`| `(Bytes, Int, Int) -> Result<Void, String>` | Writes 64-bit Big-Endian uint |
| `buf.set_u64_le(off, v)`| `(Bytes, Int, Int) -> Result<Void, String>` | Writes 64-bit Little-Endian uint |

---

### `std.net`

```parts
let net = std.net;
```

| Function | Signature | Description |
| :--- | :--- | :--- |
| `tcp_connect(addr)` | `(String) -> Result<Int, String>` | Opens a TCP connection (`"ip:port"`) |
| `tls_connect(host, port)`| `(String, Int) -> Result<Int, String>` | Opens a TLS encrypted TCP stream |
| `unix_connect(path)` | `(String) -> Result<Int, String>` | Connects to a UNIX domain socket |
| `read(sock, len)` | `(Int, Int) -> Result<Bytes, String>` | Reads up to `len` bytes from socket |
| `write(sock, data)` | `(Int, String/Bytes) -> Result<Int, String>` | Writes data to socket |
| `close(sock)` | `(Int) -> Result<Void, String>` | Closes socket descriptor |

---

### `std.ws` (RFC 6455 WebSockets)

Parts implements the full **RFC 6455 WebSocket** client protocol in pure Parts (`lib/std/ws.pts`):

```parts
let std = import `@std/std.pts`;
let ws = std.ws;

"1. Handshake creation"
let key = "dGhlIHNhbXBsZSBub25jZQ==";
let req = ws.create_client_handshake("echo.websocket.events", "/", key);

"2. Connection over TLS"
let sock = std.tls_connect("echo.websocket.events", 443).unwrap();
std.net_write(sock, req);

"3. Construct and encode a masked text frame"
let mask_key = std.bytes.from_hex("12345678").unwrap();
let frame = ws.text_frame("Hello from Parts WebSocket!", true, mask_key);
let encoded = ws.encode_frame(frame).unwrap();
std.net_write(sock, encoded);

"4. Read and decode response frame"
let raw_resp = std.net_read(sock, 1024).unwrap();
let decoded = ws.decode_frame(raw_resp, 0).unwrap();
println("Received: ", decoded.frame.payload.to_string().unwrap());

"5. Graceful close"
let close = ws.close_frame(ws.CLOSE_NORMAL, "done", true, mask_key);
std.net_write(sock, ws.encode_frame(close).unwrap());
std.net_close(sock);
```

---

## 14. Language Quirks & Idiosyncrasies

### 1. Comments are Bare String Literals
Parts has no comment tokens (`//`, `/* */`, `#`). Standalone string expressions (`"..."` or `` `...` ``) at the statement level are ignored by the parser and emitted as no-ops.

### 2. Object Delimiters (`|> ... <|`)
Object literals use pipeline brackets `|>` and `<|` to prevent syntax ambiguity with code blocks and match arms `{ ... }`.

### 3. `for` Condition is a `while` Loop
The syntax `for <condition> { ... }` acts as a while loop. Iteration over collections requires `for <var> in <iterable> { ... }`.

### 4. Single `&` and `|` for Logical Operations
Parts does not use `&&` or `||`. Instead, single ampersand `&` and pipe `|` are used for both bitwise and logical operations (e.g., `(a > 0) & (b > 0)`) or mathematical operators: `*` and `+` can be used instead (e.g., `(a > 0) * (b > 0)`).

### 5. Falsy Values Model
In conditional statements (`if`, `for`, `JumpNot`), only the following values evaluate to **falsy**:
- `Int(0)`
- `Double(0.0)`
- `Bool(false)`
- `String("")` (empty string)
- `Object(|><|)` (empty object)
- `Bytes(<0 bytes>)` (empty byte buffer)

All other values (including populated objects, functions, and active enum instances) evaluate to **truthy**.

### 6. UFCS Resolution Priority
When calling `receiver.method(...)`, the compiler first looks up whether `method` is a local function or registered native function. If found, it compiles as `method(receiver, ...)`. Otherwise, it falls back to dynamic property retrieval on the object.

### 7. 64-Bit Integer Key Hashing
All object keys and struct fields are hashed into `u64` via `FxHash`/`xxh3`. Property lookups operate directly on 64-bit integer hashes in bytecode.

### 8. Scope Register Recycling
Each VM call frame has 256 registers. Variables declared in inner blocks `{ let x = 1; }` release their register addresses upon block exit, allowing subsequent allocations to reuse those register slots.

### 9. First-Class Bytecode Enum Opcodes
Enums are not plain dictionary maps. They are first-class runtime entities emitted as `ConstEnum` (constructs tagged variant with args) and matched with `MatchEnum` (tag checking opcode).

### 10. Embedded Virtual Filesystem (`@std/...`)
Paths prefixed with `@std/` are embedded inside the compiled Rust binary with `rust-embed` and resolved entirely in memory.

### 11. XXH3 Bytecode Caching (`.ptc`)
When executed with `-c` / `--cached`, Parts generates a `.ptc` cache file containing a 32-byte header (`\x7fPTS`, version, XXH3 source hash, payload sizes) plus the serialized constant pool and bytecode. If the source file hash matches, compilation is skipped, achieving sub-millisecond execution.
