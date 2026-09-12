# Parts (`.pts`)

**Parts** is a high-performance, lightweight, register-based scripting language and virtual machine written in Rust. Designed for fast embedding, clear syntax, and expressive scripting, Parts provides **Uniform Function Call Syntax (UFCS)**, **token-level hygienic macros (`part`)**, **algebraic data types (Enums)**, **pattern matching**, **endian-aware byte buffer manipulation**, **embedded standard library modules**, **zero-overhead bytecode caching**, and **bidirectional Rust interoperability**.

> 📖 **Looking for syntax, standard library reference, and language quirks?**  
> Check out the complete [**Parts Language Guide & Specific Quirks (GUIDE.md)**](GUIDE.md).

---

## Highlights

- **Register-Based Virtual Machine**: Fast execution model with 256-register frames and compact bytecode emission.
- **Uniform Function Call Syntax (UFCS)**: Call any function as a method (`value.func(args)` $\equiv$ `func(value, args)`).
- **First-Class Enums & Tagged Variants**: Algebraic data types with pattern matching (`match`) and named fields.
- **Syntactic Macros (`part`)**: Token-pattern matching macros with hygienic identifier substitution (`@param`).
- **Rich Byte Buffer Primitives (`std.bytes`)**: Direct memory buffers with 8/16/32/64-bit Little/Big Endian encoding/decoding.
- **Pure-Script WebSocket Stack (`std.ws`)**: Complete RFC 6455 WebSocket client/server frame encoder, decoder, and handshake implementation in pure Parts.
- **Embedded Standard Library**: Built-in modules (`@std/...`) embedded directly into the binary via `rust-embed`.
- **Multi-Pass Optimizations**: AST constant folding, function inlining, register compaction, peephole optimizations, and dead-code elimination.
- **Instantaneous Bytecode Caching (`.ptc`)**: XXH3-verified binary bytecode cache for sub-millisecond startup times.

---

## Code at a Glance

```parts
"Import standard library from embedded binary assets"
let std = import `@std/std.pts`;

"1. First-class Algebraic Enums"
enum Shape {
    Circle(radius),
    Rectangle(width, height)
}

"2. Compact Functions & UFCS"
let area(shape) {
    return match shape {
        Shape::Circle(r) => std.math.PI * (r * r),
        Shape::Rectangle(w, h) => w * h
    };
}

"3. Object Literals with |> ... <|"
let c = Shape::Circle(5.0);
let rect = Shape::Rectangle(10.0, 4.0);

"4. UFCS: calling area() as a method"
println("Circle area: ", c.area());
println("Rect area: ", rect.area());

"5. Token-Level Macros"
part greet {
    (@name) => { println("Hello, ", @name, "!"); }
}

greet!{ "World" };
```

---

## Quick Start & CLI

### Building from Source

```bash
cargo build --release
```

The compiled binary will be located at `target/release/parts`.

### Running Scripts

```bash
cargo run -- examples/example_script.pts
# or using the release binary:
./target/release/parts examples/example_script.pts
```

### Command-Line Flags

```
Usage: parts [OPTIONS] [FILE]

Arguments:
  [FILE]  Input script (.pts) to run or check

Options:
  -d, --debug     Print debug compiler and VM information
  -s, --shebang   Skip first line (shebang) of source file
  -t, --timed     Print phase execution timers
  -c, --cached    Use cached compiled bytecode (.ptc)
  -o, --optimize  Run optimization passes
      --check     Check syntax and report diagnostics without executing
      --no-color  Disable ANSI colored output
      --lsp       Start Language Server Protocol (LSP) mode
  -h, --help      Print help information
  -V, --version   Print version information
```

### Shebang Scripts

You can write standalone executable scripts by adding a shebang line:

```parts
#!/usr/bin/env -S parts -s
let std = import `@std/std.pts`;
println("Hello from a standalone Parts script!");
```

```bash
chmod +x script.pts
./script.pts
```

---

## Editor Support, Syntax Highlighting & LSP

Parts provides first-class developer tooling with syntax highlighting packages and a built-in Language Server Protocol (LSP) server in `editors/`.

### 1. Language Server Protocol (`parts --lsp`)

The built-in LSP server provides rich editor features out of the box:
- **Real-time Diagnostics**: Syntax, scanner, and compiler errors reported with caret positions on-the-fly.
- **Hover Documentation**: Markdown documentation and signatures for keywords, primitive types (`Int`, `Double`, `Bool`, `String`, `Bytes`), built-ins (`println`, `exec`, `unwrap`, etc.), and user-defined symbols.
- **Auto-Completion & Snippets**: Contextual completion for keywords, snippets (`fn`, `match`, `enum`, `for in`), built-in variants (`Result::Ok`, `Option::Some`), and `@std/...` modules.
- **Document Outline & Symbols**: Hierarchical outline of functions, enums, variants, and variables.
- **Go to Definition**: Jump directly to function, enum, variable, and macro definitions.
- **Document Formatting**: Code cleanup and trailing whitespace normalization.

To run the language server over `stdio`:
```bash
parts --lsp
```

### 2. Editor Setup

Complete installation packages and setup files are located in the [`editors/`](editors/) directory:

- **VS Code / Cursor / VSCodium**: TextMate grammar and language configuration in [`editors/vscode/`](editors/vscode/).
- **Neovim / Vim**: Syntax and filetype detection in [`editors/vim/`](editors/vim/), auto-start with `vim.lsp.start({ cmd = { 'parts', '--lsp' } })`.
- **Helix**: Language definition in [`editors/helix/languages.toml`](editors/helix/languages.toml).
- **Sublime Text**: YAML syntax in [`editors/sublime/Parts.sublime-syntax`](editors/sublime/Parts.sublime-syntax).
- **GNU Nano**: Color highlighting in [`editors/nano/parts.nanorc`](editors/nano/parts.nanorc).
- **Emacs**: Support via `eglot` mapping to `parts --lsp`.

See [**`editors/README.md`**](editors/README.md) for detailed configuration guides for each editor.

---

## Language Guide & Reference

The complete language manual has been organized into [**`GUIDE.md`**](GUIDE.md):

- [1. Comments & Strings](GUIDE.md#1-comments--strings) *(String literals as comments)*
- [2. Variables & Scope](GUIDE.md#2-variables--scope) *(Block scopes & register recycling)*
- [3. Data Types & Literals](GUIDE.md#3-data-types--literals) *(Primitives, Objects `|> <|`, Byte Buffers)*
- [4. Operators & Precedence](GUIDE.md#4-operators--precedence)
- [5. Functions & Closures](GUIDE.md#5-functions--closures)
- [6. Enums & Algebraic Data Types](GUIDE.md#6-enums--algebraic-data-types)
- [7. Built-in Result and Option](GUIDE.md#7-built-in-result-and-option)
- [8. Control Flow](GUIDE.md#8-control-flow) *(`if/else`, while-style `for`, collection `for-in`)*
- [9. Pattern Matching (`match`)](GUIDE.md#9-pattern-matching-match)
- [10. Uniform Function Call Syntax (UFCS)](GUIDE.md#10-uniform-function-call-syntax-ufcs)
- [11. Hygienic Macros (`part`)](GUIDE.md#11-hygienic-macros-part)
- [12. Modules & Imports](GUIDE.md#12-modules--imports)
- [13. Standard Library Reference](GUIDE.md#13-standard-library-reference) *(`math`, `sys`, `str`, `bytes`, `net`, `ws`)*
- [14. Specific Language Quirks & Idiosyncrasies](GUIDE.md#14-language-quirks--idiosyncrasies)

---

## Rust Embedding & Interoperability

### Embedding the `Engine`

You can embed Parts directly into your Rust applications using the `Engine` API:

```rust
use parts::engine::Engine;
use parts::value::Value;

fn main() -> Result<(), String> {
    let mut engine = Engine::new();

    // Register a custom native Rust function
    engine.register_native("rust_multiply", 2, |args| {
        match (&args[0], &args[1]) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a * b)),
            _ => Err("Expected two integers".to_string()),
        }
    });

    // Execute Parts script
    let source = "
        let result = rust_multiply(6, 7);
        return result + 10;
    ";

    let result = engine.execute(source)?;
    assert_eq!(result, Some(Value::Int(52)));

    println!("Result from Parts: {:?}", result);
    Ok(())
}
```

### Struct Derives (`IntoPartsObject`, `FromPartsObject`)

The `parts-macros` crate enables frictionless serialization between Rust structs and Parts objects:

```rust
use parts_macros::{IntoPartsObject, FromPartsObject};
use parts::value::{IntoValue, FromValue, Value};

#[derive(IntoPartsObject, FromPartsObject, Debug, PartialEq, Clone)]
pub struct Player {
    pub name: String,
    pub score: i64,
    pub is_online: bool,
    pub rank: Option<i32>,
}

fn main() {
    let player = Player {
        name: "Kirito".to_string(),
        score: 9500,
        is_online: true,
        rank: Some(1),
    };

    // Convert Rust struct -> Parts Value::Object
    let parts_val = player.clone().into_value();

    // Convert Parts Value::Object -> Rust struct
    let decoded = Player::from_value(&parts_val).unwrap();
    assert_eq!(player, decoded);
}
```

## License

Parts is open-source software licensed under the MIT License or Apache-2.0 License.
