//! #15: ordered `.param` resolution, diagnostics and literalization.
use ngspice_rs::netlist::ast::{Netlist, ParameterKind};
use ngspice_rs::netlist::elaborate::{SiteKind, literalize};
use ngspice_rs::netlist::eval::{EvalBudget, EvalLimits, ParamBinding, ParamScope, ParamState};
use ngspice_rs::netlist::{Parser, source::parse_deck_text};
use std::path::Path;
use std::sync::Arc;

fn deck(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("e.cir"),
            &format!("t\n{body}\n.end\n"),
        ))
        .unwrap()
}

fn scope(body: &str) -> Result<ParamScope, String> {
    ParamScope::root(&deck(body).params).map_err(|e| e.to_string())
}

fn value(body: &str, name: &str) -> f64 {
    scope(body).unwrap().get(name).unwrap()
}

#[test]
fn forward_references_and_last_definition_wins() {
    assert_eq!(value(".param b={a*2}\n.param a=3", "b"), 6.0);
    // Earlier a=1 is dropped, so b sees a=10 (C inp_sort_params).
    assert_eq!(value(".param a=1\n.param b={a*2}\n.param a=10", "b"), 20.0);
    assert_eq!(value(".param a=1 a=3 b={a}", "b"), 3.0);
    assert_eq!(value(".param A=2", "a"), 2.0);
    let s = scope(".param a=1\n.param a=2").unwrap();
    assert_eq!(s.entries().len(), 2);
    assert!(matches!(
        s.entries()[0].state,
        ParamState::Superseded { .. }
    ));
    assert_eq!(s.entries()[1].state, ParamState::Resolved(2.0));
    // A superseded definition is not evaluated at all.
    assert_eq!(value(".param a={zz}\n.param a=2", "a"), 2.0);
    // Chains resolve regardless of card order.
    assert_eq!(
        value(".param c={b+1}\n.param b={a+1}\n.param a=1", "c"),
        3.0
    );
}

#[test]
fn self_reference_cycle_and_undefined_are_errors_with_chains() {
    let e = scope(".param a=1\n.param a={a+1}").unwrap_err();
    assert!(
        e.contains("undefined parameter 'a'") && e.contains("e.cir:3:"),
        "{e}"
    );
    let e = scope(".param a={b}\n.param b={c}\n.param c={a}").unwrap_err();
    assert!(e.contains("circular parameter definition"), "{e}");
    assert!(
        e.contains("'a' (e.cir:2:8) -> 'b'") && e.contains("-> 'a' ("),
        "{e}"
    );
    let e = scope(".param c={b+1}\n.param b={zz*2}").unwrap_err();
    assert!(e.contains("undefined parameter 'zz'"), "{e}");
    assert!(e.contains("required by parameter 'c'"), "{e}");
    assert!(e.starts_with("e.cir:3:"), "{e}");
}

#[test]
fn domain_errors_and_nonfinite_results_are_explicit() {
    for (text, needle) in [
        ("1/0", "division by zero"),
        ("0/0", "division by zero"),
        ("sqrt(-1)", "domain error"),
        ("ln(0)", "overflow or pole"),
        ("log10(-2)", "domain error"),
        ("asin(2)", "domain error"),
        ("acosh(0)", "domain error"),
        ("atanh(1)", "overflow or pole"),
        ("exp(1000)", "overflow or pole"),
        ("1e200*1e200", "overflow or pole"),
        ("0^-1", "overflow or pole"),
        ("pow(-8,0.5)", "domain error"),
        ("sqr(1e200)", "overflow or pole"),
    ] {
        let e = scope(&format!(".param p={{{text}}}")).unwrap_err();
        assert!(e.contains(needle), "{text}: {e}");
        assert!(e.starts_with("e.cir:2:"), "{text}: {e}");
        assert!(e.contains("parameter 'p'"), "{text}: {e}");
    }
}

#[test]
fn function_semantics_follow_xpressn() {
    for (text, want) in [
        ("(-2)^2", 4.0),
        ("-2^2", -4.0),
        ("2^3^2", 64.0),
        ("(-8)^2", 64.0), // pow(fabs(x), y)
        ("pwr(-3,2)", 9.0),
        ("pow(-3,2)", 9.0),
        ("int(-2.7)", -2.0),
        ("int(2.7)", 2.0),
        ("nint(2.5)", 2.0),
        ("nint(3.5)", 4.0),
        ("nint(-2.5)", -2.0),
        ("sgn(-4)", -1.0),
        ("sgn(0)", 0.0),
        ("sgn(7)", 1.0),
        ("ceil(1.2)", 2.0),
        ("floor(-1.2)", -2.0),
        ("max(1,2)", 2.0),
        ("min(1,2)", 1.0),
        ("ln(exp(1))", 1.0),
        ("log(exp(2))", 2.0),
        ("log10(1000)", 3.0),
        ("sqr(3)", 9.0),
        ("abs(-3)", 3.0),
        ("1k+1meg", 1_001_000.0),
    ] {
        let got = value(&format!(".param p={{{text}}}"), "p");
        assert!(
            (got - want).abs() <= 1e-12 * want.abs().max(1.0),
            "{text}: {got}"
        );
    }
}

#[test]
fn work_is_bounded() {
    let n = deck(".param a=1\n.param b={a+a+a+a}");
    let mut budget = EvalBudget::new(EvalLimits {
        max_nodes: 3,
        ..EvalLimits::default()
    });
    let e = ParamScope::resolve(None, &[], &n.params, &mut budget).unwrap_err();
    assert!(e.to_string().contains("budget"), "{e}");
    let mut budget = EvalBudget::new(EvalLimits {
        max_definitions: 1,
        ..EvalLimits::default()
    });
    assert!(ParamScope::resolve(None, &[], &n.params, &mut budget).is_err());
    let long = format!(".param p={{{}1}}", "1+".repeat(2000));
    let e = scope(&long).unwrap_err();
    assert!(e.contains("deeper than"), "{e}");
}

#[test]
fn child_scopes_see_parents_and_bindings_without_flattening() {
    let root = Arc::new(scope(".param base=10").unwrap());
    let sub = deck(".param local={w*base}");
    let bindings = [ParamBinding {
        name: "w".into(),
        value: 3.0,
        location: sub.location.clone(),
    }];
    let child = ParamScope::resolve(
        Some(Arc::clone(&root)),
        &bindings,
        &sub.params,
        &mut EvalBudget::default(),
    )
    .unwrap();
    assert_eq!(child.get("local"), Some(30.0));
    assert_eq!(child.get("base"), Some(10.0));
    assert_eq!(root.get("local"), None);
    // A body definition may shadow a parent name and still reference it.
    let shadow = deck(".param base={base+1}");
    let child =
        ParamScope::resolve(Some(root), &[], &shadow.params, &mut EvalBudget::default()).unwrap();
    assert_eq!(child.get("base"), Some(11.0));
    // Redefining a bound formal is explicit.
    let again = deck(".param w=1");
    assert!(
        ParamScope::resolve(None, &bindings, &again.params, &mut EvalBudget::default()).is_err()
    );
}

#[test]
fn an_instance_scope_lets_an_override_outrank_a_body_param() {
    // C: numparam.spicenum.c — the instance value wins, so `resolve`'s strict
    // "bound value redefined" error would reject this legitimate deck.
    let root = Arc::new(scope(".param base=5k").unwrap());
    let overrides = [ParamBinding {
        name: "rval".into(),
        value: 3.0,
        location: deck(".param rval=3").params[0].location.clone(),
    }];
    let body = deck(".param base={base*2}\n.param rval=9");
    let child = ParamScope::resolve_instance(
        Some(Arc::clone(&root)),
        &overrides,
        &body.params,
        &mut EvalBudget::default(),
    )
    .unwrap();
    assert_eq!(child.get("rval"), Some(3.0), "the override wins");
    assert_eq!(
        child.get("base"),
        Some(10_000.0),
        "the body still sees the parent"
    );
    let dropped = child
        .entries()
        .iter()
        .find(|entry| entry.source.as_deref() == Some("9"))
        .expect("the redefining card is kept visible");
    assert!(matches!(dropped.state, ParamState::Superseded { .. }));
    // Without an override, the same card resolves normally.
    let plain =
        ParamScope::resolve_instance(Some(root), &[], &body.params, &mut EvalBudget::default())
            .unwrap();
    assert_eq!(plain.get("rval"), Some(9.0));
}

#[test]
fn literalize_replaces_sites_but_keeps_original_text_and_terminals() {
    let n = deck(
        ".param r=2k half={r/2}\nr1 1 2 {r}\nc1 2 0 cm {half/1meg}\n.model cm c cap=1p\n.tran {half/1meg} 1m\n.dc r1 1 {r} 1",
    );
    let e = literalize(&n).unwrap();
    // Input untouched.
    assert!(matches!(
        n.devices[0].parameters[0].kind,
        ParameterKind::Expression(_)
    ));
    let d = &e.netlist.devices[0];
    assert_eq!(d.nodes, ["1", "2"]);
    assert_eq!(d.parameters[0].kind, ParameterKind::Scalar);
    assert_eq!(d.parameters[0].value, "2000");
    assert_eq!(e.netlist.devices[1].model.as_deref(), Some("cm"));
    assert_eq!(e.netlist.analyses[0].arguments, ["1e-3", "1m"]);
    assert!(e.netlist.analyses[0].expressions.is_empty());
    assert_eq!(e.netlist.analyses[1].arguments[2], "2000");
    assert_eq!(e.netlist.params, n.params);
    assert_eq!(e.sites.len(), 4);
    assert_eq!(e.sites[0].original, "{r}");
    assert_eq!(e.sites[0].value, 2000.0);
    assert_eq!(
        e.sites[0].kind,
        SiteKind::DeviceParameter {
            device: 0,
            parameter: 0
        }
    );
    assert_eq!(e.sites[0].location.line, 3);
    // Idempotent on its own output.
    assert_eq!(literalize(&e.netlist).unwrap().netlist, e.netlist);
}

#[test]
fn site_errors_carry_card_context() {
    let n = deck("r1 1 0 {nope}");
    let e = literalize(&n).unwrap_err().to_string();
    assert!(
        e.contains("undefined parameter 'nope'") && e.contains("parameter 'resistance'"),
        "{e}"
    );
    assert!(e.starts_with("e.cir:2:"), "{e}");
    let n = deck(".param z=0\nr1 1 0 {1/z}\n");
    let e = literalize(&n).unwrap_err().to_string();
    assert!(e.contains("division by zero"), "{e}");
    let n = deck(".param z=0\nv1 1 0 1\nr1 1 0 1\n.tran {1/z} 1");
    assert!(
        literalize(&n)
            .unwrap_err()
            .to_string()
            .contains("argument 1")
    );
}
