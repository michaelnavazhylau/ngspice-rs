//! Opt-in live numparam oracle for the #15 evaluator: ordering, redefinition,
//! forward references, function semantics and the failures C also reports.
//!
//! Run: `NGSPICE_BIN=/abs/path/ngspice cargo test -p ngspice-rs --test c_param_eval -- --ignored`

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use ngspice_rs::netlist::{Parser, eval::ParamScope, source::parse_deck_text};

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Runs C on `params` plus a source reading `{probe}`; `Err` when C fails.
fn c_value(label: &str, params: &str, probe: &str) -> Result<f64, String> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join(".");
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
        std::env::temp_dir().join(format!("spice-eval-{label}-{}-{nonce}", std::process::id())),
    );
    fs::create_dir(&scratch.0).unwrap();
    let deck = format!(
        "eval probe\n{params}\nv1 1 0 dc {{{probe}}}\nr1 1 0 1\n\
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

fn rust_value(params: &str, probe: &str) -> Result<f64, String> {
    let text = format!("t\n{params}\n.param probe__={{{probe}}}\n.end\n");
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("probe.cir"), &text))
        .map_err(|e| e.to_string())?;
    let scope = ParamScope::root(&netlist.params).map_err(|e| e.to_string())?;
    Ok(scope.get("probe__").unwrap())
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p ngspice-rs --test c_param_eval -- --ignored"]
fn evaluator_agrees_with_c_numparam() {
    let cases: &[(&str, &str)] = &[
        // Redefinition: last wins, earlier dropped.
        (".param a=1\n.param a=2", "a"),
        (".param a=1 a=3 b={a}", "b"),
        // Forward reference and hoisting across cards.
        (".param b={a}\n.param a=2", "b"),
        (".param a=1\n.param b={a*2}\n.param a=10", "b"),
        (".param c={b+1}\n.param b={a+1}\n.param a=1", "c"),
        // Case-insensitive names.
        (".param A=2", "a+1"),
        // Function semantics.
        (".param a=2 b=3", "(-a)^2"),
        (".param a=2 b=3", "2^3^2"),
        (".param a=2 b=3", "-a^2"),
        (".param a=2 b=3", "pwr(-3,b)"),
        (".param a=2 b=3", "pow(-3,a)"),
        (
            ".param a=2 b=3",
            "int(-2.7)+nint(2.5)*10+nint(3.5)*100+nint(-2.5)*1000",
        ),
        (".param a=2 b=3", "sgn(-a)*100+sgn(0)*10+sgn(b)"),
        (".param a=2 b=3", "ceil(1.2)+floor(-1.2)*10"),
        (
            ".param a=2 b=3",
            "ln(exp(a))+log(exp(b))*10+log10(1000)*100",
        ),
        (
            ".param a=2 b=3",
            "sqr(b)+sqrt(16)+abs(-a)+max(a,b)+min(a,b)",
        ),
        (
            ".param a=0.5",
            "asin(a)+acos(a)+atan(a)+arctan(a)+sinh(a)+cosh(a)+tanh(a)+tan(a)",
        ),
        (".param a=2", "asinh(a)+acosh(a)+atanh(a/4)"),
    ];
    for (index, (params, probe)) in cases.iter().enumerate() {
        let c = c_value(&format!("ok{index}"), params, probe)
            .unwrap_or_else(|e| panic!("{params} {probe}: {e}"));
        let r = rust_value(params, probe).unwrap_or_else(|e| panic!("{params} {probe}: {e}"));
        assert!(
            (c - r).abs() <= 1e-12 * c.abs().max(1.0),
            "{params} {probe}: C {c} Rust {r}"
        );
    }
}

#[test]
#[ignore = "requires NGSPICE_BIN; run cargo test -p ngspice-rs --test c_param_eval -- --ignored"]
fn failures_c_reports_are_errors_here_too() {
    let cases: &[(&str, &str)] = &[
        // Self reference after a dropped earlier definition is undefined.
        (".param a=1\n.param a={a+1}", "a"),
        (".param c={zz+1}", "c"),
        // A cycle is fatal in C.
        (".param a={b}\n.param b={a}", "a"),
        // Unused undefined names are still errors in C.
        (".param c={zz+1}", "1"),
    ];
    for (index, (params, probe)) in cases.iter().enumerate() {
        assert!(
            c_value(&format!("bad{index}"), params, probe).is_err(),
            "C accepted {params}"
        );
        assert!(rust_value(params, probe).is_err(), "Rust accepted {params}");
    }
    // Deliberate divergence: C yields inf/nan (an unused `1/0` runs fine), the
    // port refuses non-finite values.
    let unused = c_value("unusedinf", ".param c={1/0}", "3").unwrap();
    assert!((unused - 3.0).abs() < 1e-9, "{unused}");
    assert!(rust_value(".param c={1/0}", "3").is_err());
    assert!(rust_value(".param c={sqrt(-1)}", "3").is_err());
}
