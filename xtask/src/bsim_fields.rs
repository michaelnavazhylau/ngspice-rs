//! Extract the numeric field declarations of a BSIM device's three C structs
//! (`sBSIMnmodel`, `bsimnSizeDependParam`, `sBSIMninstance`) as Rust struct
//! definitions: field names and scalar types only. The BSIM3 and BSIM4 ports
//! keep C's field names (without the `BSIM3`/`BSIM4` prefix) so that the
//! setup, temperature and load files can be followed statement by statement.
//! No formula, default or comment text is copied; pointers, arrays and matrix
//! bindings are skipped.
use std::fmt::Write as _;

/// One scalar field of a C struct.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Field {
    /// The C member name, e.g. `BSIM4vth0`.
    c_name: String,
    /// `f64`, `i32`, `bool` (a `:1` given bit) or `String` (`char *`).
    kind: &'static str,
}

/// Members of the struct whose body starts at `opening`, in declaration order.
fn members(file: &str, header: &str, opening: &str, closing: &str) -> Result<Vec<Field>, String> {
    let start = header
        .find(opening)
        .ok_or_else(|| format!("{file}: missing `{opening}`"))?;
    let body = &header[start + opening.len()..];
    let end = body
        .find(closing)
        .ok_or_else(|| format!("{file}: unterminated `{opening}`"))?;
    let mut fields = Vec::new();
    for line in body[..end].lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("struct ") {
            continue;
        }
        let words: Vec<&str> = line.trim_end_matches(';').split_whitespace().collect();
        let field = match words.as_slice() {
            ["double", name] if !name.contains(['*', '[']) => Field {
                c_name: (*name).into(),
                kind: "f64",
            },
            ["int", name] | ["const", "int", name] if !name.contains(['*', '[']) => Field {
                c_name: (*name).into(),
                kind: "i32",
            },
            ["unsigned", name, ":1"] | ["unsigned", name, ":", "1"] => Field {
                c_name: (*name).into(),
                kind: "bool",
            },
            ["char", name] if name.starts_with('*') && !name[1..].contains(['*', '[']) => Field {
                c_name: name[1..].into(),
                kind: "String",
            },
            // Pointers, matrix bindings, noise arrays and OpenMP/KLU blocks.
            ["double" | "BindElement" | "char", ..] => continue,
            _ => return Err(format!("{file}: unrecognised member `{line}`")),
        };
        if !field
            .c_name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(format!("{file}: unexpected member name `{line}`"));
        }
        fields.push(field);
    }
    Ok(fields)
}

/// The Rust spelling of a C member: the `BSIM3`/`BSIM4` prefix removed, a
/// raw identifier for Rust keywords.
fn rust_name(c_name: &str) -> String {
    let name = c_name
        .strip_prefix("BSIM4")
        .or_else(|| c_name.strip_prefix("BSIM3"))
        .unwrap_or(c_name);
    if matches!(
        name,
        "type" | "pub" | "mod" | "ref" | "in" | "fn" | "use" | "as"
    ) {
        format!("r#{name}")
    } else {
        name.into()
    }
}

fn emit_struct(
    out: &mut String,
    name: &str,
    c_struct: &str,
    fields: &[Field],
) -> Result<(), String> {
    let mut seen = std::collections::BTreeSet::new();
    for field in fields {
        if !seen.insert(rust_name(&field.c_name)) {
            return Err(format!("{c_struct}: duplicate member {}", field.c_name));
        }
    }
    writeln!(
        out,
        "\n/// `{c_struct}`'s scalar members ({} of them), in declaration order.\n\
         #[derive(Clone, Default)]\npub(crate) struct {name} {{",
        fields.len()
    )
    .map_err(|e| e.to_string())?;
    for field in fields {
        writeln!(
            out,
            "    pub(crate) {}: {},",
            rust_name(&field.c_name),
            field.kind
        )
        .map_err(|e| e.to_string())?;
    }
    out.push_str("}\n");
    writeln!(out, "\n#[rustfmt::skip]\nimpl {name} {{").map_err(|e| e.to_string())?;
    for (kind, accessor) in [("f64", "real"), ("i32", "int"), ("bool", "flag")] {
        if !fields.iter().any(|f| f.kind == kind) {
            continue;
        }
        writeln!(
            out,
            "    /// The `{kind}` member named `c_name` in C, if there is one.\n    \
             #[allow(dead_code)]\n    pub(crate) fn {accessor}_mut(&mut self, c_name: &str) -> Option<&mut {kind}> {{\n        \
             Some(match c_name {{"
        )
        .map_err(|e| e.to_string())?;
        for field in fields.iter().filter(|f| f.kind == kind) {
            writeln!(
                out,
                "            \"{}\" => &mut self.{},",
                field.c_name,
                rust_name(&field.c_name)
            )
            .map_err(|e| e.to_string())?;
        }
        out.push_str("            _ => return None,\n        })\n    }\n");
    }
    // Positional access over the scalar (non-string) members, for records
    // that hold the whole struct as numbers (`.sens`).
    let scalars: Vec<&Field> = fields.iter().filter(|f| f.kind != "String").collect();
    out.push_str("    /// C member names of the scalar members, in declaration order.\n    #[allow(dead_code)]\n    pub(crate) const MEMBERS: &[&str] = &[\n");
    for field in &scalars {
        writeln!(out, "        \"{}\",", field.c_name).map_err(|e| e.to_string())?;
    }
    out.push_str("    ];\n\n");
    out.push_str("    /// The scalar members as numbers, in [`Self::MEMBERS`] order (`i32` exactly,\n    /// a given bit as 0 or 1).\n    #[allow(dead_code)]\n    pub(crate) fn to_values(&self) -> Vec<f64> {\n        vec![\n");
    for field in &scalars {
        let name = rust_name(&field.c_name);
        let expression = match field.kind {
            "f64" => format!("self.{name}"),
            "i32" => format!("f64::from(self.{name})"),
            _ => format!("f64::from(u8::from(self.{name}))"),
        };
        writeln!(out, "            {expression},").map_err(|e| e.to_string())?;
    }
    out.push_str("        ]\n    }\n\n");
    out.push_str("    /// Stores numbers produced by [`Self::to_values`]; `values` must have\n    /// [`Self::MEMBERS`]`.len()` entries.\n    #[allow(dead_code, clippy::cast_possible_truncation)]\n    pub(crate) fn set_values(&mut self, values: &[f64]) {\n");
    for (index, field) in scalars.iter().enumerate() {
        let name = rust_name(&field.c_name);
        let expression = match field.kind {
            "f64" => format!("values[{index}]"),
            "i32" => format!("values[{index}] as i32"),
            _ => format!("values[{index}] != 0.0"),
        };
        writeln!(out, "        self.{name} = {expression};").map_err(|e| e.to_string())?;
    }
    out.push_str("    }\n");
    out.push_str("}\n");
    Ok(())
}

/// `(C parameter ID, member)` for every `case` of an ask routine that reads
/// one real member (`value->rValue = <prefix>X;`), in source order.
fn real_asks(source: &str, prefix: &str) -> Result<Vec<(String, String)>, String> {
    let source = crate::ifparm::strip_comments(source)?;
    let mut asks: Vec<(String, String)> = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    // The asks recorded for the last case group, dropped again when C goes
    // on to scale the value (`value->rValue *= ...`).
    let mut last = 0..0;
    for line in source.lines() {
        let line = line.trim().trim_start_matches('{').trim();
        if line.starts_with("value->rValue *=") {
            asks.drain(last.clone());
            last = 0..0;
            continue;
        }
        if let Some(rest) = line.strip_prefix("case ") {
            let id = rest.trim_end_matches(':').trim();
            if !id.starts_with("BSIM4_") {
                return Err(format!("ask: unexpected case `{line}`"));
            }
            pending.push(id.into());
        } else if let Some(rest) = line.strip_prefix("value->rValue = ") {
            if let Some(member) = rest
                .strip_suffix(';')
                .map(str::trim)
                .and_then(|r| r.strip_prefix(prefix))
                .filter(|m| m.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'))
            {
                let start = asks.len();
                for id in pending.drain(..) {
                    asks.push((id, member.into()));
                }
                last = start..asks.len();
            }
            pending.clear();
        } else if line.starts_with("value->") || line.starts_with("return") {
            pending.clear();
        }
    }
    Ok(asks)
}

/// The generated `asks.rs` text: the real asks of `b4mask.c` and `b4ask.c`
/// that read a member directly (`sens_getp()` succeeds only for these).
pub(crate) fn asks(mask: &str, ask: &str, revision: &str) -> Result<String, String> {
    let model = real_asks(mask, "model->")?;
    let instance = real_asks(ask, "here->")?;
    if model.len() != 869 || instance.len() != 40 {
        return Err(format!(
            "b4mask.c/b4ask.c: {} model and {} instance plain real asks; review them",
            model.len(),
            instance.len()
        ));
    }
    let mut out = format!(
        "// Generated ask facts (C parameter IDs and the member each real ask\n\
         // reads), not C implementation code. Source: ngspice {revision},\n\
         // bsim4/{{b4mask.c,b4ask.c}}.\n\
         // Regenerate: cargo xtask bsim4-schema --source /path/to/ngspice\n\n\
         /// `b4mask.c`: `(model parameter ID, member)` asks that read one real member.\n\
         #[rustfmt::skip]\npub(crate) const MODEL: &[(&str, &str)] = &[\n"
    );
    for (id, member) in &model {
        writeln!(out, "    (\"{id}\", \"{member}\"),").map_err(|e| e.to_string())?;
    }
    out.push_str("];\n\n/// `b4ask.c`: `(instance parameter ID, member)` asks that read one real member.\n#[rustfmt::skip]\npub(crate) const INSTANCE: &[(&str, &str)] = &[\n");
    for (id, member) in &instance {
        writeln!(out, "    (\"{id}\", \"{member}\"),").map_err(|e| e.to_string())?;
    }
    out.push_str("];\n");
    Ok(out)
}

/// The generated `fields.rs` text for one device: the model, size-dependent
/// and instance structs of `header` (`file`), with the reviewed member counts.
fn device_fields(
    file: &str,
    header: &str,
    revision: &str,
    device: &str,
    counts: [usize; 3],
) -> Result<String, String> {
    let header = crate::ifparm::strip_comments(header)?;
    let upper = device.to_ascii_uppercase();
    let instance = members(
        file,
        &header,
        &format!("typedef struct s{upper}instance\n{{"),
        &format!("}} {upper}instance"),
    )?;
    let size = members(
        file,
        &header,
        &format!("struct {device}SizeDependParam\n{{"),
        "};",
    )?;
    let model = members(
        file,
        &header,
        &format!("typedef struct s{upper}model\n{{"),
        &format!("}} {upper}model"),
    )?;
    for ((label, fields), expected) in [("instance", &instance), ("size", &size), ("model", &model)]
        .into_iter()
        .zip(counts)
    {
        if fields.len() != expected {
            return Err(format!(
                "{file}: {} {label} members, reviewed {expected}; review the header",
                fields.len()
            ));
        }
    }
    let mut out = format!(
        "// Generated field facts (C member names and scalar types), not C\n\
         // implementation code. Source: ngspice {revision}, {device}/{file}.\n\
         // Regenerate: cargo xtask {device}-schema --source /path/to/ngspice\n\
         #![allow(non_snake_case, clippy::struct_excessive_bools)]\n"
    );
    emit_struct(&mut out, "Model", &format!("struct s{upper}model"), &model)?;
    emit_struct(
        &mut out,
        "SizeParam",
        &format!("struct {device}SizeDependParam"),
        &size,
    )?;
    emit_struct(
        &mut out,
        "Instance",
        &format!("struct s{upper}instance"),
        &instance,
    )?;
    Ok(out)
}

/// The generated BSIM4 `fields.rs` text.
pub(crate) fn fields(header: &str, revision: &str) -> Result<String, String> {
    device_fields("bsim4def.h", header, revision, "bsim4", [370, 224, 1824])
}

/// The generated BSIM3 `fields.rs` text.
pub(crate) fn bsim3_fields(header: &str, revision: &str) -> Result<String, String> {
    device_fields("bsim3def.h", header, revision, "bsim3", [140, 120, 868])
}
