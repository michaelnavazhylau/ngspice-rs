//! WebAssembly front end for the Rust ngspice port.
//!
//! A deck and its `.include`/`.lib` files arrive as text and are resolved
//! through [`MemorySources`]; there is no file-system access. [`run`] returns
//! the plots; [`to_json`] and [`RawFile::to_ascii`] render them. On `wasm32`
//! [`bindings`] exports `simulate` and `version` to JavaScript.
use spice_analysis::{Plot, RawFile, RawPlot, RunConfig, runner};
use spice_netlist::{MemorySources, Parser, SourceLimits};
use std::fmt::Write as _;

/// The in-memory path of the deck itself. Include paths are relative to it.
pub const DECK_PATH: &str = "/deck.cir";

/// Parses `deck` (resolving includes against `files`, `(path, text)` pairs)
/// and runs every analysis card in order.
///
/// # Errors
///
/// A file that would replace the deck, or the parser, configuration or
/// analysis error, rendered as text.
pub fn run(deck: &str, files: &[(String, String)]) -> Result<Vec<RawPlot>, String> {
    let mut sources = MemorySources::new();
    for (path, text) in files {
        sources.insert(path, text.as_str());
    }
    if sources.get(DECK_PATH).is_some() {
        return Err(format!("file name '{DECK_PATH}' is reserved for the deck"));
    }
    sources.insert(DECK_PATH, deck);
    let netlist = Parser::new()
        .parse_file_with_sources(DECK_PATH, &sources, SourceLimits::default())
        .map_err(|e| e.to_string())?;
    if netlist.analyses.is_empty() {
        return Err("the deck has no analysis card (.op, .dc, .ac or .tran)".into());
    }
    let config = RunConfig::from_netlist(&netlist).map_err(|e| e.to_string())?;
    let mut plots = Vec::new();
    for card in &netlist.analyses {
        let request = config.request_for(card).map_err(|e| e.to_string())?;
        let mut circuit = config.circuit(&netlist).map_err(|e| e.to_string())?;
        let plot = runner(request.kind)
            .and_then(|driver| driver.run(&mut circuit, &request, &config.context()))
            .map_err(|e| e.to_string())?;
        plots.push(RawPlot {
            title: netlist.title.clone(),
            date: String::new(),
            command: String::new(),
            plot,
        });
    }
    Ok(plots)
}

/// [`run`] rendered as one ASCII rawfile.
///
/// # Errors
/// As [`run`].
pub fn simulate_rawfile(deck: &str, files: &[(String, String)]) -> Result<String, String> {
    Ok(RawFile {
        plots: run(deck, files)?,
    }
    .to_ascii())
}

/// Renders plots as JSON:
/// `{"plots":[{"name","type","complex","variables":[{"name","unit","re":[..],"im":[..]?}]}]}`.
/// `im` is present only for complex plots; non-finite values are `null`.
#[must_use]
pub fn to_json(plots: &[RawPlot]) -> String {
    let mut out = String::from("{\"plots\":[");
    for (p, RawPlot { plot, .. }) in plots.iter().enumerate() {
        if p > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":");
        string(&mut out, &plot.name);
        out.push_str(",\"type\":");
        string(&mut out, &plot.plotname);
        let complex = plot.flags.is_complex();
        let _ = write!(out, ",\"complex\":{complex},\"variables\":[");
        for (v, variable) in plot.variables.iter().enumerate() {
            if v > 0 {
                out.push(',');
            }
            out.push_str("{\"name\":");
            string(&mut out, &variable.name);
            out.push_str(",\"unit\":");
            string(&mut out, &variable.unit);
            out.push_str(",\"re\":");
            column(&mut out, plot, v, |c| c.re);
            if complex {
                out.push_str(",\"im\":");
                column(&mut out, plot, v, |c| c.im);
            }
            out.push('}');
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    out
}

fn column(out: &mut String, plot: &Plot, index: usize, part: fn(&spice_core::Complex) -> f64) {
    out.push('[');
    for (i, point) in plot.points.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        match point.get(index).map(part) {
            // `{:?}` is the shortest round-trip form, valid JSON when finite.
            Some(value) if value.is_finite() => {
                let _ = write!(out, "{value:?}");
            }
            _ => out.push_str("null"),
        }
    }
    out.push(']');
}

fn string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// JavaScript bindings (wasm32 only).
#[cfg(target_arch = "wasm32")]
pub mod bindings {
    use wasm_bindgen::prelude::*;

    /// Runs `deck` with `file_names[i]` holding `file_contents[i]` and returns
    /// the plots as JSON (see [`super::to_json`]). Throws the error text.
    #[wasm_bindgen]
    pub fn simulate(
        deck: &str,
        file_names: Vec<String>,
        file_contents: Vec<String>,
    ) -> Result<String, JsError> {
        if file_names.len() != file_contents.len() {
            return Err(JsError::new(
                "file_names and file_contents differ in length",
            ));
        }
        let files: Vec<_> = file_names.into_iter().zip(file_contents).collect();
        super::run(deck, &files)
            .map(|plots| super::to_json(&plots))
            .map_err(|e| JsError::new(&e))
    }

    /// The engine version.
    #[wasm_bindgen]
    #[must_use]
    pub fn version() -> String {
        format!("spice-rs {}", env!("CARGO_PKG_VERSION"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_has_one_column_per_variable_and_escapes_strings() {
        let deck =
            "rc\nv1 in 0 dc 0 ac 1\nr1 in out 1k\nc1 out 0 1u\n.ac lin 2 100 1k\n.op\n.end\n";
        let json = to_json(&run(deck, &[]).unwrap());
        assert!(
            json.starts_with(
                "{\"plots\":[{\"name\":\"ac1\",\"type\":\"AC Analysis\",\"complex\":true,"
            ),
            "{json}"
        );
        assert!(
            json.contains(
                "{\"name\":\"frequency\",\"unit\":\"frequency\",\"re\":[100.0,1000.0],\"im\":[0.0,0.0]}"
            ),
            "{json}"
        );
        assert!(json.contains("\"name\":\"op1\""), "{json}");
        let mut text = String::new();
        string(&mut text, "a\"b\\c\n");
        assert_eq!(text, r#""a\"b\\c\u000a""#);
    }

    #[test]
    fn files_resolve_relative_to_the_deck_and_cannot_replace_it() {
        let files = vec![("lib/r.inc".to_owned(), "r2 out 0 1k\n".to_owned())];
        let deck = "d\n.include lib/r.inc\nv1 in 0 dc 2\nr1 in out 1k\n.op\n.end\n";
        let json = to_json(&run(deck, &files).unwrap());
        assert!(
            json.contains("{\"name\":\"v(out)\",\"unit\":\"voltage\",\"re\":[1.0]}"),
            "{json}"
        );
        let clash = vec![("deck.cir".to_owned(), String::new())];
        assert!(run(deck, &clash).unwrap_err().contains("reserved"));
        assert!(
            run("t\nr1 a 0 1\n.end\n", &[])
                .unwrap_err()
                .contains("no analysis")
        );
    }
}
