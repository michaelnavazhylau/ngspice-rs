//! Opt-in live numparam oracle for #107: `.func` definitions and single-quoted
//! expressions, compared value by value with the C binary, plus the failures
//! C also reports (undefined function, wrong arity, recursion).
//!
//! Run: `NGSPICE_BIN=/abs/path/ngspice cargo test -p spice-netlist --test c_func_eval -- --ignored`

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use spice_netlist::{Parser, eval::ParamScope, source::parse_deck_text};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs C on `cards` plus a source whose DC value is `value` (written as is,
/// so `{...}` or `'...'`); `Err` when C fails or never prints the node.
fn c_value(label: &str, cards: &str, value: &str) -> Result<f64, String> {
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
    let scratch = Scratch(
        std::env::temp_dir().join(format!("spice-func-{label}-{}-{nonce}", std::process::id())),
    );
    fs::create_dir(&scratch.0).unwrap();
    let deck = format!(
        "func probe\n{cards}\nv1 1 0 dc {value}\nr1 1 0 1\n\
         .control\nset numdgt=17\nop\nprint v(1)\nquit\n.endc\n.end\n"
    );
    fs::write(scratch.0.join("probe.cir"), deck).unwrap();
    let output = Command::new(binary)
        .args(["-b", "probe.cir"])
        .current_dir(&scratch.0)
        .output()
        .expect("run reference ngspice");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("v(1) = "))
        .map(|v| v.trim().parse().unwrap())
        .ok_or_else(|| format!("{stdout}{}", String::from_utf8_lossy(&output.stderr)))
}

/// The same probe through the port: `value` becomes a `.param` definition.
fn rust_value(cards: &str, value: &str) -> Result<f64, String> {
    let text = format!("t\n{cards}\n.param probe__={value}\n.end\n");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("probe.cir"), &text))
        .map_err(|e| e.to_string())?;
    let scope = ParamScope::for_netlist(&netlist).map_err(|e| e.to_string())?;
    Ok(scope.get("probe__").unwrap())
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_func_eval -- --ignored"]
fn functions_and_quotes_agree_with_c_numparam() {
    let cases: &[(&str, &str)] = &[
        // Body spellings: braces, quotes, bare, optional '='.
        (".func f(x) {x*2}", "{f(3)}"),
        (".func f(x) 'x*2'", "{f(3)}"),
        (".func f(x) x*2", "{f(3)}"),
        (".func f(x)=x*2", "{f(3)}"),
        (".func f(x) {x * 2 + 1}", "{f(3)}"),
        // Several formals, free names at the call site, zero arity.
        (".func f(x,y) {x*y+k}\n.param k=10", "{f(3,4)}"),
        (".func f() {5}", "{f()+1}"),
        // A formal shadows a parameter only inside the body.
        (".func f(x) {x*2}\n.param x=100", "{f(3)+x}"),
        (".param f=10\n.func f(x) {x+1}", "{f(3)+f}"),
        // Case-insensitive names.
        (".func F(X) {X*2}", "{f(3)}"),
        // Last definition wins; definitions are hoisted.
        (".func f(x) {x*2}\n.func f(x) {x*3}", "{f(3)}"),
        (".func f(x) {g(x)+1}\n.func g(y) {y*10}", "{f(3)}"),
        (".param p={f(2)}\n.func f(x) {x*q}\n.param q=3", "{p}"),
        (".func f(x) {x*2}", "{f(f(2))}"),
        // An enclosing call's formal captures a callee's free name.
        (
            ".func g(y) {y+x}\n.func f(x) {g(1)}\n.param x=100",
            "{f(5)}",
        ),
        // A .func replaces a built-in, also inside other bodies.
        (".func max(a,b) {a+b}", "{max(3,4)}"),
        (".func max(a,b) {a+b}\n.func f(x) {max(x,1)}", "{f(3)}"),
        (".func limit(x,a,b) {min(max(x,a),b)}", "{limit(5,1,3)}"),
        // An unused body may name an undefined parameter.
        (".func f(x) {zz*x}", "{1}"),
        // `.param name(formals) = body` is a `.func` (inp_fix_macro_param_func_paren_io).
        (".param f(x)={x*3}", "{f(3)}"),
        (".param g(x,y) = 'x+y'", "'g(3,4)'"),
        (".param h()=4", "{h()+1}"),
        (".param k(x)=x*2\n.param q={k(5)}", "{q}"),
        // Single quotes are braces.
        (".param a='1+2' b='a*2'", "{b}"),
        (".param a = '1 + 2'", "{a*2}"),
        (".func f(x) {x*2}", "'f(3)+1'"),
        (".param a=2", "'a*pwr(a,3)'"),
    ];
    for (index, (cards, value)) in cases.iter().enumerate() {
        let c = c_value(&format!("ok{index}"), cards, value)
            .unwrap_or_else(|e| panic!("{cards} {value}: {e}"));
        let r = rust_value(cards, value).unwrap_or_else(|e| panic!("{cards} {value}: {e}"));
        assert!(
            (c - r).abs() <= 1e-12 * c.abs().max(1.0),
            "{cards} {value}: C {c} Rust {r}"
        );
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_func_eval -- --ignored"]
fn function_failures_c_reports_are_errors_here_too() {
    let cases: &[(&str, &str, &str)] = &[
        ("", "{g(3)}", "undefined function 'g'"),
        (
            ".func f(x) {x*2}",
            "{f(3,4)}",
            "takes 1 argument(s), found 2",
        ),
        // Arity is checked inside an unused body as well.
        (
            ".func f(x) {g(x,1)}\n.func g(x) {x}",
            "{1}",
            "in the body of function 'f'",
        ),
        // Recursion is fatal in C (unbounded expansion), even unused.
        (".func f(x) {f(x)}", "{1}", "recursive .func definition"),
        (
            ".func f(x) {g(x)}\n.func g(x) {f(x)}",
            "{1}",
            "recursive .func definition",
        ),
        // The `.param` spelling of a definition needs its '='.
        (
            ".param f(x) {x*3}",
            "{f(3)}",
            "expected '=' after the parameter list",
        ),
        // A function's free name that is not defined where it is used.
        (".func f(x) {zz*x}", "{f(1)}", "undefined parameter 'zz'"),
    ];
    for (index, (cards, value, message)) in cases.iter().enumerate() {
        assert!(
            c_value(&format!("bad{index}"), cards, value).is_err(),
            "C accepted {cards} {value}"
        );
        let error = rust_value(cards, value).expect_err(cards);
        assert!(error.contains(message), "{cards} {value}: {error}");
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p spice-netlist --test c_func_eval -- --ignored"]
fn forms_c_accepts_but_the_port_does_not_are_not_yet_ported() {
    let cases: &[(&str, &str)] = &[
        // inp_strip_braces() glues the pieces: `x+1`.
        (".func f(x) {x}+{1}", "{f(2)}"),
        // C binds the first of two equal formals.
        (".func f(x,x) {x*2}", "{f(2,3)}"),
        // C splits the card, then rewrites `f(x)={x}` to `.func`.
        (".param a=1 f(x)={x}", "{f(3)}"),
        (".param f(x)={x} a=2", "{f(3)+a}"),
        // A different-arity redefinition of an allowlisted built-in.
        (".func max(a) {a}", "{max(3)}"),
    ];
    for (index, (cards, value)) in cases.iter().enumerate() {
        c_value(&format!("nyp{index}"), cards, value)
            .unwrap_or_else(|e| panic!("C rejected {cards} {value}: {e}"));
        let text = format!("t\n{cards}\n.param probe__={value}\n.end\n");
        let error = Parser::new()
            .parse_deck(&parse_deck_text(Path::new("probe.cir"), &text))
            .expect_err(cards);
        assert!(error.is_not_yet_ported(), "{cards}: {error}");
    }
}
