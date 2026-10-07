//! `.param` assignments and parameter-expression syntax (GitHub #14).
//!
//! Syntax only: nothing is evaluated. Precedence expectations follow
//! `xpressn.c` and are cross-checked against the C binary by the opt-in
//! `c_param_reference` test.

use std::path::Path;

use spice_core::{SourceLoc, SpiceError, parse_spice_number};
use spice_netlist::{
    Parser,
    ast::{Netlist, ParameterKind, ScopedCardKind},
    expr::{BinaryOp, Expr, ExprKind, Function, ParameterExpression, UnaryOp},
    source::parse_deck_text,
};

fn parse(body: &str) -> Result<Netlist, SpiceError> {
    Parser::new().parse_deck(&parse_deck_text(
        Path::new("param.cir"),
        &format!("Title\n{body}\n"),
    ))
}

/// Expression text at line 5, column 10 so offsets are visible in columns.
fn expression(text: &str) -> Result<ParameterExpression, SpiceError> {
    Parser::new().parse_expression(
        text,
        &SourceLoc::new(Path::new("e.cir").to_path_buf(), 5, 10),
    )
}

fn sexp(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Number { spelling, .. } => spelling.clone(),
        ExprKind::Identifier(name) => name.clone(),
        ExprKind::Unary { op, operand } => {
            let op = match op {
                UnaryOp::Plus => "pos",
                UnaryOp::Minus => "neg",
            };
            format!("({op} {})", sexp(operand))
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let op = match op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
                BinaryOp::Pow => "^",
            };
            format!("({op} {} {})", sexp(lhs), sexp(rhs))
        }
        ExprKind::Call {
            function,
            arguments,
        } => {
            let arguments: Vec<_> = arguments.iter().map(sexp).collect();
            format!("{}({})", function.name(), arguments.join(", "))
        }
        ExprKind::Group(inner) => format!("[{}]", sexp(inner)),
    }
}

fn shape(text: &str) -> String {
    sexp(
        &expression(text)
            .unwrap_or_else(|e| panic!("{text}: {e}"))
            .root,
    )
}

#[test]
fn precedence_and_associativity_follow_c_numparam() {
    for (text, expected) in [
        ("1+2*3", "(+ 1 (* 2 3))"),
        ("1*2+3", "(+ (* 1 2) 3)"),
        ("1-2-3", "(- (- 1 2) 3)"),
        ("2/4/2", "(/ (/ 2 4) 2)"),
        ("a*b/c", "(/ (* a b) c)"),
        // Exponent: tighter than * and /, left associative (C: 2^3^2 = 64).
        ("2*3^2", "(* 2 (^ 3 2))"),
        ("2^3^2", "(^ (^ 2 3) 2)"),
        ("2**3**2", "(^ (^ 2 3) 2)"),
        ("2^3*4", "(* (^ 2 3) 4)"),
        ("2 ^ 3 ** 2", "(^ (^ 2 3) 2)"),
        // A leading sign has additive weight: -2^2 is -4, -a*b+1 is -(a*b)+1.
        ("-2^2", "(neg (^ 2 2))"),
        ("-a^2", "(neg (^ a 2))"),
        ("-a*b+1", "(+ (neg (* a b)) 1)"),
        ("+3", "(pos 3)"),
        ("- 3", "(neg 3)"),
        ("-(3)", "(neg [3])"),
        // After a binary operator `-` binds to the literal only (C `negate`).
        ("2*-3^2", "(* 2 (^ (neg 3) 2))"),
        ("2^-1", "(^ 2 (neg 1))"),
        ("2--3", "(- 2 (neg 3))"),
        ("1+-2", "(+ 1 (neg 2))"),
        ("--3", "(neg (neg 3))"),
        ("-2*-3", "(neg (* 2 (neg 3)))"),
        ("(1+2)*3", "(* [(+ 1 2)] 3)"),
        ("2^(1+1)", "(^ 2 [(+ 1 1)])"),
        // Names are case-folded; calls, whitespace and nesting.
        ("A+B_2.x", "(+ a b_2.x)"),
        ("max(1, 2*3)", "max(1, (* 2 3))"),
        ("SQRT (16)+pow(2,3)^2", "(+ sqrt([16]) (^ pow(2, 3) 2))"),
        ("sin(cos(a))", "sin(cos(a))"),
        ("  1 +  2 ", "(+ 1 2)"),
    ] {
        // Calls print arguments without the group brackets.
        let expected = expected.replace("sqrt([16])", "sqrt(16)");
        assert_eq!(shape(text), expected, "{text}");
    }
}

#[test]
fn scale_factors_units_and_literal_spellings_are_kept() {
    for text in [
        "1k", "1K", "2.5meg", "2.5MEG", "1m", "1mil", "5V", "1kohm", ".5", "1e3", "3u", "10n",
        "7p", "4f", "2a", "1t", "1g", "1.", "1e-3",
    ] {
        let parsed = expression(text).unwrap();
        let ExprKind::Number { value, spelling } = &parsed.root.kind else {
            panic!("{text}: {:?}", parsed.root);
        };
        assert_eq!(*value, parse_spice_number(text).unwrap(), "{text}");
        assert_eq!(spelling, text);
        assert_eq!(parsed.text, text);
        assert!(!parsed.braced);
    }
    // `m` is milli, `meg` mega: the suffix does not leak into the operator.
    assert_eq!(shape("1m*1meg"), "(* 1m 1meg)");
}

#[test]
fn spans_and_original_text_are_byte_exact() {
    let parsed = expression("  a +  2*max(b,1)  ").unwrap();
    assert_eq!(parsed.text, "  a +  2*max(b,1)  ");
    assert_eq!(
        (parsed.span.start.column, parsed.span.end.column),
        (10, 10 + 19)
    );
    // The tree span excludes surrounding whitespace.
    assert_eq!(parsed.root.span.start.column, 12);
    assert_eq!(parsed.root.span.end.column, 10 + 17);
    let ExprKind::Binary { rhs, .. } = &parsed.root.kind else {
        panic!("{:?}", parsed.root);
    };
    assert_eq!((rhs.span.start.column, rhs.span.end.column), (17, 27));
    assert_eq!(rhs.span.len(), 10);
    assert_eq!(parsed.span.start.line, 5);
    assert_eq!(parsed.references(), ["a", "b"]);
    assert!(
        matches!(&rhs.kind, ExprKind::Binary { rhs, .. } if matches!(
            &rhs.kind,
            ExprKind::Call { function: Function::Max, arguments } if arguments.len() == 2
        ))
    );
}

#[test]
fn every_allowlisted_function_has_its_documented_arity() {
    for function in Function::ALL {
        let args = vec!["1"; function.arity()].join(",");
        let text = format!("{}({args})", function.name());
        assert_eq!(
            shape(&text),
            format!("{}({})", function.name(), args.replace(',', ", "))
        );
        // Case-insensitive lookup.
        assert_eq!(
            Function::from_name(&function.name().to_ascii_uppercase()),
            Some(*function)
        );
        let wrong = vec!["1"; function.arity() + 1].join(",");
        let error = expression(&format!("{}({wrong})", function.name())).unwrap_err();
        assert!(error.to_string().contains("argument"), "{error}");
    }
    let pow2: Vec<_> = Function::ALL
        .iter()
        .filter(|f| f.arity() == 2)
        .map(|f| f.name())
        .collect();
    assert_eq!(pow2, ["pow", "pwr", "max", "min"]);
}

#[test]
fn malformed_expressions_commit_byte_column_errors() {
    // Column = 10 + byte offset in the expression text.
    for (text, column, message) in [
        ("", 10, "empty expression"),
        ("   ", 10, "empty expression"),
        ("1+", 12, "expected an operand"),
        ("1 + ", 14, "expected an operand"),
        ("*2", 10, "unexpected '*'"),
        ("1**", 13, "expected an operand"),
        ("(1+2", 14, "expected ')'"),
        ("1+2)", 13, "unmatched ')'"),
        ("()", 11, "expected an operand, found ')'"),
        ("max(1,)", 16, "expected an operand, found ')'"),
        ("max(1,2", 17, "expected ')'"),
        ("2*+3", 12, "unexpected '+'"),
        ("2*-a", 13, "directly before a numeric literal"),
        ("2*-(3)", 13, "directly before a numeric literal"),
        ("--a", 12, "directly before a numeric literal"),
        ("a b", 12, "unexpected 'b'"),
        ("3 k", 12, "unexpected 'k'"),
        ("1 2", 12, "unexpected '2'"),
        ("a,b", 11, "unexpected ','"),
        ("1.2.3", 13, "unexpected '.'"),
        ("a{b}", 11, "nested '{'"),
        ("1e999", 10, "overflows"),
        ("2*1e999", 12, "overflows"),
        ("1e999k", 10, "overflows"),
        ("pow(1)", 10, "takes 2 argument(s), found 1"),
        ("sqrt(1,2)", 10, "takes 1 argument(s), found 2"),
        ("sqrt", 10, "requires an argument list"),
        ("foo bar", 14, "unexpected 'b'"),
        ("a#b", 11, "unexpected '#'"),
    ] {
        let error = expression(text).unwrap_err();
        match &error {
            SpiceError::Parse {
                location,
                message: found,
            } => {
                assert_eq!(location.line, 5, "{text}");
                assert_eq!(location.column, column, "{text}: {error}");
                assert!(found.contains(message), "{text}: {found}");
            }
            other => panic!("{text}: expected Parse, got {other}"),
        }
    }
}

#[test]
fn valid_numparam_outside_the_subset_is_not_yet_ported() {
    for (text, fragment) in [
        ("a<b", "operator '<'"),
        ("a==b", "operator '='"),
        ("a&&b", "operator '&'"),
        ("a?b:c", "operator '?'"),
        ("a%b", "operator '%'"),
        ("a\\b", "operator '\\'"),
        ("!a", "operator '!'"),
        ("a>=1", "operator '>'"),
        ("agauss(1,2,3)", "outside the bounded allowlist"),
        ("unif(1,2)", "outside the bounded allowlist"),
        ("ternary_fcn(1,2,3)", "outside the bounded allowlist"),
        ("vec(x)", "outside the bounded allowlist"),
        ("myfunc(1)", "not in the bounded function allowlist"),
        ("'1+2'", "quoted"),
        ("\"s\"", "quoted"),
    ] {
        let error = expression(text).unwrap_err();
        assert!(error.is_not_yet_ported(), "{text}: {error}");
        let rendered = error.to_string();
        assert!(rendered.contains("e.cir:5:"), "{rendered}");
        assert!(rendered.contains(fragment), "{text}: {rendered}");
        assert!(rendered.contains("xpressn.c"), "{rendered}");
    }
}

#[test]
fn nesting_is_bounded_not_recursive_without_limit() {
    let deep = format!("{}1{}", "(".repeat(64), ")".repeat(64));
    assert!(expression(&deep).is_ok());
    let too_deep = format!("{}1{}", "(".repeat(65), ")".repeat(65));
    let error = expression(&too_deep).unwrap_err();
    assert!(error.to_string().contains("nesting limit"), "{error}");
    let calls = format!("{}1{}", "abs(".repeat(100), ")".repeat(100));
    assert!(expression(&calls).is_err());
}

#[test]
fn param_card_keeps_ordered_assignments_and_duplicates() {
    let n = parse(".param A=1 b = 2k, c={a + b*2}\n.PARAM a=3 d=-1\n.param e=(1+2)*3 f=sqrt(b)+1")
        .unwrap();
    assert_eq!(n.params.len(), 3);
    let names: Vec<Vec<&str>> = n
        .params
        .iter()
        .map(|card| card.assignments.iter().map(|a| a.name.as_str()).collect())
        .collect();
    assert_eq!(names, [vec!["a", "b", "c"], vec!["a", "d"], vec!["e", "f"]]);
    let first = &n.params[0];
    assert_eq!(first.location.line, 2);
    assert_eq!(
        first
            .assignments
            .iter()
            .map(|a| (a.expression.text.as_str(), a.expression.braced))
            .collect::<Vec<_>>(),
        [("1", false), ("2k", false), ("a + b*2", true)]
    );
    // Byte columns: `.param A=1 b = 2k, c={a + b*2}`
    let columns: Vec<_> = first
        .assignments
        .iter()
        .map(|a| {
            (
                a.name_span.start.column,
                a.name_span.end.column,
                a.expression.span.start.column,
                a.expression.span.end.column,
            )
        })
        .collect();
    assert_eq!(
        columns,
        [(8, 9, 10, 11), (12, 13, 16, 18), (20, 21, 23, 30)]
    );
    assert_eq!(first.assignments[2].expression.references(), ["a", "b"]);
    assert_eq!(sexp(&n.params[1].assignments[1].expression.root), "(neg 1)");
    assert_eq!(
        sexp(&n.params[2].assignments[1].expression.root),
        "(+ sqrt(b) 1)"
    );
    assert_eq!(
        n.cards.iter().map(|c| c.kind).collect::<Vec<_>>(),
        [
            ScopedCardKind::Param(0),
            ScopedCardKind::Param(1),
            ScopedCardKind::Param(2),
            ScopedCardKind::End
        ]
        .into_iter()
        .take(3)
        .collect::<Vec<_>>()
    );
}

#[test]
fn param_cards_join_continuation_lines_with_joined_card_columns() {
    let n = parse(".param a=1\n+ b={2 +\n+ a}\n+ c=3").unwrap();
    let card = &n.params[0];
    assert_eq!(card.location.line, 2);
    assert_eq!(
        card.assignments
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    // Joined text: `.param a=1 b={2 + a} c=3`; columns are joined-card bytes.
    let b = &card.assignments[1];
    assert_eq!(b.expression.text, "2 + a");
    assert_eq!(b.expression.span.start.column, 15);
    assert_eq!(b.name_span.start.column, 12);
    assert_eq!(card.assignments[2].name_span.start.column, 22);
}

#[test]
fn param_extent_follows_the_c_multi_assignment_split() {
    // Whitespace ends an unbraced value outside (); inside () and {} it does not.
    let n = parse(".param a=(1 + 2) b={ 3 * 4 } c = 5").unwrap();
    let texts: Vec<_> = n.params[0]
        .assignments
        .iter()
        .map(|a| a.expression.text.as_str())
        .collect();
    assert_eq!(texts, ["(1 + 2)", " 3 * 4 ", "5"]);
    // Braced text keeps inner whitespace; the tree span does not.
    let braced = &n.params[0].assignments[1].expression;
    assert_eq!(braced.root.span.start.column, braced.span.start.column + 1);
    // C parses `a = 1 + 2` on a lone line, but its multi-assignment splitter
    // would cut at the space: the port requires braces and says so.
    let error = parse(".param a = 1 + 2").unwrap_err();
    match error {
        SpiceError::Parse { location, message } => {
            assert_eq!((location.line, location.column), (2, 14));
            assert!(message.contains("name = expression"), "{message}");
            assert!(message.contains("{...}"), "{message}");
        }
        other => panic!("{other}"),
    }
}

#[test]
fn param_diagnostics_are_committed_with_byte_columns() {
    for (card, column, message) in [
        (".param", 7, "name=expression"),
        (".param a", 9, "expected '=' after parameter name 'a'"),
        (".param a =", 11, "expected an expression after '='"),
        (".param a=1 b", 13, "expected '=' after parameter name 'b'"),
        (".param 1a=2", 8, "expected a parameter name"),
        (".param =2", 8, "expected a parameter name"),
        (".param a=1 +2", 12, "name = expression"),
        (".param a=1 ) b=2", 12, "unmatched ')'"),
        (".param a=1+", 12, "expected an operand"),
        (".param a={1+}", 13, "expected an operand"),
        (".param a={}", 11, "empty expression"),
        (".param a={ }", 11, "empty expression"),
        (".param a=1e999", 10, "overflows"),
        (".param a=2 b={x*1e999}", 17, "overflows"),
        (".param a=(1+2", 14, "expected ')'"),
        (".param a=1 2", 12, "name = expression"),
        (".param a=1@2", 11, "unexpected '@'"),
        (".param a=1{b}", 11, "name = expression"),
        (".param a=sqrt()", 15, "expected an operand, found ')'"),
        (".param a=pow(2)", 10, "takes 2 argument(s)"),
    ] {
        match parse(card).unwrap_err() {
            SpiceError::Parse {
                location,
                message: found,
            } => {
                assert_eq!(location.line, 2, "{card}");
                assert_eq!(location.column, column, "{card}: {found}");
                assert!(found.contains(message), "{card}: {found}");
            }
            other => panic!("{card}: expected Parse, got {other}"),
        }
    }
}

#[test]
fn unmatched_braces_are_tokenizer_errors_with_columns() {
    for (card, column, message) in [
        (".param a={1+2", 10, "unterminated '{'"),
        (".param a=1}", 11, "unmatched '}'"),
        ("R1 a 0 }", 8, "unmatched '}'"),
        ("R1 a 0 {1", 8, "unterminated '{'"),
        (".tran {1u 10u", 7, "unterminated '{'"),
    ] {
        match parse(card).unwrap_err() {
            SpiceError::Parse {
                location,
                message: found,
            } => {
                assert_eq!((location.line, location.column), (2, column), "{card}");
                assert!(found.contains(message), "{card}: {found}");
            }
            other => panic!("{card}: expected Parse, got {other}"),
        }
    }
}

#[test]
fn quoted_param_values_and_unsupported_syntax_are_not_silently_dropped() {
    for card in [
        ".param a='1+2'",
        ".param s=\"text\"",
        ".param a=1<2",
        ".param a=f(1)",
    ] {
        let error = parse(card).unwrap_err();
        assert!(error.is_not_yet_ported(), "{card}: {error}");
        assert!(error.to_string().contains("param.cir:2:"), "{error}");
    }
}

#[test]
fn param_cards_in_subcircuit_scopes_stay_local() {
    let n = parse(
        ".param top=1\n.subckt s a b params: w=2\n.param inner=top*2 w={w+1}\nr1 a b 1k\n.ends s\n.param top2=2\n.end\n.param ignored=1",
    )
    .unwrap();
    assert_eq!(n.params.len(), 2);
    assert_eq!(n.params[1].assignments[0].name, "top2");
    let sub = &n.subcircuits[0];
    assert_eq!(sub.params.len(), 1);
    assert_eq!(
        sub.params[0]
            .assignments
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        ["inner", "w"]
    );
    assert_eq!(
        sub.cards.iter().map(|c| c.kind).collect::<Vec<_>>(),
        [
            ScopedCardKind::Param(0),
            ScopedCardKind::Device(0),
            ScopedCardKind::Ends
        ]
    );
    assert_eq!(
        n.cards.iter().map(|c| c.kind).collect::<Vec<_>>(),
        [
            ScopedCardKind::Param(0),
            ScopedCardKind::Subcircuit(0),
            ScopedCardKind::Param(1),
            ScopedCardKind::End
        ]
    );
    // The formal parameter list is a separate, ordered assignment vector.
    assert_eq!(sub.parameters[0].name, "w");
}

#[test]
fn braced_values_parse_at_device_model_and_analysis_sites() {
    let n = parse(
        "R1 a 0 {rval}\nR2 a 0 mdl {2*x}\nR3 a 0 mdl tc1={t1} w={wd}\nC1 a 0 1u ic={v0}\n\
         V1 a 0 {vdc} ac {mag} {ph}\nV2 b 0 dc={v2}\nD1 a 0 dm {area} pj={pj}\n\
         Q1 c b e qm {qa} m={mq}\nM1 d g s b nm w={w} l=1u\n\
         .model dm d(is={isat} rs=1)\n.model mdl r\n.model qm npn\n.model nm nmos\n.tran {tstep} {tstop}\n.op",
    )
    .unwrap();
    let kind = |device: &str, name: &str| {
        n.device(device)
            .unwrap()
            .parameters
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("{device}.{name}"))
            .clone()
    };
    for (device, name, text, refs) in [
        ("r1", "resistance", "{rval}", vec!["rval"]),
        ("r2", "resistance", "{2*x}", vec!["x"]),
        ("r3", "tc1", "{t1}", vec!["t1"]),
        ("r3", "w", "{wd}", vec!["wd"]),
        ("c1", "ic", "{v0}", vec!["v0"]),
        ("v1", "dc", "{vdc}", vec!["vdc"]),
        ("v1", "acmag", "{mag}", vec!["mag"]),
        ("v1", "acphase", "{ph}", vec!["ph"]),
        ("v2", "dc", "{v2}", vec!["v2"]),
        ("d1", "area", "{area}", vec!["area"]),
        ("d1", "pj", "{pj}", vec!["pj"]),
        ("q1", "area", "{qa}", vec!["qa"]),
        ("q1", "m", "{mq}", vec!["mq"]),
        ("m1", "w", "{w}", vec!["w"]),
    ] {
        let parameter = kind(device, name);
        assert_eq!(parameter.value, text, "{device}.{name}");
        let ParameterKind::Expression(expression) = &parameter.kind else {
            panic!("{device}.{name}: {:?}", parameter.kind);
        };
        assert!(expression.braced);
        assert_eq!(expression.references(), refs, "{device}.{name}");
        assert_eq!(format!("{{{}}}", expression.text), text);
    }
    // Literals and their spellings are unchanged at the same sites.
    assert_eq!(kind("m1", "l").kind, ParameterKind::Scalar);
    assert_eq!(kind("m1", "l").value, "1u");
    let model = n.model("dm").unwrap();
    assert!(matches!(
        model.parameters[0].kind,
        ParameterKind::Expression(_)
    ));
    assert_eq!(model.parameters[1].kind, ParameterKind::Scalar);
    assert_eq!(model.level, None);
    let tran = &n.analyses[0];
    assert_eq!(tran.arguments, ["{tstep}", "{tstop}"]);
    assert_eq!(
        tran.expressions
            .iter()
            .map(|e| (e.index, e.expression.text.as_str()))
            .collect::<Vec<_>>(),
        [(0, "tstep"), (1, "tstop")]
    );
    assert!(n.analyses[1].expressions.is_empty());
}

#[test]
fn expression_terms_do_not_confuse_terminals_or_model_names() {
    // Numeric terminals stay terminals; the brace is the value.
    let n = parse("R1 1 2 {r}\nC1 2 0 cm {c}\n.model cm c").unwrap();
    assert_eq!(n.device("r1").unwrap().nodes, ["1", "2"]);
    assert_eq!(n.device("c1").unwrap().model.as_deref(), Some("cm"));
    assert_eq!(n.device("c1").unwrap().parameters.len(), 1);
    // A bare undeclared name is still not a parameter reference in a device
    // card (C only substitutes braces/quotes there): it stays an explicit gap.
    for card in [
        "R1 a 0 rval",
        "V1 a 0 dc vdd",
        "D1 a 0 dm area=asize\n.model dm d",
    ] {
        let error = parse(card).unwrap_err();
        assert!(error.is_not_yet_ported(), "{card}: {error}");
    }
    // A declared model name stays a model even when a brace follows.
    let n = parse("R1 a 0 rm {2*k}\n.model rm r").unwrap();
    assert_eq!(n.device("r1").unwrap().model.as_deref(), Some("rm"));
    // Q terminals/model: braces cannot be a substrate or model name.
    assert!(parse("Q1 c b e {m}\n.model m npn").is_err());
    // MOS has no positional values: a brace there is malformed, not a gap.
    assert!(matches!(
        parse("M1 d g s b nm {w}\n.model nm nmos"),
        Err(SpiceError::Parse { .. })
    ));
}

#[test]
fn site_expression_errors_carry_byte_columns() {
    for (card, column, message) in [
        ("R1 a 0 {1+}", 11, "expected an operand"),
        ("R1 a 0 {}", 9, "empty expression"),
        ("R1 a 0 {1e999}", 9, "overflows"),
        ("R1 a 0 1k tc1={2*}", 18, "expected an operand"),
        ("V1 a 0 ac {1 2}", 14, "unexpected '2'"),
        ("D1 a 0 dm area={(1}", 19, "expected ')'"),
        (".tran {1u*} 10u", 11, "expected an operand"),
        ("X1 a b sub w={2*l+}", 19, "expected an operand"),
    ] {
        match parse(card).unwrap_err() {
            SpiceError::Parse {
                location,
                message: found,
            } => {
                assert_eq!(
                    (location.line, location.column),
                    (2, column),
                    "{card}: {found}"
                );
                assert!(found.contains(message), "{card}: {found}");
            }
            other => panic!("{card}: expected Parse, got {other}"),
        }
    }
    // Valid numparam outside the bounded subset is explicit, not dropped.
    for card in ["R1 a 0 {a<b}", "R1 a 0 {agauss(1,2,3)}", ".tran {a?b:c} 1"] {
        assert!(parse(card).unwrap_err().is_not_yet_ported(), "{card}");
    }
}

#[test]
fn subcircuit_and_x_values_use_parsed_expressions_or_stay_textual() {
    let n = parse(
        "X1 a b sub w={2*l} k=base m=4k7 s='x+1' f=min n=3\n.subckt sub a b params: w={1+1} d=dflt\n.ends",
    )
    .unwrap();
    let kinds: Vec<_> = n.devices[0]
        .parameters
        .iter()
        .map(|p| match &p.kind {
            ParameterKind::Expression(e) => format!("expr:{}", e.braced),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "expr:true",
            "expr:false",
            "Textual",
            "Textual",
            "Textual",
            "Scalar"
        ]
    );
    let formals = &n.subcircuits[0].parameters;
    assert!(matches!(&formals[0].kind, ParameterKind::Expression(e) if e.braced));
    assert!(matches!(&formals[1].kind, ParameterKind::Expression(e)
        if !e.braced && e.references() == ["dflt"]));
    // Malformed braces fail instead of being kept as opaque text.
    assert!(matches!(
        parse("X1 a b sub w={1+}"),
        Err(SpiceError::Parse { .. })
    ));
}

#[test]
fn unsupported_expression_sites_stay_explicit_gaps() {
    for card in [
        "V1 a 0 pulse({v1} 1 0)",
        "V1 a 0 pwl(0 {v1})",
        "Q1 c b e qm ic={vbe},0.1\n.model qm npn",
        ".model nm nmos level={lv}",
        ".model dm d(is='x')",
        "R1 a 0 'rval'",
    ] {
        let error = parse(card).unwrap_err();
        assert!(error.is_not_yet_ported(), "{card}: {error}");
    }
}

#[test]
fn param_fixture_parses_without_resolving_any_value() {
    let netlist = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("param_expressions.cir"),
            include_str!("../../../conformance/parser/param_expressions.cir"),
        ))
        .unwrap();
    let assignments: Vec<_> = netlist
        .params
        .iter()
        .flat_map(|card| &card.assignments)
        .map(|a| (a.name.as_str(), a.expression.text.as_str()))
        .collect();
    // Duplicate `vdd` is retained in order; nothing is folded or checked.
    assert_eq!(
        assignments,
        [
            ("vdd", "5"),
            ("rload", "2k"),
            ("scale", "1meg"),
            ("half", "vdd/2"),
            ("tau", "rload*1u"),
            ("ratio", "(vdd+1)*2^2"),
            ("vdd", "3.3"),
            ("decay", "-2^2"),
        ]
    );
    assert_eq!(
        netlist.subcircuits[0].params[0].assignments[0].name,
        "local"
    );
    // `{vdd}` is an unresolved reference, not the number 5 or 3.3.
    let dc = &netlist.device("v1").unwrap().parameters[0];
    assert_eq!((dc.name.as_str(), dc.value.as_str()), ("dc", "{vdd}"));
    assert!(matches!(dc.kind, ParameterKind::Expression(_)));
    assert_eq!(netlist.analyses[0].expressions.len(), 2);
}

#[test]
fn arbitrary_short_inputs_never_panic_or_hang() {
    const ALPHABET: [&str; 14] = [
        "1", "a", "(", ")", ",", "+", "-", "*", "^", "{", "}", ".", " ", "α",
    ];
    let mut total = 0usize;
    let mut stack: Vec<String> = vec![String::new()];
    while let Some(text) = stack.pop() {
        let _ = expression(&text);
        let _ = parse(&format!(".param p={text}"));
        let _ = parse(&format!("R1 a 0 {{{text}}}"));
        total += 1;
        if text.chars().count() < 4 {
            for piece in ALPHABET {
                stack.push(format!("{text}{piece}"));
            }
        }
    }
    assert!(total > 40_000);
}
