//! Opt-in live numparam oracle for the parameter-expression syntax tree.
//!
//! The port has no evaluator yet (GitHub #15). This test folds the parsed tree
//! with a deliberately tiny, test-local evaluator that mirrors `xpressn.c`
//! `operate()` for the bounded subset, then compares the result with the C
//! binary's own `.param` evaluation. Agreement therefore pins the tree shape
//! (precedence, associativity, leading-sign rule), not a Rust evaluator.
//!
//! Run: `NGSPICE_BIN=/abs/path/ngspice cargo test -p spice-netlist --test c_param_reference -- --ignored`

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use spice_core::SourceLoc;
use spice_netlist::{
    Parser,
    expr::{BinaryOp, Expr, ExprKind, Function, UnaryOp},
};

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// C `operate()` with default compatibility: `^` is `pow(fabs(x), y)`.
fn evaluate(expr: &Expr, names: &BTreeMap<&str, f64>) -> f64 {
    match &expr.kind {
        ExprKind::Number { value, .. } => *value,
        ExprKind::Identifier(name) => names[name.as_str()],
        ExprKind::Group(inner) => evaluate(inner, names),
        ExprKind::Unary { op, operand } => match op {
            UnaryOp::Plus => evaluate(operand, names),
            UnaryOp::Minus => -evaluate(operand, names),
        },
        ExprKind::Binary { op, lhs, rhs } => {
            let (x, y) = (evaluate(lhs, names), evaluate(rhs, names));
            match op {
                BinaryOp::Add => x + y,
                BinaryOp::Sub => x - y,
                BinaryOp::Mul => x * y,
                BinaryOp::Div => x / y,
                BinaryOp::Pow => x.abs().powf(y),
            }
        }
        ExprKind::Call {
            function,
            arguments,
        } => {
            let args: Vec<f64> = arguments.iter().map(|a| evaluate(a, names)).collect();
            match function {
                Function::Sqrt => args[0].sqrt(),
                Function::Abs => args[0].abs(),
                Function::Sqr => args[0] * args[0],
                Function::Pow => args[0].powf(args[1]),
                Function::Pwr => args[0].abs().powf(args[1]),
                Function::Max => args[0].max(args[1]),
                Function::Min => args[0].min(args[1]),
                other => panic!("probe evaluator lacks {}", other.name()),
            }
        }
    }
}

fn c_value(label: &str, expression: &str) -> f64 {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let binary = PathBuf::from(std::env::var_os("NGSPICE_BIN").expect("set NGSPICE_BIN"));
    let binary = if binary.is_absolute() {
        binary
    } else {
        workspace.join(binary)
    };
    let binary = binary.canonicalize().expect("reference binary exists");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "spice-param-{label}-{}-{nonce}",
        std::process::id()
    )));
    fs::create_dir(&scratch.0).unwrap();
    let deck = format!(
        "param probe\n.param a=2 b=3\nv1 1 0 dc {{{expression}}}\nr1 1 0 1\n\
         .control\nset numdgt=17\nop\nprint v(1)\nquit\n.endc\n.end\n"
    );
    fs::write(scratch.0.join("probe.cir"), deck).unwrap();
    let output = Command::new(binary)
        .args(["-b", "probe.cir"])
        .current_dir(&scratch.0)
        .output()
        .expect("run reference ngspice");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{expression}\n{stdout}\n{stderr}");
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("v(1) = "))
        .unwrap_or_else(|| panic!("{expression}: no v(1)\n{stdout}\n{stderr}"))
        .trim()
        .parse()
        .unwrap()
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_param_reference -- --ignored"]
fn expression_tree_shape_agrees_with_c_numparam() {
    let names = BTreeMap::from([("a", 2.0), ("b", 3.0)]);
    let origin = SourceLoc::new(PathBuf::from("probe.cir"), 3, 1);
    for (index, text) in [
        "1+2*3",
        "1-2-3",
        "2/4/2",
        "2^3^2",
        "2**3**2",
        "2*3^2",
        "-2^2",
        "-a^2",
        "-a*b+1",
        "-2*-3",
        "2*-3^2",
        "2^-1",
        "2--3",
        "1+-2",
        "--3",
        "+3",
        "(1+2)*3",
        "(-2)^2",
        "-(3)",
        "a^b^a",
        "2.5meg*1k",
        "5V+1m",
        "1e3/4",
        "sqrt(16)+pow(2,3)",
        "max(a,b*2)-min(1,2)",
        "pwr(-2,2)",
        "abs(-5)*sqr(3)",
        " a + b * 2 ",
    ]
    .into_iter()
    .enumerate()
    {
        let parsed = Parser::new()
            .parse_expression(text, &origin)
            .unwrap_or_else(|e| panic!("{text}: {e}"));
        let rust = evaluate(&parsed.root, &names);
        let c = c_value(&format!("e{index}"), text);
        assert!(
            (rust - c).abs() <= 1e-12 * rust.abs().max(1.0),
            "{text}: tree folds to {rust}, C numparam gives {c}"
        );
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_param_reference -- --ignored"]
fn c_rejects_the_sign_forms_the_parser_rejects() {
    let origin = SourceLoc::new(PathBuf::from("probe.cir"), 3, 1);
    for (index, text) in ["2*+3", "2*-a", "2*-(3)", "1+", "a b", "3 k"]
        .into_iter()
        .enumerate()
    {
        assert!(
            Parser::new().parse_expression(text, &origin).is_err(),
            "{text}"
        );
        let result = std::panic::catch_unwind(|| c_value(&format!("r{index}"), text));
        assert!(result.is_err(), "C unexpectedly accepted {text}");
    }
}
