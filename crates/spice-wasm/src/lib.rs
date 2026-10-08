//! WebAssembly front end for the Rust ngspice port.
//!
//! A deck and its `.include`/`.lib` files arrive as text and are resolved
//! through [`MemorySources`]; there is no file-system access. [`run`] follows
//! the `spice-rs simulate` pipeline: exactly one analysis, `.save`/`.print`
//! vector selection, and `.measure` and `.four` evaluated over the **full**
//! plot. [`to_json`] and [`simulate_rawfile`] render the result. On `wasm32`
//! [`bindings`] exports `simulate` and `version` to JavaScript.
use spice_analysis::fourier::{self, FourierAnalysis};
use spice_analysis::measure::{self, Measurement};
use spice_analysis::selection::{self, Selection};
use spice_analysis::{Plot, RawFile, RawPlot, RunConfig, runner};
use spice_netlist::ast::AnalysisCard;
use spice_netlist::{MemorySources, Parser, SourceLimits};
use std::fmt::Write as _;

/// The in-memory path of the deck itself. Include paths are relative to it.
pub const DECK_PATH: &str = "/deck.cir";

/// One simulated deck.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// The analysis plot restricted to the `.save` selection (every vector
    /// when the deck has no `.save`), as `spice-rs simulate` writes it.
    pub plot: RawPlot,
    /// The `.print` table, when the deck has a `.print` card.
    pub printed: Option<String>,
    /// `.measure` results in card order.
    pub measurements: Vec<Measurement>,
    /// `.four` results in card order.
    pub fourier: Vec<FourierAnalysis>,
}

/// Parses `deck` (resolving includes against `files`, `(path, text)` pairs),
/// runs its analysis and evaluates its output cards.
///
/// # Errors
///
/// A file that would replace the deck, a deck with no or several analysis
/// cards, or the parser, configuration, analysis, selection, measurement or
/// Fourier error, rendered as text. Nothing is returned on failure.
pub fn run(deck: &str, files: &[(String, String)]) -> Result<Outcome, String> {
    let mut sources = MemorySources::new();
    for (path, text) in files {
        sources.insert(path, text.as_str());
    }
    if sources.get(DECK_PATH).is_some() {
        return Err(format!("file name '{DECK_PATH}' is reserved for the deck"));
    }
    sources.insert(DECK_PATH, deck);
    let text = |e: spice_core::SpiceError| e.to_string();
    let parsed = Parser::new()
        .parse_file_with_sources_and_output(DECK_PATH, &sources, SourceLimits::default())
        .map_err(text)?;
    let netlist = &parsed.netlist;
    let card = only_analysis(&netlist.analyses)?;
    let config = RunConfig::from_netlist(netlist).map_err(text)?;
    let request = config.request_for(card).map_err(text)?;
    let mut circuit = config.circuit(netlist).map_err(text)?;
    let plot = runner(request.kind)
        .and_then(|driver| driver.run(&mut circuit, &request, &config.context()))
        .map_err(text)?;
    // As in `spice-rs simulate`: selection, measurements and Fourier results all
    // resolve against the full plot, and any failure publishes nothing.
    let requests = selection::write_requests(&parsed.output, card.kind).map_err(text)?;
    let selected = Selection::resolve(&plot, card.kind, &requests).map_err(text)?;
    let print_requests = selection::print_requests(&parsed.output, card.kind).map_err(text)?;
    let printed = if print_requests.is_empty() {
        None
    } else {
        Some(
            Selection::resolve(&plot, card.kind, &print_requests)
                .and_then(|printed| printed.to_text(&plot))
                .map_err(text)?,
        )
    };
    let measurements = measure::resolve(&plot, card.kind, &parsed.measurements).map_err(text)?;
    let fourier = fourier::resolve(&plot, card.kind, &parsed.fourier).map_err(text)?;
    let written = selected.apply(&plot).map_err(text)?;
    Ok(Outcome {
        plot: RawPlot {
            title: netlist.title.clone(),
            date: String::new(),
            command: String::new(),
            plot: written,
        },
        printed,
        measurements,
        fourier,
    })
}

fn only_analysis(cards: &[AnalysisCard]) -> Result<&AnalysisCard, String> {
    match cards {
        [only] => Ok(only),
        [] => Err("the deck has no analysis card: add one .op, .dc, .ac or .tran".into()),
        several => Err(format!(
            "{} analysis cards in one deck; like 'spice-rs simulate', each run takes exactly one \
             .op, .dc, .ac or .tran card",
            several.len()
        )),
    }
}

/// [`run`] rendered as one ASCII rawfile (the selected vectors).
///
/// # Errors
/// As [`run`].
pub fn simulate_rawfile(deck: &str, files: &[(String, String)]) -> Result<String, String> {
    Ok(RawFile {
        plots: vec![run(deck, files)?.plot],
    }
    .to_ascii())
}

/// Renders an [`Outcome`] as JSON:
///
/// ```text
/// {"plots":[{"name","type","complex","variables":[{"name","unit","re":[..],"im":[..]?}]}],
///  "printed": string|null,
///  "measurements":[{"name","value","unit","at"}],
///  "fourier":[{"vector","unit","fundamental","dc","thd","harmonics":[{"order","frequency","amplitude","phase"}]}]}
/// ```
///
/// `im` is present only for complex plots; non-finite numbers are `null`.
#[must_use]
pub fn to_json(outcome: &Outcome) -> String {
    let plot = &outcome.plot.plot;
    let mut out = String::from("{\"plots\":[{\"name\":");
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
    out.push_str("]}],\"printed\":");
    match &outcome.printed {
        Some(table) => string(&mut out, table),
        None => out.push_str("null"),
    }
    out.push_str(",\"measurements\":[");
    for (i, m) in outcome.measurements.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":");
        string(&mut out, &m.name);
        out.push_str(",\"value\":");
        number(&mut out, Some(m.value));
        out.push_str(",\"unit\":");
        string(&mut out, &m.unit);
        out.push_str(",\"at\":");
        number(&mut out, m.at);
        out.push('}');
    }
    out.push_str("],\"fourier\":[");
    for (i, f) in outcome.fourier.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str("{\"vector\":");
        string(&mut out, &f.vector);
        out.push_str(",\"unit\":");
        string(&mut out, &f.unit);
        for (key, value) in [("fundamental", f.fundamental), ("dc", f.dc), ("thd", f.thd)] {
            let _ = write!(out, ",\"{key}\":");
            number(&mut out, Some(value));
        }
        out.push_str(",\"harmonics\":[");
        for (j, h) in f.harmonics.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            let _ = write!(out, "{{\"order\":{},\"frequency\":", h.order);
            number(&mut out, Some(h.frequency));
            out.push_str(",\"amplitude\":");
            number(&mut out, Some(h.amplitude));
            out.push_str(",\"phase\":");
            number(&mut out, Some(h.phase));
            out.push('}');
        }
        out.push_str("]}");
    }
    out.push_str("]}");
    out
}

fn number(out: &mut String, value: Option<f64>) {
    match value {
        // `{:?}` is the shortest round-trip form, valid JSON when finite.
        Some(value) if value.is_finite() => {
            let _ = write!(out, "{value:?}");
        }
        _ => out.push_str("null"),
    }
}

fn column(out: &mut String, plot: &Plot, index: usize, part: fn(&spice_core::Complex) -> f64) {
    out.push('[');
    for (i, point) in plot.points.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        number(out, point.get(index).map(part));
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
            .map(|outcome| super::to_json(&outcome))
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
        let deck = "rc\nv1 in 0 dc 0 ac 1\nr1 in out 1k\nc1 out 0 1u\n.ac lin 2 100 1k\n.end\n";
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
        assert!(
            json.ends_with("]}],\"printed\":null,\"measurements\":[],\"fourier\":[]}"),
            "{json}"
        );
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
        let two = "t\nv1 a 0 1\nr1 a 0 1\n.op\n.dc v1 0 1 1\n.end\n";
        assert!(run(two, &[]).unwrap_err().contains("2 analysis cards"));
    }

    #[test]
    fn included_subcircuits_elaborate() {
        let files = vec![(
            "parts/div.inc".to_owned(),
            ".subckt div a b\nr1 a b 1k\n.ends div\n".to_owned(),
        )];
        let deck =
            "s\n.include parts/div.inc\nv1 in 0 dc 10\nx1 in out div\nr2 out 0 1k\n.op\n.end\n";
        let outcome = run(deck, &files).unwrap();
        assert_eq!(outcome.plot.plot.value("v(out)", 0).unwrap().re, 5.0);
    }

    #[test]
    fn output_cards_follow_the_simulate_pipeline() {
        // As crates/spice-cli/tests/simulate.rs: a 1 kHz square wave into an RC
        // low-pass. `.save` keeps only v(in), yet `.measure`/`.four` still see
        // v(out) because they run over the full plot.
        let deck = "rc lowpass\nv1 in 0 pulse(0 1 0 1n 1n 0.5m 1m)\nr1 in out 1k\nc1 out 0 1u\n\
                    .tran 1u 5m\n.save v(in)\n.print tran v(in)\n.meas tran vmax max v(out)\n\
                    .four 1k v(in)\n.end\n";
        let outcome = run(deck, &[]).unwrap();
        let names: Vec<_> = outcome
            .plot
            .plot
            .variables
            .iter()
            .map(|v| v.name.as_str())
            .collect();
        assert_eq!(names, ["time", "v(in)"]);
        // `.print` vectors are saved too (C's `dbs` order), so this one is v(in) to
        // keep v(out) out of the written plot.
        assert!(outcome.printed.as_deref().unwrap().contains("v(in)"));
        assert_eq!(outcome.measurements.len(), 1);
        assert_eq!(outcome.measurements[0].name, "vmax");
        assert!(outcome.measurements[0].value > 0.4 && outcome.measurements[0].value < 1.0);
        let four = &outcome.fourier[0];
        // Square wave: DC 0.5, fundamental amplitude 2/pi.
        assert!((four.dc - 0.5).abs() < 1e-2, "{}", four.dc);
        let first = &four.harmonics[0];
        assert!((first.amplitude - 2.0 / std::f64::consts::PI).abs() < 1e-2);
        let json = to_json(&outcome);
        assert!(
            json.contains("\"measurements\":[{\"name\":\"vmax\",\"value\":"),
            "{json}"
        );
        assert!(
            json.contains("\"fourier\":[{\"vector\":\"v(in)\""),
            "{json}"
        );
        // A measurement of a missing vector fails the whole run.
        let bad = deck.replace("max v(out)", "max v(nosuch)");
        assert!(run(&bad, &[]).is_err());
    }
}
