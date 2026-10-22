//! Streamed output (M10 0.5g): [`Analysis::run_into`] hands an analysis's
//! points to a [`PointSink`] instead of storing them, and [`RawFileWriter`]
//! writes plots one at a time, spooling a streamed plot's points until its
//! count is known. Both must reproduce the stored path exactly: the same rows,
//! and a rawfile byte-identical to [`RawFile::write_with_format`].

use std::fs;
use std::path::{Path, PathBuf};

use ngspice_rs::analysis::{
    Plot, PlotFlags, PointSink, RawFile, RawFileWriter, RawFormat, RawPlot, RunConfig, Variable,
    runner,
};
use ngspice_rs::netlist::Parser;
use ngspice_rs::netlist::ast::Netlist;
use ngspice_rs::netlist::source::parse_deck_text;
use ngspice_rs::primitives::{Complex, SpiceError, SpiceResult};

fn deck(body: &str) -> Netlist {
    Parser::new()
        .parse_deck(&parse_deck_text(
            Path::new("stream.cir"),
            &format!("streamed output\n{body}\n.end\n"),
        ))
        .unwrap()
}

/// A private scratch directory. `name` keeps concurrent tests apart.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("spice-rs-stream-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("readable directory")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Records what an analysis hands its sink.
#[derive(Default)]
struct Recorder {
    header: Option<Plot>,
    points: Vec<Vec<Complex>>,
}

impl PointSink for Recorder {
    fn begin(&mut self, header: &Plot) -> SpiceResult<()> {
        assert!(self.header.is_none(), "begin is called once");
        assert!(header.points.is_empty(), "the header holds no points");
        self.header = Some(header.clone());
        Ok(())
    }

    fn point(&mut self, values: &[Complex]) -> SpiceResult<()> {
        assert!(self.header.is_some(), "begin precedes the points");
        self.points.push(values.to_vec());
        Ok(())
    }
}

/// Runs the deck's first analysis stored and streamed and checks that the
/// sink saw exactly the stored plot.
fn assert_streams_the_stored_plot(body: &str) {
    let netlist = deck(body);
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let driver = runner(request.kind).unwrap();

    let mut circuit = config.circuit(&netlist).unwrap();
    let stored = driver
        .run(&mut circuit, &request, &config.context())
        .unwrap();
    let final_state = circuit.final_transient_state();

    let mut circuit = config.circuit(&netlist).unwrap();
    let mut recorder = Recorder::default();
    let returned = driver
        .run_into(&mut circuit, &request, &config.context(), &mut recorder)
        .unwrap();

    assert!(returned.points.is_empty(), "run_into returns no points");
    let header = recorder.header.expect("the sink saw a header");
    assert_eq!(header.variables, stored.variables);
    assert_eq!(header.plotname, stored.plotname);
    assert_eq!(returned.variables, stored.variables);
    assert!(!stored.points.is_empty());
    // Bit-identical rows: streaming changes where points go, not what they are.
    assert_eq!(recorder.points, stored.points);
    assert_eq!(circuit.final_transient_state(), final_state);
}

#[test]
fn a_streamed_transient_hands_the_sink_every_stored_row() {
    assert_streams_the_stored_plot(
        "v1 in 0 pulse(0 1 1m 1u 1u 1m 2m)\nr1 in out 1k\nc1 out 0 1u\n.tran 0.1m 6m",
    );
    // A delayed start writes only the rows at or after tstart.
    assert_streams_the_stored_plot(
        "v1 in 0 pulse(0 1 1m 1u 1u 1m 2m)\nr1 in out 1k\nc1 out 0 1u\n.tran 0.1m 6m 2m",
    );
}

#[test]
fn a_streamed_mos_transient_carries_the_same_final_load_as_the_stored_path() {
    assert_streams_the_stored_plot(
        "vd d 0 dc 3\nvg g 0 dc 2 ac 1\nvs s 0 0\n\
         vb b 0 dc -0.3 sin(-0.3 0.2 100k)\n\
         m1 d g s b nm l=10u w=20u ad=10p as=20p nrd=2 nrs=3\n\
         .model nm nmos level=1 vto=0.7 rsh=100 cj=1e-4 cgso=1e-10\n\
         .tran 0.2u 1u",
    );
}

#[test]
fn the_default_run_into_replays_the_stored_plot() {
    assert_streams_the_stored_plot("v1 in 0 dc 5\nr1 in out 1k\nr2 out 0 1k\n.op");
    assert_streams_the_stored_plot("v1 in 0 dc 5 ac 1\nr1 in out 1k\nc1 out 0 1u\n.ac dec 5 1 1k");
}

/// A sink that fails on its third point.
struct Refusing(usize);

impl PointSink for Refusing {
    fn begin(&mut self, _header: &Plot) -> SpiceResult<()> {
        Ok(())
    }

    fn point(&mut self, _values: &[Complex]) -> SpiceResult<()> {
        self.0 += 1;
        if self.0 == 3 {
            return Err(SpiceError::Unsupported {
                feature: "sink refused".to_owned(),
                location: None,
            });
        }
        Ok(())
    }
}

#[test]
fn a_sink_error_stops_the_transient_with_that_error() {
    let netlist = deck("v1 in 0 dc 1\nr1 in out 1k\nc1 out 0 1u\n.tran 0.1m 1m");
    let config = RunConfig::from_netlist(&netlist).unwrap();
    let request = config.request_for(&netlist.analyses[0]).unwrap();
    let mut circuit = config.circuit(&netlist).unwrap();
    let mut sink = Refusing(0);
    let error = runner(request.kind)
        .unwrap()
        .run_into(&mut circuit, &request, &config.context(), &mut sink)
        .unwrap_err();
    assert!(error.to_string().contains("sink refused"), "{error}");
    assert_eq!(sink.0, 3, "no point is offered after the failing one");
    assert!(circuit.final_transient_state().is_none());

    // A later failed run must not publish its partial accepted history over
    // the last successful transient's snapshot, either.
    runner(request.kind)
        .unwrap()
        .run(&mut circuit, &request, &config.context())
        .unwrap();
    let before = circuit
        .final_transient_state()
        .expect("successful run retained its load");
    let mut sink = Refusing(0);
    assert!(
        runner(request.kind)
            .unwrap()
            .run_into(&mut circuit, &request, &config.context(), &mut sink)
            .is_err()
    );
    assert_eq!(circuit.final_transient_state(), Some(before));
}

fn sample_plots() -> RawFile {
    let mut real = Plot::new("tran1", "Transient Analysis", PlotFlags::Real);
    real.push_variable(Variable::new("time", "time"));
    real.push_variable(Variable::new("v(out)", "voltage"));
    for index in 0..5 {
        let t = f64::from(index) * 1e-4;
        real.push_point(vec![Complex::real(t), Complex::real((-t / 1e-3).exp())])
            .unwrap();
    }
    let mut complex = Plot::new("ac1", "AC Analysis", PlotFlags::Complex);
    complex.push_variable(Variable::new("frequency", "frequency"));
    complex.push_variable(Variable::complex("v(out)", "voltage"));
    for index in 0..3 {
        let f = 10f64.powi(index);
        complex
            .push_point(vec![Complex::real(f), Complex::new(1.0 / f, -0.5 / f)])
            .unwrap();
    }
    let mut empty = Plot::new("tran2", "Transient Analysis", PlotFlags::Real);
    empty.push_variable(Variable::new("time", "time"));
    RawFile {
        plots: [real, complex, empty]
            .into_iter()
            .map(|plot| RawPlot {
                title: "streamed output".to_owned(),
                date: "Thu Jan  1 00:00:00 1970".to_owned(),
                command: "test".to_owned(),
                plot,
            })
            .collect(),
    }
}

#[test]
fn streamed_and_stored_plots_write_the_same_bytes() {
    let dir = scratch("writer");
    let rawfile = sample_plots();
    for format in [RawFormat::Ascii, RawFormat::Binary] {
        let stored = dir.join("stored.raw");
        rawfile.write_with_format(&stored, format).unwrap();

        // The first plot whole, the rest streamed point by point.
        let streamed = dir.join("streamed.raw");
        let spool = dir.join("points.spool");
        let mut writer = RawFileWriter::create(&streamed, format).unwrap();
        writer.write_plot(&rawfile.plots[0]).unwrap();
        for raw_plot in &rawfile.plots[1..] {
            let mut plot = writer.begin_plot(raw_plot.clone(), &spool).unwrap();
            assert!(plot.header().plot.points.is_empty());
            for point in &raw_plot.plot.points {
                plot.push_point(point).unwrap();
            }
            assert_eq!(plot.point_count(), raw_plot.plot.points.len());
            writer.finish_plot(plot).unwrap();
            assert!(!spool.exists(), "finishing removes the spool");
        }
        writer.finish().unwrap();

        assert_eq!(
            fs::read(&streamed).unwrap(),
            fs::read(&stored).unwrap(),
            "{format:?}"
        );
        // Loading renames the plots and ASCII rounds decimals (both known
        // round-trip limits), so the loaded files are compared with each other.
        assert_eq!(
            RawFile::load(&streamed).unwrap(),
            RawFile::load(&stored).unwrap(),
            "{format:?}"
        );
    }
    assert_eq!(entries(&dir), ["stored.raw", "streamed.raw"]);
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_streamed_plot_rejects_ragged_points_and_cleans_up_when_abandoned() {
    let dir = scratch("abandon");
    let rawfile = sample_plots();
    let spool = dir.join("points.spool");
    let writer = RawFileWriter::create(dir.join("out.raw"), RawFormat::Binary).unwrap();
    let mut plot = writer.begin_plot(rawfile.plots[0].clone(), &spool).unwrap();
    plot.push_point(&[Complex::real(0.0), Complex::real(1.0)])
        .unwrap();
    let error = plot.push_point(&[Complex::real(0.0)]).unwrap_err();
    assert!(error.to_string().contains("1 value(s)"), "{error}");
    assert_eq!(plot.point_count(), 1, "a rejected point is not counted");

    // An existing spool is never overwritten.
    let error = writer
        .begin_plot(rawfile.plots[1].clone(), &spool)
        .unwrap_err();
    assert!(matches!(error, SpiceError::Io { .. }), "{error}");

    assert!(spool.exists());
    drop(plot);
    assert!(
        !spool.exists(),
        "dropping an unfinished plot removes its spool"
    );
    drop(writer);
    assert_eq!(entries(&dir), ["out.raw"]);
    fs::remove_dir_all(&dir).unwrap();
}
