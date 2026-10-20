//! Extract BSIM3 interface facts, not C implementation code. The committed
//! tables are offline input-schema artifacts; Cargo never needs the C tree.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use crate::ifparm::{self, Spec};

const USAGE: &str = "usage: cargo xtask bsim3-schema --source <ngspice-checkout> [--check]";

const SPEC: Spec = Spec {
    directory: "bsim3",
    prefix: "BSIM3_",
    command: "bsim3-schema",
    interface_inputs: "{b3.c,bsim3def.h,b3mpar.c,b3par.c}",
    model_table: "BSIM3mPTable",
    instance_table: "BSIM3pTable",
    setup_file: "b3set.c",
    instance_boundary: "for (here = BSIM3instances",
    model_expected: 429,
    instance_expected: 49,
    literal_expected: 376,
};

pub(crate) fn main(args: &[String]) -> Result<(), String> {
    let parsed = ifparm::args(args, USAGE)?;
    let source = parsed.source;
    let files = [
        "b3.c",
        "bsim3def.h",
        "b3mpar.c",
        "b3par.c",
        "b3set.c",
        "b3temp.c",
    ]
    .map(|name| format!("src/spicelib/devices/bsim3/{name}"));
    let revision = ifparm::revision(&source, &files)?;
    let read = |name: &str| ifparm::read(&source, &SPEC, name);
    let text = ifparm::parameters(
        &SPEC,
        &read("b3.c")?,
        &read("bsim3def.h")?,
        &read("b3mpar.c")?,
        &read("b3par.c")?,
        &revision,
    )?;
    let defaults = ifparm::literal_defaults(
        &SPEC,
        &read("b3.c")?,
        &read("b3mpar.c")?,
        &read("b3set.c")?,
        &revision,
    )?;
    let size = size_dependent(
        &read("b3.c")?,
        &read("b3mpar.c")?,
        &read("b3temp.c")?,
        &revision,
    )?;
    for (path, text) in [
        ("src/devices/bsim3/parameters.rs", text),
        ("src/devices/bsim3/defaults_data.rs", defaults),
        ("src/devices/bsim3/size_data.rs", size),
    ] {
        ifparm::emit(path, &text, parsed.check)?;
    }
    if parsed.check {
        println!("BSIM3 schema/default facts match {revision} (429 model, 49 instance entries)");
    }
    Ok(())
}

/// `b3temp.c` builds each instance's size-dependent parameter from the model
/// value plus L, W and reciprocal-area coefficients. Extract only that
/// interface relation, in C order; no formula or expression is translated.
fn size_dependent(
    tables: &str,
    mpar: &str,
    temperature: &str,
    revision: &str,
) -> Result<String, String> {
    let model = ifparm::table(tables, "BSIM3mPTable")?;
    let setters = ifparm::setters(mpar, SPEC.prefix)?;
    let mut canonical = BTreeMap::new();
    for p in &model {
        if !p.alias {
            let (field, _) = setters.get(&p.symbol).ok_or("missing model setter")?;
            // C body terms name fields without the `BSIM3` prefix.
            canonical.insert(field.trim_start_matches("BSIM3"), p.name.as_str());
        }
    }
    let clean = ifparm::strip_comments(temperature)?;
    let start = clean
        .find("Ldrn = here->BSIM3l;")
        .ok_or("missing BSIM3 size-dependent start")?;
    let end = clean
        .find("pParam->BSIM3abulkCVfactor")
        .ok_or("missing BSIM3 size-dependent end")?;
    let mut out = format!(
        "// Generated size-dependent coefficient facts, not C implementation code.\n// Source: ngspice {revision}, bsim3/b3temp.c.\n// Each row is a canonical parameter plus its L, W and reciprocal-area\n// coefficients. Dependent formulas live in setup.rs, not here.\n\n/// Canonical base keyword and its L, W, P coefficient keywords, in C order.\n#[rustfmt::skip]\npub(super) const SIZE_DEPENDENT: &[(&str, &str, &str, &str)] = &[\n"
    );
    let mut seen = BTreeSet::new();
    let body = &clean[start..end];
    let mut cursor = 0;
    while let Some(offset) = body[cursor..].find("pParam->BSIM3") {
        let begin = cursor + offset + "pParam->BSIM3".len();
        let statement_end = body[begin..]
            .find(';')
            .map(|end| begin + end)
            .ok_or("unterminated size-dependent statement")?;
        let chunk = &body[begin..statement_end];
        cursor = statement_end + 1;
        let Some((field, rest)) = chunk.split_once('=') else {
            // An inner `pParam` reference (for example `pParam->BSIM3weff` in
            // an effective-size expression) is not a coefficient statement.
            continue;
        };
        let field = field.trim();
        if !rest.contains("Inv_L") {
            // Effective-size and other non-binned assignments are handled by
            // reviewed code in setup.rs, not by this table.
            continue;
        }
        let base = *canonical
            .get(field)
            .ok_or_else(|| format!("no interface field for {field}"))?;
        let mut coefficients = Vec::new();
        for term in rest.split('+') {
            let Some((lhs, rhs)) = term.split_once('*') else {
                continue;
            };
            let lhs = lhs.trim();
            let Some(name) = lhs.strip_prefix("model->BSIM3") else {
                return Err(format!("unexpected coefficient {lhs} for {field}"));
            };
            let name = name.trim_end_matches([';', ' ', '\n']);
            let coefficient = *canonical
                .get(name)
                .ok_or_else(|| format!("no interface field for coefficient {name}"))?;
            coefficients.push((rhs.trim(), coefficient));
        }
        if coefficients.len() != 3
            || coefficients[0].0 != "Inv_L"
            || coefficients[1].0 != "Inv_W"
            || coefficients[2].0 != "Inv_LW"
        {
            return Err(format!("unexpected coefficient order for {field}"));
        }
        if !seen.insert(base) {
            return Err(format!("duplicate size-dependent row for {base}"));
        }
        writeln!(
            out,
            "    ({base:?}, {:?}, {:?}, {:?}),",
            coefficients[0].1, coefficients[1].1, coefficients[2].1
        )
        .map_err(|e| e.to_string())?;
    }
    if seen.len() != 83 {
        return Err(format!(
            "size-dependent row count changed: {}; review before regenerating",
            seen.len()
        ));
    }
    out.push_str("];\n");
    Ok(out)
}
