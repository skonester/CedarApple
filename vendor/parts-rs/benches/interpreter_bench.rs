use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use parts::compiler::Compiler;
use parts::emitter::Emitter;
use parts::engine::Engine;
use parts::optimize::{AstOptimizer, IrOptimizer};
use parts::parser::Parser;
use parts::value::Value;

struct TrackingAllocator;

static TOTAL_ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static CURRENT_ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static PEAK_ALLOCATED: AtomicUsize = AtomicUsize::new(0);
static ALLOC_COUNT: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for TrackingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            let size = layout.size();
            TOTAL_ALLOCATED.fetch_add(size, Ordering::Relaxed);
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            let current = CURRENT_ALLOCATED.fetch_add(size, Ordering::Relaxed) + size;
            let mut peak = PEAK_ALLOCATED.load(Ordering::Relaxed);
            while current > peak {
                match PEAK_ALLOCATED.compare_exchange_weak(
                    peak,
                    current,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(actual) => peak = actual,
                }
            }
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: TrackingAllocator = TrackingAllocator;

#[derive(Debug, Clone)]
struct BenchResult {
    name: &'static str,
    time_ms: f64,
    total_allocated_kb: f64,
    peak_allocated_kb: f64,
    alloc_count: usize,
}

fn reset_stats() {
    TOTAL_ALLOCATED.store(0, Ordering::SeqCst);
    CURRENT_ALLOCATED.store(0, Ordering::SeqCst);
    PEAK_ALLOCATED.store(0, Ordering::SeqCst);
    ALLOC_COUNT.store(0, Ordering::SeqCst);
}

fn measure<F: FnOnce() -> R, R>(name: &'static str, f: F) -> (BenchResult, R) {
    reset_stats();
    let start = Instant::now();
    let res = f();
    let elapsed = start.elapsed();

    let total = TOTAL_ALLOCATED.load(Ordering::SeqCst);
    let peak = PEAK_ALLOCATED.load(Ordering::SeqCst);
    let count = ALLOC_COUNT.load(Ordering::SeqCst);

    (
        BenchResult {
            name,
            time_ms: elapsed.as_secs_f64() * 1000.0,
            total_allocated_kb: total as f64 / 1024.0,
            peak_allocated_kb: peak as f64 / 1024.0,
            alloc_count: count,
        },
        res,
    )
}

fn print_result(r: &BenchResult) {
    println!(
        "{:<35} | {:>9.2} ms | {:>12.2} KB allocated | {:>10.2} KB peak | {:>8} allocs",
        r.name, r.time_ms, r.total_allocated_kb, r.peak_allocated_kb, r.alloc_count
    );
}

fn main() {
    println!("=== Parts Interpreter Benchmark Suite ===\n");
    println!(
        "{:<35} | {:>12} | {:>22} | {:>15} | {:>10}",
        "Benchmark", "Time", "Total Allocated", "Peak Memory", "Allocs"
    );
    println!("{:-<105}", "");

    // 1. Native Call Loop (50k iterations)
    {
        let mut engine = Engine::new();
        engine.register_native("native_add", 2, |args| match (&args[0], &args[1]) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
            _ => Err("Expected ints".to_string()),
        });
        let script = r#"
            let s = 0;
            let i = 0;
            for i < 50000 {
                s = native_add(s, 1);
                i = i + 1;
            }
            return s;
        "#;
        let (result, val) = measure("1. Native call in loop (50k)", || {
            engine.execute(script).unwrap()
        });
        assert_eq!(val, Some(Value::Int(50000)));
        print_result(&result);
    }

    // 2. Function Body Loop (100k iterations inside function)
    {
        let engine = Engine::new();
        let script = r#"
            let compute(n) = {
                let s = 0;
                let i = 0;
                for i < n {
                    s = s + i;
                    i = i + 1;
                }
                return s;
            }
            return compute(100000);
        "#;
        let (result, val) = measure("2. Loop in function body (100k)", || {
            engine.execute(script).unwrap()
        });
        assert_eq!(val, Some(Value::Int(4999950000)));
        print_result(&result);
    }

    // 3. Tiny Function Calls (20k iterations)
    {
        let engine = Engine::new();
        let script = r#"
            let inc(x) = { return x + 1; }
            let i = 0;
            for i < 20000 {
                i = inc(i);
            }
            return i;
        "#;
        let (result, val) = measure("3. Tiny function calls (20k)", || {
            engine.execute(script).unwrap()
        });
        assert_eq!(val, Some(Value::Int(20000)));
        print_result(&result);
    }

    // 4. Recursive Fibonacci (fib(22))
    {
        let engine = Engine::new();
        let script = r#"
            let fib(n) = {
                if n <= 1 { return n; }
                return fib(n - 1) + fib(n - 2);
            }
            return fib(22);
        "#;
        let (result, val) = measure("4. Recursive fibonacci (n=22)", || {
            engine.execute(script).unwrap()
        });
        assert_eq!(val, Some(Value::Int(17711)));
        print_result(&result);
    }

    // 5. Compiler & Emitter workload (100 iterations of compile + emit)
    {
        let script = r#"
            let add(a, b) = { return a + b; }
            let mul(a, b) = { return a * b; }
            let obj = |> x: 10, y: 20 <|;
            let compute(x, y) = {
                let sum = add(x, y);
                let prod = mul(x, y);
                return |> sum: sum, prod: prod <|;
            }
            return compute(5, 10);
        "#;
        let (result, _) = measure("5. Compile & Emit (x100)", || {
            for _ in 0..100 {
                let mut parser = Parser::new(script.to_string());
                let mut ast = parser.parse_all().unwrap();
                let mut optimizer = AstOptimizer::new();
                optimizer.collect_all(&ast);
                let mut changed = true;
                while changed {
                    changed = optimizer.optimize_all(&mut ast);
                }
                let mut compiler = Compiler::new("./".into());
                compiler.optimize = true;
                let ir = compiler.compile_all(ast).unwrap();
                let ir = IrOptimizer::optimize(ir);
                let _bc = Emitter {}.emit(ir);
            }
        });
        print_result(&result);
    }

    // 6. Enum construct & Pattern Match in loop (50k)
    {
        let engine = Engine::new();
        let script = r#"
            enum Shape {
                Circle(r),
                Rect(w, h)
            }
            let s = 0;
            let i = 0;
            for i < 50000 {
                let c = Shape::Circle(i);
                let area = match c {
                    Shape::Circle(r) => r * 2,
                    Shape::Rect(w, h) => w * h
                };
                s = s + area;
                i = i + 1;
            }
            return s;
        "#;
        let (result, val) = measure("6. Enum construct & Match (50k)", || {
            engine.execute(script).unwrap()
        });
        assert_eq!(val, Some(Value::Int(2499950000)));
        print_result(&result);
    }

    println!("{:-<105}", "");
    println!("Benchmark completed successfully.");
}
