//! Shared IFparm-table machinery for the Berkeley short-channel models.
//!
//! `b3.c`/`b4.c` describe their model and instance parameters with the same
//! `IFparm` macro tables, their `*def.h` headers declare the numeric IDs with
//! `#define <DEV>_X <n>`, and their `*mpar.c`/`*par.c` setters assign one
//! destination field (plus its given bit) per case. This module extracts only
//! those interface facts: keyword, canonical alias, ID, setter destination,
//! given bit, input type and access. No implementation, formula or
//! description text is copied.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// One model family's generator configuration.
pub(crate) struct Spec {
    /// The device directory under `src/spicelib/devices`.
    pub(crate) directory: &'static str,
    /// The C identifier prefix, including the trailing underscore.
    pub(crate) prefix: &'static str,
    /// The `cargo xtask` subcommand that regenerates these files.
    pub(crate) command: &'static str,
    /// The interface source set named in the generated header.
    pub(crate) interface_inputs: &'static str,
    /// The model table's C name.
    pub(crate) model_table: &'static str,
    /// The instance table's C name.
    pub(crate) instance_table: &'static str,
    /// The setup file whose literal defaults are extracted.
    pub(crate) setup_file: &'static str,
    /// The text that ends the model-default block in the setup file.
    pub(crate) instance_boundary: &'static str,
    /// Reviewed model-table row count; a change must be reviewed.
    pub(crate) model_expected: usize,
    /// Reviewed instance-table row count.
    pub(crate) instance_expected: usize,
    /// Reviewed literal-default count.
    pub(crate) literal_expected: usize,
}

/// A parsed `cargo xtask <dev>-schema` invocation.
pub(crate) struct Args {
    /// The ngspice checkout holding the C sources.
    pub(crate) source: PathBuf,
    /// Compare instead of writing.
    pub(crate) check: bool,
}

/// Parses `--source <dir> [--check]`.
pub(crate) fn args(args: &[String], usage: &str) -> Result<Args, String> {
    let (mut source, mut check) = (None, false);
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--source" if source.is_none() => {
                source = Some(PathBuf::from(args.next().ok_or(usage)?))
            }
            "--check" if !check => check = true,
            _ => return Err(usage.into()),
        }
    }
    Ok(Args {
        source: source.ok_or(usage)?,
        check,
    })
}

/// The upstream Git revision of the named clean input files.
pub(crate) fn revision(source: &Path, files: &[String]) -> Result<String, String> {
    // Only the named read inputs must be clean; unrelated reference-tree work
    // is neither inspected nor modified. Record the Git revision, not a date.
    let status = Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["status", "--porcelain", "--"])
        .args(files)
        .output()
        .map_err(|e| e.to_string())?;
    if !status.status.success() || !status.stdout.is_empty() {
        return Err("schema generator inputs must be clean tracked files in a Git checkout".into());
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
pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) symbol: String,
    pub(crate) kind: String,
    pub(crate) access: &'static str,
    pub(crate) alias: bool,
    /// C's `IF_UNINTERESTING` (`IOPU`/`IOPUR`): settable and askable but
    /// omitted from `show all`. Recorded, not dropped.
    pub(crate) uninteresting: bool,
}

/// Restricted IFparm reader: one declarator per line, four comma-separated
/// leading arguments. Descriptions are deliberately not copied into Rust.
pub(crate) fn table(text: &str, name: &str) -> Result<Vec<Entry>, String> {
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
        let (access, alias, uninteresting) = match mac.trim() {
            "IOP" => ("ReadWrite", false, false),
            "IOPR" => ("ReadWrite", true, false),
            "IOPU" => ("ReadWrite", false, true),
            "IOPUR" => ("ReadWrite", true, true),
            "IP" => ("Input", false, false),
            "OP" => ("Output", false, false),
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
            uninteresting,
        });
    }
    Ok(entries)
}

fn ids<'a>(header: &'a str, prefix: &str) -> Result<BTreeMap<&'a str, u16>, String> {
    let mut result = BTreeMap::new();
    for line in header.lines() {
        let words: Vec<_> = line.split_whitespace().collect();
        if words.len() >= 3 && words[0] == "#define" && words[1].starts_with(prefix) {
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
pub(crate) fn setters(
    text: &str,
    prefix: &str,
) -> Result<BTreeMap<String, (String, Option<String>)>, String> {
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
            if symbol.starts_with(prefix) {
                if cases.insert(symbol.into(), Vec::new()).is_some() {
                    return Err(format!("duplicate setter {symbol}"));
                }
                current = Some(symbol.to_owned());
            }
        }
        if let Some(symbol) = &current {
            for field_prefix in ["mod->", "here->"] {
                if let Some(rest) = line.strip_prefix(field_prefix) {
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
            if symbol == format!("{prefix}IC") {
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

/// Strip C block comments without changing physical line boundaries, so that
/// statement windows stay intact.
pub(crate) fn strip_comments(text: &str) -> Result<String, String> {
    let mut clean = String::new();
    let mut input = text.chars().peekable();
    let mut comment = false;
    while let Some(c) = input.next() {
        if !comment && c == '/' && input.peek().is_some_and(|next| *next == '*') {
            input.next();
            comment = true;
        } else if comment && c == '*' && input.peek().is_some_and(|next| *next == '/') {
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

/// The generated `parameters.rs` text.
pub(crate) fn parameters(
    spec: &Spec,
    table_text: &str,
    header: &str,
    model_setters: &str,
    instance_setters: &str,
    revision: &str,
) -> Result<String, String> {
    let ids = ids(header, spec.prefix)?;
    let models = table(table_text, spec.model_table)?;
    let instances = table(table_text, spec.instance_table)?;
    if models.len() != spec.model_expected || instances.len() != spec.instance_expected {
        return Err(format!(
            "table size changed: {} model, {} instance; review before accepting",
            models.len(),
            instances.len()
        ));
    }
    let mut out = format!(
        "// Generated interface facts; no C implementation code. Do not hand edit.\n// Source: ngspice {revision}, {}/{}.\n// Regenerate: cargo xtask {} --source /path/to/ngspice\nuse super::{{Access, InputType, Parameter}};\n\n/// Upstream revision of the four clean generator inputs.\npub const SOURCE_REVISION: &str = \"{revision}\";\n",
        spec.directory, spec.interface_inputs, spec.command
    );
    let mut uninteresting: Vec<String> = Vec::new();
    for (name, entries, setter_text) in [
        ("MODEL", models, model_setters),
        ("INSTANCE", instances, instance_setters),
    ] {
        let setters = setters(setter_text, spec.prefix)?;
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
            if entry.uninteresting {
                uninteresting.push(format!("{name}:{}", entry.name));
            }
            writeln!(out, "    Parameter::new({:?}, {canonical:?}, {id}, {:?}, {}, {}, InputType::{}, Access::{}, {}),", entry.name, entry.symbol, quoted(field), quoted(given), entry.kind, entry.access, entry.alias).map_err(|e| e.to_string())?;
        }
        out.push_str("];\n");
    }
    out.push_str(
        "\n/// Entries C marks `IF_UNINTERESTING` (`IOPU`/`IOPUR`): settable and\n/// askable, but omitted from `show all`. `TABLE:keyword` pairs in table order.\n#[rustfmt::skip]\npub const UNINTERESTING: &[&str] = &[\n",
    );
    for item in &uninteresting {
        writeln!(out, "    {item:?},").map_err(|e| e.to_string())?;
    }
    out.push_str("];\n");
    Ok(out)
}

/// The generated `defaults_data.rs` text: the setup file's literal model
/// defaults, in C order, as interface facts only.
pub(crate) fn literal_defaults(
    spec: &Spec,
    tables: &str,
    mpar: &str,
    setup: &str,
    revision: &str,
) -> Result<String, String> {
    let model = table(tables, spec.model_table)?;
    let setters = setters(mpar, spec.prefix)?;
    let mut fields = BTreeMap::new();
    for p in &model {
        if !p.alias {
            let (field, given) = setters.get(&p.symbol).ok_or("missing model setter")?;
            fields.insert(field.as_str(), (p, given));
        }
    }
    let clean = strip_comments(setup)?;
    let end = clean
        .find(spec.instance_boundary)
        .ok_or_else(|| format!("missing instance-setup boundary in {}", spec.setup_file))?;
    let lines: Vec<_> = clean[..end]
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let mut out = format!(
        "// Generated literal-default facts, not C implementation code.\n// Source: ngspice {revision}, {}/{}.\n// Dependent defaults and diagnostics are implemented separately.\n\n/// Canonical model keyword and literal default, in C setup order.\n#[rustfmt::skip]\npub(super) const LITERALS: &[(&str, f64)] = &[\n",
        spec.directory, spec.setup_file
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
    if seen.len() != spec.literal_expected {
        return Err(format!(
            "literal default count changed: {} (expected {}); review before regenerating",
            seen.len(),
            spec.literal_expected
        ));
    }
    out.push_str("];\n");
    Ok(out)
}

/// Writes or checks one generated file.
pub(crate) fn emit(path: &str, text: &str, check: bool) -> Result<(), String> {
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
            "wrote {} from the recorded revision; review the generated facts",
            output.display()
        );
    }
    Ok(())
}

/// Reads one input file from the device directory.
pub(crate) fn read(source: &Path, spec: &Spec, name: &str) -> Result<String, String> {
    let path = source
        .join("src/spicelib/devices")
        .join(spec.directory)
        .join(name);
    fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))
}
