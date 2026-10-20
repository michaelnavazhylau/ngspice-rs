//! Extract the numeric field declarations of BSIM4's three C structs
//! (`sBSIM4model`, `bsim4SizeDependParam`, `sBSIM4instance`) as Rust struct
//! definitions: field names and scalar types only. The BSIM4 port keeps C's
//! field names (without the `BSIM4` prefix) so that `b4set.c`, `b4temp.c` and
//! `b4ld.c` can be followed statement by statement. No formula, default or
//! comment text is copied; pointers, arrays and matrix bindings are skipped.
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
fn members(header: &str, opening: &str, closing: &str) -> Result<Vec<Field>, String> {
    let start = header
        .find(opening)
        .ok_or_else(|| format!("bsim4def.h: missing `{opening}`"))?;
    let body = &header[start + opening.len()..];
    let end = body
        .find(closing)
        .ok_or_else(|| format!("bsim4def.h: unterminated `{opening}`"))?;
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
            _ => return Err(format!("bsim4def.h: unrecognised member `{line}`")),
        };
        if !field
            .c_name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(format!("bsim4def.h: unexpected member name `{line}`"));
        }
        fields.push(field);
    }
    Ok(fields)
}

/// The Rust spelling of a C member: the `BSIM4` prefix removed, a raw
/// identifier for Rust keywords.
fn rust_name(c_name: &str) -> String {
    let name = c_name.strip_prefix("BSIM4").unwrap_or(c_name);
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
    out.push_str("}\n");
    Ok(())
}

/// The generated `fields.rs` text.
pub(crate) fn fields(header: &str, revision: &str) -> Result<String, String> {
    let header = crate::ifparm::strip_comments(header)?;
    let instance = members(
        &header,
        "typedef struct sBSIM4instance\n{",
        "} BSIM4instance",
    )?;
    let size = members(&header, "struct bsim4SizeDependParam\n{", "};")?;
    let model = members(&header, "typedef struct sBSIM4model\n{", "} BSIM4model")?;
    for (label, fields, expected) in [
        ("instance", &instance, 370),
        ("size", &size, 224),
        ("model", &model, 1824),
    ] {
        if fields.len() != expected {
            return Err(format!(
                "bsim4def.h: {} {label} members, reviewed {expected}; review the header",
                fields.len()
            ));
        }
    }
    let mut out = format!(
        "// Generated field facts (C member names and scalar types), not C\n\
         // implementation code. Source: ngspice {revision}, bsim4/bsim4def.h.\n\
         // Regenerate: cargo xtask bsim4-schema --source /path/to/ngspice\n\
         #![allow(non_snake_case, clippy::struct_excessive_bools)]\n"
    );
    emit_struct(&mut out, "Model", "struct sBSIM4model", &model)?;
    emit_struct(&mut out, "SizeParam", "struct bsim4SizeDependParam", &size)?;
    emit_struct(&mut out, "Instance", "struct sBSIM4instance", &instance)?;
    Ok(out)
}
