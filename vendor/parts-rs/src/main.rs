use clap::Parser;
use parts::{
    compiler::Compiler,
    disassemble,
    emitter::Emitter,
    optimize::{AstOptimizer, IrOptimizer},
    parser::Parser as Partser,
    value::Value,
    vm::VM,
};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};
use xxhash_rust::xxh3::xxh3_64;

#[derive(Parser, Clone)]
#[command(version, about = "Parts programming language runtime & tools", long_about = None)]
struct Cli {
    #[arg(short, long, help = "Print debug compiler and VM information")]
    debug: bool,
    #[arg(short, long, help = "Skip first line (shebang) of source file")]
    shebang: bool,
    #[arg(short, long, help = "Print phase execution timers")]
    timed: bool,
    #[arg(short, long, help = "Use cached compiled bytecode (.ptc)")]
    cached: bool,
    #[arg(short, long, help = "Run optimization passes")]
    optimize: bool,
    #[arg(long, help = "Check syntax and report diagnostics without executing")]
    check: bool,
    #[arg(long, help = "Disable ANSI colored output")]
    no_color: bool,
    #[arg(long, help = "Start Language Server Protocol (LSP) mode")]
    lsp: bool,
    #[arg(value_name = "FILE", help = "Input script (.pts) to run or check")]
    input: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();

    if cli.no_color {
        unsafe {
            std::env::set_var("NO_COLOR", "1");
        }
    }

    if cli.lsp {
        if let Err(e) = parts::lsp::run_stdio_server() {
            eprintln!("LSP server error: {}", e);
            std::process::exit(1);
        }
        return;
    }

    let input_path = match cli.input {
        Some(ref path) => path.clone(),
        None => {
            eprintln!(
                "error: no input file provided (use --help for usage, or --lsp to start language server)"
            );
            std::process::exit(1);
        }
    };

    let raw_path_str = input_path.to_string_lossy().to_string();

    if cli.check {
        let content = match fs::read_to_string(&input_path) {
            Ok(c) => {
                if cli.shebang {
                    c.split_once('\n')
                        .map(|(_, rest)| rest.to_string())
                        .unwrap_or(c)
                } else {
                    c
                }
            }
            Err(e) => {
                parts::diagnostic::Diagnostic::error(format!(
                    "failed to read '{}': {}",
                    raw_path_str, e
                ))
                .with_file(raw_path_str)
                .eprint(None, None);
                std::process::exit(1);
            }
        };

        let import_path = input_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();
        let report = parts::tools::LanguageTools::check_with_import_path(
            &content,
            Some(&raw_path_str),
            import_path,
        );

        if report.has_errors() {
            report.eprint(Some(&content), Some(&raw_path_str));
            std::process::exit(1);
        } else {
            if !report.is_empty() {
                report.eprint(Some(&content), Some(&raw_path_str));
            }
            println!("Syntax OK: {}", raw_path_str);
            std::process::exit(0);
        }
    }

    let res = get_code(cli.clone());

    let start_time_e = Instant::now();

    let mut vm = VM::new(res.code, res.consts);

    let run_res = match vm.run() {
        Ok(val) => val,
        Err(e) => {
            let ip = vm.frames.last().map(|f| f.ip).unwrap_or(0);
            let diag = e
                .to_diagnostic(Some(&raw_path_str))
                .with_note(format!("at instruction pointer {:04}", ip));
            diag.eprint(None, Some(&raw_path_str));
            std::process::exit(1);
        }
    };

    if cli.timed {
        println!("Execution took: {:?} ", start_time_e.elapsed());
    }

    if let Some(val) = run_res {
        if cli.debug {
            println!("Output: \n {:?}", val);
        } else {
            println!("{}", val);
        }
    }
}

fn get_code(config: Cli) -> CompilerOutput {
    if config.cached {
        let start_time_c = Instant::now();

        if config.debug {
            println!("Attempting to use the cached version");
        }

        let btc = get_bytecode(config.clone());

        if config.timed {
            println!("Decoding took: {:?} ", start_time_c.elapsed());
        }

        if config.debug {
            println!("Bytecode: {:?}\n", btc.code);

            println!("Consts:");

            for constant in &btc.consts {
                println!("{:?}", constant);
            }

            println!();

            disassemble::disassemble(&btc.code, &btc.consts);

            println!();
        }

        return btc;
    }

    let raw_path = config.input.as_ref().unwrap().clone();

    let content = match fs::read_to_string(&raw_path) {
        Ok(c) => {
            if config.shebang {
                c.split_once('\n')
                    .map(|(_, rest)| rest.to_string())
                    .unwrap_or(c)
            } else {
                c
            }
        }
        Err(e) => {
            parts::diagnostic::Diagnostic::error(format!(
                "failed to read '{}': {}",
                raw_path.display(),
                e
            ))
            .with_file(raw_path.to_string_lossy().to_string())
            .eprint(None, None);
            std::process::exit(1);
        }
    };

    if config.debug {
        println!("Code: \n{}\n", content);
    }

    let mut p = Partser::new(content.clone());

    let start_time_p = Instant::now();

    let file_str = raw_path.to_string_lossy().to_string();
    let mut ast = match p.parse_all() {
        Ok(ast) => ast,
        Err(e) => {
            let diag = e.to_diagnostic(Some(&content), Some(&file_str));
            diag.eprint(Some(&content), Some(&file_str));
            std::process::exit(1);
        }
    };

    if config.timed {
        println!("Parsing took: {:?} ", start_time_p.elapsed());
    }

    if config.optimize {
        let start_time_o = Instant::now();

        let mut optimizer = AstOptimizer::new();

        optimizer.collect_all(&ast);

        let mut pass = 0;

        {
            let mut changed = true;
            while changed {
                pass += 1;
                changed = optimizer.optimize_all(&mut ast);
            }
        }

        if config.timed {
            println!(
                "Optimization took: {:?} (passes: {})",
                start_time_o.elapsed(),
                pass
            );
        }
    }

    if config.debug {
        println!("Ast output:");

        for stmt in &ast {
            println!("{:?}", stmt);
        }

        println!();
    }

    let mut c = Compiler::new(
        raw_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    );
    c.optimize = config.optimize;
    let start_time_p = Instant::now();

    let mut ir = match c.compile_all(ast) {
        Ok(ir) => ir,
        Err(errors) => {
            for err in errors {
                let diag = err.to_diagnostic(Some(&content), Some(&file_str));
                diag.eprint(Some(&content), Some(&file_str));
            }
            std::process::exit(1);
        }
    };

    if config.debug && config.optimize {
        println!("Compilation took: {:?} ", start_time_p.elapsed());
    }

    if config.debug {
        println!("Before optimization {}", ir.len());
    }
    if config.optimize {
        ir = IrOptimizer::optimize(ir);
    }

    if config.debug && config.optimize {
        println!("After optimization {}", ir.len());
    }

    if config.debug {
        println!("IR output:");

        for node in &ir {
            println!("{:?}", node);
        }

        println!();
    }

    let bc = Emitter {}.emit(ir);

    if config.debug {
        println!("Bytecode: {:?}\n", bc);

        println!("Consts:");

        for constant in &c.constant_pool {
            println!("{:?}", constant);
        }

        println!();

        disassemble::disassemble(&bc, &c.constant_pool);

        println!();
    }

    CompilerOutput {
        code: bc,
        consts: c.constant_pool,
    }
}

#[repr(C)]
struct BytecodeHeader {
    magic: [u8; 4],
    version: u32,
    source_hash: u64,
    payload_size: u64,
    consts_offset: u64,
}

fn get_bytecode(config: Cli) -> CompilerOutput {
    let source_path = config.input.as_ref().unwrap().clone();
    let cache_path = source_path.with_extension("ptc");
    let source_content = match fs::read(&source_path) {
        Ok(c) => c,
        Err(e) => {
            parts::diagnostic::Diagnostic::error(format!(
                "failed to read '{}': {}",
                source_path.display(),
                e
            ))
            .with_file(source_path.to_string_lossy().to_string())
            .eprint(None, None);
            std::process::exit(1);
        }
    };
    let current_hash = xxh3_64(&source_content);

    if let Ok(mut file) = File::open(&cache_path) {
        let mut header_buf = [0u8; std::mem::size_of::<BytecodeHeader>()];

        if file.read_exact(&mut header_buf).is_ok() {
            let magic = &header_buf[0..4];
            let version = u32::from_le_bytes(header_buf[4..8].try_into().unwrap());
            let source_hash = u64::from_le_bytes(header_buf[8..16].try_into().unwrap());
            let payload_size = u64::from_le_bytes(header_buf[16..24].try_into().unwrap());
            let consts_offset = u64::from_le_bytes(header_buf[24..32].try_into().unwrap());

            let header = BytecodeHeader {
                magic: magic.try_into().unwrap(),
                version,
                source_hash,
                payload_size,
                consts_offset,
            };

            if header.magic == [0x7F, b'P', b'T', b'S']
                && header.version == 1
                && header.source_hash == current_hash
            {
                let mut buff = Vec::with_capacity(
                    header.payload_size as usize + header.consts_offset as usize,
                );

                file.read_to_end(&mut buff).unwrap();

                let encoded_c = &buff[0..header.consts_offset as usize];

                let bytecode = buff[header.consts_offset as usize
                    ..header.consts_offset as usize + header.payload_size as usize]
                    .to_vec();

                return CompilerOutput {
                    code: bytecode,
                    consts: Value::decode(encoded_c, false, 0).0,
                };
            }
        }
    }

    let content = if config.shebang {
        let str_content = String::from_utf8(source_content).unwrap_or_default();
        str_content
            .split_once('\n')
            .map(|(_, rest)| rest.to_string())
            .unwrap_or(str_content)
    } else {
        String::from_utf8(source_content).unwrap_or_default()
    };

    let mut p = Partser::new(content.clone());

    let file_str = source_path.to_string_lossy().to_string();
    let mut ast = match p.parse_all() {
        Ok(ast) => ast,
        Err(e) => {
            let diag = e.to_diagnostic(Some(&content), Some(&file_str));
            diag.eprint(Some(&content), Some(&file_str));
            std::process::exit(1);
        }
    };
    if config.optimize {
        let mut optimizer = AstOptimizer::new();
        optimizer.collect_all(&ast);
        let mut changed = true;
        while changed {
            changed = optimizer.optimize_all(&mut ast);
        }
    }

    let mut c = Compiler::new(
        source_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    );
    c.optimize = config.optimize;
    let mut ir = match c.compile_all(ast) {
        Ok(ir) => ir,
        Err(errors) => {
            for err in errors {
                let diag = err.to_diagnostic(Some(&content), Some(&file_str));
                diag.eprint(Some(&content), Some(&file_str));
            }
            std::process::exit(1);
        }
    };
    if config.optimize {
        ir = IrOptimizer::optimize(ir);
    }

    let bc = Emitter {}.emit(ir);
    save_cache(&cache_path, &bc, c.constant_pool.clone(), current_hash);
    CompilerOutput {
        code: bc,
        consts: c.constant_pool,
    }
}

struct CompilerOutput {
    code: Vec<u8>,
    consts: Vec<Value>,
}

fn save_cache(path: &Path, raw_bc: &[u8], constant_pool: Vec<Value>, hash: u64) {
    let mut encoded_c = Vec::new();
    for c in &constant_pool {
        c.encode(&mut encoded_c);
    }

    let data = raw_bc;

    let temp_path = path.with_extension("tmp");
    let mut file = File::create(&temp_path).unwrap();

    let mut header_bytes = Vec::new();
    header_bytes.extend_from_slice(&[0x7F, b'P', b'T', b'S']);
    header_bytes.extend_from_slice(&1u32.to_le_bytes());
    header_bytes.extend_from_slice(&hash.to_le_bytes());
    header_bytes.extend_from_slice(&(data.len() as u64).to_le_bytes());
    header_bytes.extend_from_slice(&(encoded_c.len() as u64).to_le_bytes());

    file.write_all(&header_bytes).unwrap();
    file.write_all(&encoded_c).unwrap();
    file.write_all(data).unwrap();
    fs::rename(temp_path, path).unwrap();
}
