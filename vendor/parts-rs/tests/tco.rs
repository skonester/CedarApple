use parts::engine::Engine;
use parts::value::Value;

#[test]
fn test_tail_call_optimization_deep_recursion() {
    let engine = Engine::new();
    let source = "
        let count_down(n, acc) {
            if n <= 0 {
                return acc;
            }
            return count_down(n - 1, acc + 1);
        }
        return count_down(10000, 0);
    ";
    let result = engine.execute(source).unwrap();
    assert_eq!(result, Some(Value::Int(10000)));
}

#[test]
fn test_tail_call_accumulator_large() {
    let engine = Engine::new();
    let source = "
        let sum_tail(n, acc) {
            if n <= 0 {
                return acc;
            }
            return sum_tail(n - 1, acc + n);
        }
        return sum_tail(5000, 0);
    ";
    let result = engine.execute(source).unwrap();
    // 5000 * 5001 / 2 = 12502500
    assert_eq!(result, Some(Value::Int(12502500)));
}
