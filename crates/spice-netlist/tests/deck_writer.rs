//! #20: normalized deck writer. Round trips are checked semantically (locations
//! and lexical formatting differ on re-parse) and by the writer fixed point.
use std::fs;
use std::path::{Path, PathBuf};

use spice_core::SpiceError;
use spice_netlist::ast::{Netlist, ParameterKind, PositionedValue, ScopedCardKind, SourceWaveform};
use spice_netlist::source::parse_deck_text;
use spice_netlist::{Parser, semantic_diff, semantic_eq, write_netlist};

fn parse(text: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(Path::new("in.cir"), text))
        .unwrap_or_else(|error| panic!("{error}\n{text}"))
}

/// Parse, write, re-parse; checks semantic equivalence and the fixed point.
/// Returns (written text, original, re-parsed).
fn round_trip(text: &str) -> (String, Netlist, Netlist) {
    let first = parse(text);
    let written = write_netlist(&first).unwrap_or_else(|error| panic!("{error}\n{text}"));
    let second = Parser::new()
        .parse_deck(&parse_deck_text(Path::new("out.cir"), &written))
        .unwrap_or_else(|error| panic!("{error}\n{written}"));
    if let Some(difference) = semantic_diff(&first, &second) {
        panic!("not equivalent: {difference}\n--- input\n{text}\n--- written\n{written}");
    }
    assert!(semantic_eq(&first, &second));
    let again = write_netlist(&second).unwrap();
    assert_eq!(again, written, "writer is not a fixed point");
    assert_eq!(write_netlist(&first).unwrap(), written, "not deterministic");
    (written, first, second)
}

#[test]
fn simple_deck_is_normalized_and_stable() {
    let (written, _, _) = round_trip(
        "RC divider\n\n* comment\nV1  in 0   DC 5 ; trailing\nR1 in\n+ out 1K\nr2 out GND 1k\n.tran 1u 10u\n.end\n",
    );
    assert_eq!(
        written,
        "RC divider\nv1 in 0 dc 5\nr1 in out 1K\nr2 out 0 1k\n.tran 1u 10u\n.end\n"
    );
}

#[test]
fn numeric_spelling_is_preserved() {
    let (written, first, second) = round_trip(
        "t\nv1 a 0 5V\nr1 a b 4.7KOHM\nc1 b 0 1MEG\nl1 b 0 1e-3\nd1 a 0 dm 1.50\n.model dm d(is=1.0E-14 rs=0.50)\n.end\n",
    );
    for text in ["5V", "4.7KOHM", "1MEG", "1e-3", "1.50", "1.0E-14", "0.50"] {
        assert!(written.contains(text), "{text} missing in\n{written}");
    }
    assert_eq!(first.devices[1].parameters[0].value, "4.7KOHM");
    assert_eq!(second.devices[1].parameters[0].value, "4.7KOHM");
}

#[test]
fn duplicate_setters_keep_application_order() {
    let (written, first, second) = round_trip(
        "t\nv1 c 0 dc .1\n\
         vpulse a 0 7 DC=2 AC 2 90 PULSE(0 5 1n 2n 3n 4u 5u) dc 3\n\
         dlead c 0 dm 2 OFF ic=.4 area=7 off\n\
         qfull c b 0 qm 2 off ic=.6,2 icvce=3 ic=.7,4 area=7\n\
         .model dm d\n.model qm npn\n.end\n",
    );
    let names = |n: &Netlist, i: usize| -> Vec<String> {
        n.devices[i]
            .parameters
            .iter()
            .map(|p| p.name.clone())
            .collect()
    };
    // C applies source leading DC and D/Q leading area after named setters.
    assert_eq!(
        names(&first, 1),
        ["dc", "acmag", "acphase", "pulse", "dc", "dc"]
    );
    assert_eq!(names(&first, 2), ["off", "ic", "area", "off", "area"]);
    assert_eq!(
        names(&first, 3),
        ["off", "ic", "icvce", "ic", "area", "area"]
    );
    assert_eq!(names(&first, 1), names(&second, 1));
    assert_eq!(first.devices[1].parameters[5].value, "7");
    assert_eq!(second.devices[1].parameters[5].value, "7");
    assert!(written.contains("vpulse a 0 dc 2 ac 2 90 pulse(0 5 1n 2n 3n 4u 5u) dc 3 dc 7\n"));
    assert!(written.contains("dlead c 0 dm off ic=.4 area=7 off area=2\n"));
    assert!(written.contains("qfull c b 0 qm off ic=(.6,2) icvce=3 ic=(.7,4) area=7 area=2\n"));
}

#[test]
fn passive_pre_and_post_model_scalars_keep_precedence() {
    let (written, first, second) = round_trip(
        "t\nrpre a 0 1k rm r=2k r=3k\nrpost a 0 rm 4k r=5k\nrboth a 0 7k rm 8k r=9k\n\
         rgeom a 0 rm l=4u w=2u\nrplain a 0 1k tc1=2\nrnamed a 0 tc1=2 r=1k\n\
         cpost a 0 cm 4u c=5u\nlpre a 0 1m lm inductance=2m\n\
         .model rm r\n.model cm c\n.model lm l\n.end\n",
    );
    let values = |n: &Netlist, i: usize| -> Vec<(String, String)> {
        n.devices[i]
            .parameters
            .iter()
            .map(|p| (p.name.clone(), p.value.clone()))
            .collect()
    };
    assert_eq!(
        values(&first, 1),
        [
            ("resistance".into(), "5k".into()),
            ("resistance".into(), "4k".into())
        ]
    );
    assert_eq!(values(&first, 1), values(&second, 1));
    assert!(written.contains("rpost a 0 rm resistance=5k resistance=4k\n"));
    assert!(written.contains("rplain a 0 1k tc1=2\n"));
    assert!(written.contains("rnamed a 0 tc1=2 resistance=1k\n"));
    // A model-only instance gets no invented scalar.
    assert!(written.contains("rgeom a 0 rm l=4u w=2u\n"));
    assert_eq!(first.devices[3].parameters.len(), 2);
}

#[test]
fn optional_q_substrate_stays_omitted() {
    let (written, first, second) = round_trip(
        "t\nq1 c b e qm\nq2 c b e s qm\nm1 d g s b nm w=1u\n.model qm npn\n.model nm nmos\n.end\n",
    );
    assert_eq!(first.devices[0].nodes.len(), 3);
    assert_eq!(second.devices[0].nodes, ["c", "b", "e"]);
    assert_eq!(second.devices[1].nodes.len(), 4);
    assert!(written.contains("\nq1 c b e qm\n"));
    assert!(written.contains("\nq2 c b e s qm\n"));
}

#[test]
fn waveform_omissions_stay_omitted() {
    let (written, _, second) = round_trip(
        "t\nvp a 0 PULSE(0 5)\nip a 0 pulse 0, 2m, 1n\nvw b 0 PWL(0,1,1u,2) AC\nr1 a 0 1\nr2 b 0 1\n.end\n",
    );
    assert!(written.contains("vp a 0 pulse(0 5)\n"));
    assert!(written.contains("ip a 0 pulse(0 2m 1n)\n"));
    assert!(written.contains("vw b 0 pwl(0 1 1u 2) ac 1 0\n"));
    let pulse = |n: &Netlist, i: usize| match &n.devices[i].parameters[0].kind {
        ParameterKind::Waveform(SourceWaveform::Pulse(p)) => (**p).clone(),
        other => panic!("{other:?}"),
    };
    assert!(pulse(&second, 0).delay.is_none() && pulse(&second, 0).period.is_none());
    assert_eq!(pulse(&second, 1).delay.unwrap().text, "1n");
    assert!(pulse(&second, 1).rise.is_none() && pulse(&second, 1).fall.is_none());
}

#[test]
fn mos_ic_vectors_and_flags_round_trip() {
    let (written, _, _) = round_trip(
        "t\nmfull c b 0 0 nm off w=10u l=1u ic=.1,2,-.1 icvgs=3 ic=.2,4,-.2\n\
         mpartial c b 0 0 nm off icvgs=5 icvbs=-.3 ic=.3,6 ic=.4\n\
         .model nm nmos(nmos level=1 vto=1 kp=100u)\n.end\n",
    );
    assert!(written.contains("ic=(.4)\n"));
    assert!(written.contains(".model nm nmos(nmos level=1 vto=1 kp=100u)\n"));
}

#[test]
fn model_level_spelling_and_duplicates_are_kept() {
    let (written, first, second) =
        round_trip("t\n.model nm nmos level=3.0 vto=1 level=2 vto=2\nm1 d g s b nm\n.end\n");
    assert!(written.contains(".model nm nmos(level=3.0 vto=1 level=2 vto=2)\n"));
    assert_eq!(second.models[0].level, first.models[0].level);
    assert_eq!(second.models[0].parameters[0].value, "3.0");
}

#[test]
fn scope_order_forward_models_and_nested_scopes() {
    let text = "t\nx0 a gnd outer k=2 k={base*2}\n\
        .subckt outer a b params: k=1k k=base\n.model dm d\nx1 a b inner params: t='k + 1'\n\
        .subckt inner 1 0\nd1 1 0 dm\n.ends inner\nr1 a b 1k\n.param local={k*2} z=3\n.ends outer\n\
        .subckt inner a b\nr9 a b 1\n.ends\n.op\n.end\n";
    let (written, first, second) = round_trip(text);
    let kinds = |n: &Netlist| n.cards.iter().map(|c| c.kind).collect::<Vec<_>>();
    assert_eq!(kinds(&first), kinds(&second));
    assert_eq!(
        kinds(&second),
        [
            ScopedCardKind::Device(0),
            ScopedCardKind::Subcircuit(0),
            ScopedCardKind::Subcircuit(1),
            ScopedCardKind::Analysis(0),
            ScopedCardKind::End
        ]
    );
    let expected = "t\nx0 a 0 outer k=2 k={base*2}\n\
        .subckt outer a b params: k=1k k=base\n  .model dm d\n  x1 a b inner t='k + 1'\n\
        \x20 .subckt inner 1 0\n    d1 1 0 dm\n  .ends inner\n  r1 a b 1k\n  .param local={k*2} z=3\n.ends outer\n\
        .subckt inner a b\n  r9 a b 1\n.ends inner\n.op\n.end\n";
    assert_eq!(written, expected);
    // The shadowing definition in the child scope is retained there.
    assert_eq!(second.subcircuits[0].subcircuits[0].name, "inner");
}

#[test]
fn body_cards_before_models_keep_their_position() {
    let (written, _, _) =
        round_trip("t\n.subckt s a b\nq1 a b 0 qm\n.model qm npn\n.ends s\nx1 1 2 s\n.end\n");
    let q = written.find("q1").unwrap();
    let m = written.find(".model").unwrap();
    assert!(q < m);
}

#[test]
fn expression_grouping_is_preserved() {
    let text = "t\n.param a={(1+2)*3} b=1-(2-3) c=2^3^2 d=-2^2 e=max(1,(2)) f={ x + y }\n\
        .param g=(a + b)*2 h=2*-3^2 i=2**3\n\
        v1 in 0 dc {(a+b)/2} ac {-a}\nr1 in out {1/(a*b)}\nx1 in out s w={a-(b-c)} v=a\n\
        .subckt s a b params: w=1 v=0\n.ends\n.end\n";
    let (written, first, second) = round_trip(text);
    for expected in [
        "a={(1+2)*3}",
        "b=1-(2-3)",
        "c=2^3^2",
        "d=-2^2",
        "e=max(1,(2))",
        "f={ x + y }",
        "g=(a + b)*2",
        "h=2*-3^2",
        "i=2**3",
        "dc {(a+b)/2} ac {-a}",
        "{1/(a*b)}",
        "w={a-(b-c)}",
        "v=a",
    ] {
        assert!(
            written.contains(expected),
            "{expected} missing in\n{written}"
        );
    }
    // Same trees, including explicit Group nodes.
    assert_eq!(first.params.len(), second.params.len());
    assert!(semantic_eq(&first, &second));
}

#[test]
fn options_globals_params_and_analyses() {
    let (written, _, second) = round_trip(
        "t\n.OPTION reltol=1e-4 abstol=1p method=gear\n.opt temp=30 savecurrents reltol=2m\n\
         .global VDD gnd vdd\n.param vdd=5\n.dc v1 0 5 0.5\n.ac dec 10 1 1meg\n\
         .noise v(out) v1 dec 10 1 100k\n.tran {vdd/10} 1m uic\n.end\n",
    );
    assert!(written.contains(".options reltol=1e-4 abstol=1p method=gear\n"));
    assert!(written.contains(".options temp=30 savecurrents reltol=2m\n"));
    assert!(written.contains(".global vdd 0 vdd\n"));
    assert!(written.contains(".noise v(out) v1 dec 10 1 100k\n"));
    assert!(written.contains(".tran {vdd/10} 1m uic\n"));
    assert_eq!(second.options.len(), 2);
    assert_eq!(second.options[1].settings[2].name, "reltol");
}

#[test]
fn no_auto_gnd_is_preserved_with_the_same_parser() {
    let parser = Parser::with_auto_gnd(false);
    let first = parser
        .parse_deck(&parse_deck_text(
            Path::new("a.cir"),
            "t\nr1 gnd a 1\n.global gnd\n",
        ))
        .unwrap();
    let written = write_netlist(&first).unwrap();
    assert!(written.contains("r1 gnd a 1"));
    let second = parser
        .parse_deck(&parse_deck_text(Path::new("b.cir"), &written))
        .unwrap();
    assert!(semantic_eq(&first, &second));
}

#[test]
fn deck_fragment_without_end_has_no_end() {
    let (written, _, _) = round_trip("t\nr1 a 0 1\n");
    assert_eq!(written, "t\nr1 a 0 1\n");
}

#[test]
fn syntax_only_includes_keep_paths_with_spaces() {
    let (written, first, second) = round_trip(
        "t\n.include \"dir with space/a b.inc\"\n.inc 'it\\'s.inc'\n.include plain/x.inc\n\
         .lib 'my libs/p q.lib' Typical\n.lib \"x.lib\" \"Two Words\"\n.end\n",
    );
    assert_eq!(first.includes[0].path, "dir with space/a b.inc");
    assert!(written.contains(".include \"dir with space/a b.inc\"\n"));
    assert!(written.contains(".include 'it\\'s.inc'\n"));
    assert!(written.contains(".include plain/x.inc\n"));
    assert!(
        written.contains(".lib 'my libs/p q.lib' \"typical\"\n")
            || written.contains(".lib 'my libs/p q.lib' typical\n")
    );
    assert!(written.contains(".lib \"x.lib\" \"two words\"\n"));
    assert_eq!(second.includes[1].path, "it's.inc");
    assert_eq!(second.includes[3].section.as_deref(), Some("typical"));
    assert_eq!(second.includes[4].section.as_deref(), Some("two words"));
}

#[test]
fn inconsistent_path_spelling_falls_back_to_quoting() {
    let mut netlist = parse("t\n.include a.inc\n.end\n");
    netlist.includes[0].path = "with space.inc".to_owned();
    let written = write_netlist(&netlist).unwrap();
    assert!(
        written.contains(".include \"with space.inc\"\n"),
        "{written}"
    );
    netlist.includes[0].path = "q\"b\\s.inc".to_owned();
    let written = write_netlist(&netlist).unwrap();
    assert!(
        written.contains(".include \"q\\\"b\\\\s.inc\"\n"),
        "{written}"
    );
    let reparsed = parse(&written);
    assert_eq!(reparsed.includes[0].path, "q\"b\\s.inc");
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path = std::env::temp_dir().join(format!("spice-writer-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// parse_file -> write -> (written next to the original) parse_file.
fn file_round_trip(path: &Path) -> (String, Netlist, Netlist) {
    let parser = Parser::new();
    let first = parser
        .parse_file(path)
        .unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let written = write_netlist(&first).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let out = path.with_file_name("zz written.cir");
    fs::write(&out, &written).unwrap();
    let second = parser
        .parse_file(&out)
        .unwrap_or_else(|e| panic!("{path:?}: {e}\n{written}"));
    if let Some(difference) = semantic_diff(&first, &second) {
        panic!("{path:?}: {difference}\n{written}");
    }
    assert_eq!(
        write_netlist(&second).unwrap(),
        written,
        "{path:?}: fixed point"
    );
    assert_eq!(
        write_netlist(&first).unwrap(),
        written,
        "{path:?}: deterministic"
    );
    let _ = fs::remove_file(out);
    (written, first, second)
}

#[test]
fn every_conformance_fixture_round_trips() {
    let conformance = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance");
    let temp = TempDir::new("fixtures");
    copy_dir(&conformance, &temp.0);
    let mut count = 0;
    for dir in ["netlists", "parser"] {
        let mut files: Vec<_> = fs::read_dir(temp.0.join(dir))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "cir"))
            .collect();
        files.sort();
        for file in files {
            file_round_trip(&file);
            count += 1;
        }
    }
    file_round_trip(&temp.0.join("parser/sources/main.cir"));
    assert!(count >= 17, "found {count} fixtures");
}

#[test]
fn resolved_includes_are_written_as_directives_not_content() {
    let temp = TempDir::new("includes");
    let dir = temp.0.join("my dir");
    fs::create_dir_all(dir.join("sub dir")).unwrap();
    fs::write(
        dir.join("sub dir/part file.inc"),
        "* fragment\n.subckt half a b\nr1 a b 1k\n.ends\n.model dm d\n",
    )
    .unwrap();
    fs::write(dir.join("sub dir/lib part.inc"), "r7 a 0 7\n").unwrap();
    fs::write(
        dir.join("my lib.lib"),
        ".lib typ\nr9 a 0 9\n.include 'sub dir/lib part.inc'\n.endl typ\n.lib other\nr8 a 0 8\n.endl\n",
    )
    .unwrap();
    fs::write(
        dir.join("main.cir"),
        "title\n.include \"sub dir/part file.inc\"\n.lib 'my lib.lib' TYP\nx1 a 0 half\nd1 a 0 dm\n.end\n",
    )
    .unwrap();
    let (written, first, second) = file_round_trip(&dir.join("main.cir"));
    // Resolved content exists in the AST but is not inlined into the text.
    assert!(first.devices.len() > 2 && first.subcircuits.len() == 1);
    assert!(first.cards.iter().any(|c| !c.include_chain.is_empty()));
    assert_eq!(
        written,
        "title\n.include \"sub dir/part file.inc\"\n.lib 'my lib.lib' typ\nx1 a 0 half\nd1 a 0 dm\n.end\n"
    );
    assert!(!written.contains("r9"));
    assert_eq!(first.cards.len(), second.cards.len());
    assert!(second.includes[0].resolved_path.is_some());
}

#[test]
fn source_locations_are_ignored_but_semantics_are_not() {
    let a = parse("t\nr1 a b 1k\n.end\n");
    let b = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("elsewhere/other.cir"),
            "t\n\n* x\n   R1   a   b\n+ 1K\n.end\n",
        ))
        .unwrap();
    assert!(semantic_diff(&a, &b).is_some(), "1k vs 1K spelling differs");
    let c = Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("elsewhere/other.cir"),
            "t\n\n* x\n   R1   a   b\n+ 1k\n.end\n",
        ))
        .unwrap();
    assert_ne!(a, c, "raw equality sees locations");
    assert!(semantic_eq(&a, &c));
    assert_eq!(semantic_diff(&a, &c), None);
    let d = parse("t\nr1 a b 1k\nr2 a b 1k\n.end\n");
    assert!(semantic_diff(&a, &d).unwrap().contains("entries"));
}

fn refused(netlist: &Netlist, needle: &str) {
    match write_netlist(netlist) {
        Err(SpiceError::Unsupported { feature, .. }) => {
            assert!(feature.contains(needle), "{feature} lacks {needle}");
        }
        other => panic!("expected an Unsupported error for {needle}, got {other:?}"),
    }
}

#[test]
fn unrepresentable_asts_are_explicit_errors() {
    let base = parse(
        "t\nvp a 0 pulse(0 5 1n)\nr1 a 0 {x+1}\n.param p={a*(b+c)}\nx1 a 0 s w=1\n.subckt s a b\n.ends\n.end\n",
    );

    let mut n = base.clone();
    n.title = "two\nlines".into();
    refused(&n, "title");

    let mut n = base.clone();
    n.devices[0].designator = 'z';
    n.devices[0].name = "z1".into();
    refused(&n, "no writer");

    // PULSE field present after an omitted one.
    let mut n = base.clone();
    if let ParameterKind::Waveform(SourceWaveform::Pulse(p)) = &mut n.devices[0].parameters[0].kind
    {
        p.fall = Some(PositionedValue {
            text: "1".into(),
            location: p.initial.location.clone(),
        });
        p.rise = None;
    }
    refused(&n, "omitted");

    // Expression text that no longer matches the stored tree.
    let mut n = base.clone();
    if let ParameterKind::Expression(e) = &mut n.devices[1].parameters[0].kind {
        e.text = "x*2".into();
    }
    refused(&n, "does not match");

    let mut n = base.clone();
    n.params[0].assignments[0].expression.text = "a*b+c".into();
    refused(&n, "does not match");

    // Name that is not a single token.
    let mut n = base.clone();
    n.devices[2].nodes[0] = "a b".into();
    refused(&n, "single name token");

    // Unknown parameter name for the device.
    let mut n = base.clone();
    n.devices[1].parameters[0].name = "bogus".into();
    refused(&n, "parameter");

    // Card index out of range / entries not referenced.
    let mut n = base.clone();
    n.cards.remove(0);
    refused(&n, "exactly once");
    let mut n = base.clone();
    n.cards[0].kind = ScopedCardKind::Device(99);
    refused(&n, "out of range");

    // A stray closing card.
    let mut n = base.clone();
    n.cards.push(spice_netlist::ast::ScopedCard {
        kind: ScopedCardKind::Ends,
        source: n.cards[0].source.clone(),
        include_chain: Vec::new(),
    });
    refused(&n, "outside a subcircuit");

    // Unsupported model family and analysis tokens that cannot be re-lexed.
    let mut n = parse("t\n.model dm d\n.tran 1 2\n.end\n");
    n.models[0].base = "bsim4".into();
    refused(&n, "model type");
    let mut n = parse("t\n.tran 1 2\n.end\n");
    n.analyses[0].arguments[0] = "1 2".into();
    refused(&n, "re-tokenize");
}

#[test]
fn scopes_crossing_include_boundaries_are_refused() {
    let temp = TempDir::new("crossing");
    fs::write(temp.0.join("open.inc"), ".subckt s a b\nr1 a b 1\n").unwrap();
    fs::write(
        temp.0.join("main.cir"),
        "t\n.include open.inc\n.ends\n.end\n",
    )
    .unwrap();
    let netlist = Parser::new().parse_file(temp.0.join("main.cir")).unwrap();
    refused(&netlist, "closed by `.ends` in the file that opened it");
}
