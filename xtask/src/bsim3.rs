//! Extract BSIM3 interface facts, not C implementation code. The committed
//! table is an offline input-schema artifact; Cargo never needs the C tree.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const OUTPUT: &str = "src/devices/bsim3/parameters.rs";
const DEFAULTS: &str = "src/devices/bsim3/defaults_data.rs";
const SIZE: &str = "src/devices/bsim3/size_data.rs";
const USAGE: &str = "usage: cargo xtask bsim3-schema --source <ngspice-checkout> [--check]";

pub(crate) fn main(args: &[String]) -> Result<(), String> {
    let (mut source, mut check) = (None, false);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--source" if source.is_none() => {
                source = Some(PathBuf::from(args.next().ok_or(USAGE)?))
            }
            "--check" if !check => check = true,
            _ => return Err(USAGE.into()),
        }
    }
    let source = source.ok_or(USAGE)?;
    let revision = revision(&source)?;
    let directory = source.join("src/spicelib/devices/bsim3");
    let read =
        |name: &str| fs::read_to_string(directory.join(name)).map_err(|e| format!("{name}: {e}"));
    let text = generate(
        &read("b3.c")?,
        &read("bsim3def.h")?,
        &read("b3mpar.c")?,
        &read("b3par.c")?,
        &revision,
    )?;
    let defaults = literal_defaults(
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
    for (path, text) in [(OUTPUT, text), (DEFAULTS, defaults), (SIZE, size)] {
        let output = crate::workspace_root().join(path);
        if check {
            let existing =
                fs::read_to_string(&output).map_err(|e| format!("{}: {e}", output.display()))?;
            if existing != text {
                return Err(format!(
                    "{} differs; regenerate and review the change",
                    output.display()
                ));
            }
        } else {
            fs::create_dir_all(output.parent().ok_or("missing output parent")?)
                .map_err(|e| e.to_string())?;
            fs::write(&output, text).map_err(|e| e.to_string())?;
            println!(
                "wrote {} from {revision}; review the generated facts",
                output.display()
            );
        }
    }
    if check {
        println!("BSIM3 schema/default facts match {revision} (429 model, 49 instance entries)");
    }
    Ok(())
}

fn revision(source: &Path) -> Result<String, String> {
    // Only the six read inputs must be clean; unrelated reference-tree work
    // is neither inspected nor modified. Record the Git revision, not a date.
    let paths = [
        "b3.c",
        "bsim3def.h",
        "b3mpar.c",
        "b3par.c",
        "b3set.c",
        "b3temp.c",
    ]
    .map(|name| format!("src/spicelib/devices/bsim3/{name}"));
    let status = Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["status", "--porcelain", "--"])
        .args(paths)
        .output()
        .map_err(|e| e.to_string())?;
    if !status.status.success() || !status.stdout.is_empty() {
        return Err("BSIM3 generator inputs must be clean tracked files in a Git checkout".into());
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|e| e.to_string())?;
    let revision = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;
    let revision = revision.trim();
    if !output.status.success()
        || revision.len() != 40
        || !revision.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("cannot identify the upstream Git revision".into());
    }
    Ok(revision.into())
}

#[derive(Debug, PartialEq, Eq)]
struct Entry {
    name: String,
    symbol: String,
    kind: String,
    access: &'static str,
    alias: bool,
}

/// Restricted IFparm reader: one declarator per line, four comma-separated
/// leading arguments. Descriptions are deliberately not copied into Rust.
fn table(text: &str, name: &str) -> Result<Vec<Entry>, String> {
    let marker = format!("IFparm {name}[] = {{");
    let body = text
        .split_once(&marker)
        .ok_or_else(|| format!("missing {name}"))?
        .1
        .split_once("};")
        .ok_or("unterminated IFparm table")?
        .0;
    let mut entries = Vec::new();
    let mut names = BTreeSet::new();
    for line in body.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.starts_with("/*") {
            continue;
        }
        let (mac, arguments) = line
            .split_once('(')
            .ok_or_else(|| format!("unexpected {name} line: {line}"))?;
        let (access, alias) = match mac.trim() {
            "IOP" => ("ReadWrite", false),
            "IOPR" => ("ReadWrite", true),
            "IP" => ("Input", false),
            "OP" => ("Output", false),
            other => return Err(format!("unknown IFparm macro {other}")),
        };
        let args: Vec<_> = arguments.splitn(4, ',').map(str::trim).collect();
        if args.len() != 4 || !args[3].starts_with('"') || !line.contains(")") {
            return Err(format!("unsupported IFparm declaration: {line}"));
        }
        let name = args[0]
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .ok_or("invalid keyword")?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            || !names.insert(name)
        {
            return Err(format!("invalid/duplicate IFparm keyword {name}"));
        }
        let kind = match args[2] {
            "IF_REAL" => "Real",
            "IF_INTEGER" => "Integer",
            "IF_FLAG" => "Flag",
            "IF_STRING" => "String",
            "IF_REALVEC" => "RealVector",
            other => return Err(format!("unknown IFparm type {other}")),
        };
        entries.push(Entry {
            name: name.into(),
            symbol: args[1].into(),
            kind: kind.into(),
            access,
            alias,
        });
    }
    Ok(entries)
}

fn ids(header: &str) -> Result<BTreeMap<&str, u16>, String> {
    let mut result = BTreeMap::new();
    for line in header.lines() {
        let words: Vec<_> = line.split_whitespace().collect();
        if words.len() >= 3 && words[0] == "#define" && words[1].starts_with("BSIM3_") {
            let id = words[2]
                .parse()
                .map_err(|_| format!("nonliteral parameter ID: {line}"))?;
            if result.insert(words[1], id).is_some() {
                return Err(format!("duplicate ID declaration: {}", words[1]));
            }
        }
    }
    Ok(result)
}

/// A setter case's destination and given bit. No RHS expression, control
/// flow, formula or source comment is emitted into the generated table.
fn setters(text: &str) -> Result<BTreeMap<String, (String, Option<String>)>, String> {
    let mut cases: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut current = None;
    for line in text
        .lines()
        .map(|line| line.trim().trim_start_matches('{').trim_start())
    {
        if let Some(rest) = line.strip_prefix("case")
            && let Some((symbol, _)) = rest.split_once(':')
        {
            let symbol = symbol.trim();
            if symbol.starts_with("BSIM3_") {
                if cases.insert(symbol.into(), Vec::new()).is_some() {
                    return Err(format!("duplicate setter {symbol}"));
                }
                current = Some(symbol.to_owned());
            }
        }
        if let Some(symbol) = &current {
            for prefix in ["mod->", "here->"] {
                if let Some(rest) = line.strip_prefix(prefix) {
                    let field: String = rest
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                        .collect();
                    if rest[field.len()..].trim_start().starts_with('=')
                        && !rest[field.len()..].trim_start().starts_with("==")
                    {
                        cases
                            .get_mut(symbol)
                            .ok_or("missing setter case")?
                            .push(field);
                    }
                }
            }
        }
    }
    cases
        .into_iter()
        .map(|(symbol, fields)| {
            if symbol == "BSIM3_IC" {
                return Ok((symbol, (String::new(), None)));
            }
            let destinations: BTreeSet<_> =
                fields.iter().filter(|s| !s.ends_with("Given")).collect();
            let given: BTreeSet<_> = fields.iter().filter(|s| s.ends_with("Given")).collect();
            if destinations.len() != 1 || given.len() > 1 {
                return Err(format!(
                    "ambiguous setter destinations for {symbol}: {fields:?}"
                ));
            }
            let field = (*destinations.first().ok_or("missing setter destination")?).clone();
            Ok((symbol, (field, given.first().map(|s| (*s).clone()))))
        })
        .collect()
}

fn generate(
    table_text: &str,
    header: &str,
    model_setters: &str,
    instance_setters: &str,
    revision: &str,
) -> Result<String, String> {
    let ids = ids(header)?;
    let models = table(table_text, "BSIM3mPTable")?;
    let instances = table(table_text, "BSIM3pTable")?;
    if models.len() != 429 || instances.len() != 49 {
        return Err(format!(
            "table size changed: {} model, {} instance; review before accepting",
            models.len(),
            instances.len()
        ));
    }
    let mut out = format!(
        "// Generated interface facts; no C implementation code. Do not hand edit.\n// Source: ngspice {revision}, bsim3/{{b3.c,bsim3def.h,b3mpar.c,b3par.c}}.\n// Regenerate: cargo xtask bsim3-schema --source /path/to/ngspice\nuse super::{{Access, InputType, Parameter}};\n\n/// Upstream revision of the four clean generator inputs.\npub const SOURCE_REVISION: &str = \"{revision}\";\n"
    );
    for (name, entries, setter_text) in [
        ("MODEL", models, model_setters),
        ("INSTANCE", instances, instance_setters),
    ] {
        let setters = setters(setter_text)?;
        let mut canonical = BTreeMap::new();
        writeln!(out, "\n/// `{name}` IFparm entries in C table order, including aliases and ask-only rows.\n#[rustfmt::skip]\npub const {name}: &[Parameter] = &[").map_err(|e| e.to_string())?;
        for entry in entries {
            let id = ids
                .get(entry.symbol.as_str())
                .ok_or_else(|| format!("no ID for {}", entry.symbol))?;
            let canonical = canonical.entry(*id).or_insert_with(|| entry.name.clone());
            if entry.alias != (*canonical != entry.name) {
                return Err(format!("unexpected alias declaration {}", entry.name));
            }
            let (field, given) = if entry.access == "Output" {
                (None, None)
            } else {
                let (field, given) = setters
                    .get(&entry.symbol)
                    .ok_or_else(|| format!("no setter case for {}", entry.symbol))?;
                ((!field.is_empty()).then_some(field), given.as_ref())
            };
            let quoted =
                |s: Option<&String>| s.map_or_else(|| "None".into(), |s| format!("Some({s:?})"));
            writeln!(out, "    Parameter::new({:?}, {canonical:?}, {id}, {:?}, {}, {}, InputType::{}, Access::{}, {}),", entry.name, entry.symbol, quoted(field), quoted(given), entry.kind, entry.access, entry.alias).map_err(|e| e.to_string())?;
        }
        out.push_str("];\n");
    }
    Ok(out)
}

/// Strip C block comments without changing physical line boundaries, so that
/// statement windows stay intact.
fn strip_comments(text: &str) -> Result<String, String> {
    let mut clean = String::new();
    let mut input = text.chars().peekable();
    let mut comment = false;
    while let Some(c) = input.next() {
        if !comment && c == '/' && input.peek() == Some(&'*') {
            input.next();
            comment = true;
        } else if comment && c == '*' && input.peek() == Some(&'/') {
            input.next();
            comment = false;
        } else if !comment || c == '\n' {
            clean.push(c);
        }
    }
    if comment {
        return Err("unterminated C comment".into());
    }
    Ok(clean)
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
    let model = table(tables, "BSIM3mPTable")?;
    let setters = setters(mpar)?;
    let mut canonical = BTreeMap::new();
    for p in &model {
        if !p.alias {
            let (field, _) = setters.get(&p.symbol).ok_or("missing model setter")?;
            // C body terms name fields without the `BSIM3` prefix.
            canonical.insert(field.trim_start_matches("BSIM3"), p.name.as_str());
        }
    }
    let clean = strip_comments(temperature)?;
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

/// Literal default values are interface facts too. Only an immediately
/// guarded, single assignment of a finite numeric literal is extracted.
/// Dependent defaults are reimplemented and reviewed in defaults.rs, never
/// translated from C expressions by this tool.
fn literal_defaults(
    tables: &str,
    mpar: &str,
    setup: &str,
    revision: &str,
) -> Result<String, String> {
    let model = table(tables, "BSIM3mPTable")?;
    let setters = setters(mpar)?;
    let mut fields = BTreeMap::new();
    for p in &model {
        if !p.alias {
            let (field, given) = setters.get(&p.symbol).ok_or("missing model setter")?;
            fields.insert(field.as_str(), (p, given));
        }
    }
    let clean = strip_comments(setup)?;
    let end = clean
        .find("for (here = BSIM3instances")
        .ok_or("missing BSIM3 instance-setup boundary")?;
    let lines: Vec<_> = clean[..end]
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let mut out = format!(
        "// Generated literal-default facts, not C implementation code.\n// Source: ngspice {revision}, bsim3/b3set.c.\n// Dependent defaults and diagnostics are implemented separately.\n\n/// Canonical model keyword and literal default, in C setup order.\n#[rustfmt::skip]\npub(super) const LITERALS: &[(&str, f64)] = &[\n"
    );
    let mut seen = BTreeSet::new();
    for pair in lines.windows(2) {
        let Some(given) = pair[0]
            .strip_prefix("if (!model->")
            .and_then(|s| s.strip_suffix(')'))
        else {
            continue;
        };
        let Some((field, rhs)) = pair[1]
            .strip_prefix("model->")
            .and_then(|s| s.split_once('='))
        else {
            continue;
        };
        let Some(literal) = rhs.trim().strip_suffix(';') else {
            continue;
        };
        let Ok(value) = literal.trim().parse::<f64>() else {
            continue;
        };
        if !value.is_finite() {
            return Err("nonfinite C literal default".into());
        }
        let (p, flag) = fields
            .get(field.trim())
            .ok_or_else(|| format!("default without interface field: {field}"))?;
        if flag.as_deref() != Some(given) {
            return Err(format!("default's given-bit mismatch for {}", p.name));
        }
        if !seen.insert(p.name.as_str()) {
            return Err(format!("duplicate literal default for {}", p.name));
        }
        writeln!(out, "    ({:?}, {value:?}),", p.name).map_err(|e| e.to_string())?;
    }
    if seen.len() != 376 {
        return Err(format!(
            "literal default count changed: {}; review before regenerating",
            seen.len()
        ));
    }
    out.push_str("];\n");
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{ids, setters, table};
    #[test]
    fn table_reader_preserves_order_access_and_aliases() {
        let text = "IFparm T[] = {\nIOP(\"vth0\", X, IF_REAL, \"not copied\"),\nIOPR(\"vtho\", X, IF_REAL, \"alias\"),\nOP(\"gm\", Y, IF_REAL, \"output\"),\n};";
        let entries = table(text, "T").unwrap();
        assert_eq!(
            entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["vth0", "vtho", "gm"]
        );
        assert!(entries[1].alias);
        assert_eq!(entries[2].access, "Output");
        assert!(table(&text.replace("IOPR", "UNKNOWN"), "T").is_err());
        assert!(table(&text.replace("vtho", "vth0"), "T").is_err());
    }
    #[test]
    fn generator_rejects_ambiguous_plumbing_and_nonliteral_ids() {
        let valid = "case BSIM3_MOD_TOX:\nmod->BSIM3tox = value->rValue;\nmod->BSIM3toxGiven = TRUE;\nbreak;";
        let fields = setters(valid).unwrap();
        assert_eq!(
            fields["BSIM3_MOD_TOX"],
            ("BSIM3tox".into(), Some("BSIM3toxGiven".into()))
        );
        assert!(setters(&format!("{valid}\nmod->other = value->rValue;")).is_err());
        assert!(ids("#define BSIM3_X (1+2)").is_err());
        assert_eq!(ids("#define BSIM3_X 17").unwrap()["BSIM3_X"], 17);
    }
}
