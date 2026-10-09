//! `.sens` — DC and AC sensitivity of one output to every device parameter.
//!
//! C (behaviour only, reimplemented): `sens_sens()` in
//! `src/spicelib/analysis/cktsens.c`, the parameter generator of `cktsgen.c`,
//! the card grammar of `dot_sens()` in `src/spicelib/parser/inp2dot.c` and the
//! job parameters of `senssetp.c`. The reference binary registers `.sens` as
//! this finite-difference analysis (`SENSinfo`); the SPICE2 adjoint
//! `*sload.c`/`*sset.c` code belongs to the separate `WANT_SENSE2` `.sens2`
//! analysis, which the reference build does not compile.
//!
//! ```text
//! .sens v(n[,m])|i(vsrc) [filter ...] [dc | ac dec|oct|lin pts fstart fstop]
//! ```
//!
//! 1. The DC operating point is solved as for `.op`.
//! 2. **DC**: `Y` is the operating-point Jacobian (C's `DEVload` matrix,
//!    [`crate::devices::TrialState::with_c_jacobian`]) factored once and `x`
//!    the operating point. **AC**: at every frequency the circuit is set up,
//!    temperature-updated and assembled again from its (by then perturbed and
//!    restored) records, `Y = A + j omega E` is factored and `x` solved.
//! 3. For every parameter, in C's `sgen` order, the one device is loaded with
//!    its current records (`dY0`, `dI0`), the parameter is set to `p + delta`
//!    with `delta = 1e-6 p` (`1e-6` when `p = 0`), the device is loaded again
//!    (`dY1`, `dI1`) and the parameter is set back to `p`. The sensitivity is
//!    `Y^-1 ((dI1 - dI0) - (dY1 - dY0) x)` read at the output and divided by
//!    `delta`.
//!
//! The device side, including C's side effects of the in-place perturbation,
//! lives in [`crate::devices::sensitivity`]; see `docs/port/SENSITIVITY.md`.

use std::collections::BTreeMap;

use crate::analysis::linear::{number, unsupported};
use crate::analysis::results::{PlotFlags, Variable};
use crate::analysis::{AnalysisContext, AnalysisRequest, Plot};
use crate::devices::linear::LinearSystem;
use crate::devices::sensitivity::{
    DeviceSensitivity, ParameterScope, RecordPair, SensitivityFamily, SensitivityLoad,
    SensitivityMode, SensitivityRecord,
};
use crate::devices::{AnalysisMode, Circuit, Device, IterationPhase, LoadRequest, TrialState};
use crate::maths::complex::ComplexMatrix;
use crate::maths::{SparseMatrix, Vector};
use crate::primitives::{
    Complex, NodeKind, NodeTable, Real, SpiceError, SpiceResult, parse_spice_number,
};

/// `Sens_Delta` of `cktsens.c`: the relative perturbation.
pub(crate) const RELATIVE_DELTA: Real = 1e-6;
/// `Sens_Abs_Delta` of `cktsens.c`: the perturbation of a zero parameter.
pub(crate) const ABSOLUTE_DELTA: Real = 1e-6;
/// The plot title (`dot_sens()` names the job "Sensitivity Analysis").
pub(crate) const PLOTNAME: &str = "Sensitivity Analysis";
/// `M_LOG2E`, which `count_steps()` divides by for octave sweeps.
const LOG2E: Real = std::f64::consts::LOG2_E;
/// The port's bound on the number of frequencies.
const MAX_POINTS: usize = 100_000;

/// The `.sens` output variable.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Output {
    /// `v(pos)` or `v(pos,neg)`.
    Voltage { pos: String, neg: Option<String> },
    /// `i(source)`.
    Current { source: String },
}

/// The AC stepping of `senssetp.c` (`SENS_DECADE`, `SENS_OCTAVE`,
/// `SENS_LINEAR`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stepping {
    Decade,
    Octave,
    Linear,
}

/// An AC sweep as written: `ac kind pts fstart fstop`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Sweep {
    stepping: Stepping,
    steps: i64,
    start: Real,
    stop: Real,
}

/// A parsed `.sens` card.
#[derive(Debug, Clone, PartialEq)]
struct SensCard {
    output: Output,
    /// Name filters (`Sens_filter`), lowercased; empty for every parameter.
    filters: Vec<String>,
    /// `None` for DC.
    sweep: Option<Sweep>,
}

fn syntax(message: impl std::fmt::Display) -> SpiceError {
    unsupported(format!(
        ".sens {message}; expected `.sens v(n[,m])|i(vsrc) [filter ...] [dc | ac dec|oct|lin \
         pts fstart fstop]` (C: dot_sens in inp2dot.c)"
    ))
}

/// C's `INPgetValue(IF_INTEGER)`: `floor(value + 0.5)`.
fn integer(text: Option<&str>) -> SpiceResult<i64> {
    let value = text
        .and_then(parse_spice_number)
        .filter(|v| v.is_finite())
        .ok_or_else(|| syntax("expects an integer point count"))?;
    let rounded = (value + 0.5).floor();
    if rounded.abs() > Real::from(i32::MAX) {
        return Err(syntax("point count is out of range"));
    }
    Ok(rounded as i64)
}

/// Parses the arguments of a `.sens` card (`dot_sens()`).
fn parse(arguments: &[String]) -> SpiceResult<SensCard> {
    let mut tokens = arguments.iter().map(String::as_str).peekable();
    let expect = |wanted: &str, token: Option<&str>| match token {
        Some(text) if text == wanted => Ok(()),
        Some(text) => Err(syntax(format!("expected '{wanted}', found '{text}'"))),
        None => Err(syntax(format!("is missing '{wanted}'"))),
    };
    let name = |token: Option<&str>, what: &str| -> SpiceResult<String> {
        match token {
            Some(text) if !matches!(text, "(" | ")" | ",") => Ok(text.to_ascii_lowercase()),
            Some(text) => Err(syntax(format!("expected {what}, found '{text}'"))),
            None => Err(syntax(format!("is missing {what}"))),
        }
    };
    let kind = tokens
        .next()
        .ok_or_else(|| syntax("has no output variable"))?
        .to_ascii_lowercase();
    let output = match kind.as_str() {
        "v" => {
            expect("(", tokens.next())?;
            let pos = name(tokens.next(), "an output node")?;
            if tokens.peek().is_some_and(|token| *token == ",") {
                tokens.next();
            }
            let neg = if tokens.peek().is_some_and(|token| *token == ")") {
                None
            } else {
                Some(name(tokens.next(), "the negative output node")?)
            };
            expect(")", tokens.next())?;
            Output::Voltage { pos, neg }
        }
        "i" => {
            expect("(", tokens.next())?;
            let source = name(tokens.next(), "an output source")?;
            expect(")", tokens.next())?;
            Output::Current { source }
        }
        other => {
            return Err(syntax(format!(
                "output '{other}' is neither a voltage v(...) nor a current i(...)"
            )));
        }
    };
    let mut filters = Vec::new();
    let mut sweep = None;
    while let Some(token) = tokens.next() {
        let lower = token.to_ascii_lowercase();
        match lower.as_str() {
            "dc" => break,
            "ac" => {
                let stepping = match tokens.next().map(str::to_ascii_lowercase).as_deref() {
                    Some("dec") => Stepping::Decade,
                    Some("oct") => Stepping::Octave,
                    Some("lin") => Stepping::Linear,
                    Some(other) => {
                        return Err(syntax(format!(
                            "AC stepping must be dec, oct or lin, not '{other}'"
                        )));
                    }
                    None => return Err(syntax("is missing the AC stepping")),
                };
                let steps = integer(tokens.next())?;
                let start = number(tokens.next(), ".sens start frequency")?;
                let stop = number(tokens.next(), ".sens stop frequency")?;
                sweep = Some(Sweep {
                    stepping,
                    steps,
                    start,
                    stop,
                });
                break;
            }
            "(" | ")" | "," => {
                return Err(syntax(format!(
                    "has an unexpected '{token}' among the filters"
                )));
            }
            _ if token.contains('=') => {
                return Err(syntax(format!("has an unexpected assignment '{token}'")));
            }
            _ => filters.push(lower),
        }
    }
    if let Some(extra) = tokens.next() {
        return Err(syntax(format!(
            "has an unexpected argument '{extra}' (C ignores it)"
        )));
    }
    Ok(SensCard {
        output,
        filters,
        sweep,
    })
}

/// The frequencies of an AC sweep, exactly as `count_steps()` counts them and
/// `inc_freq()` steps them in `cktsens.c`. Both carry C defects the port
/// reproduces: `inc_freq()` compares the step type with the noise/distortion
/// `LINEAR` constant (3) instead of `SENS_LINEAR`, so a `lin` sweep
/// *multiplies* by its step `(fstop - fstart)/pts`; and the octave count
/// divides `ln(fstop/fstart)` by `M_LOG2E` instead of multiplying.
fn frequencies(sweep: &Sweep) -> SpiceResult<Vec<Real>> {
    let steps = sweep.steps.max(1) as Real;
    let (mut low, mut high) = (sweep.start, sweep.stop);
    let (count, factor) = match sweep.stepping {
        Stepping::Linear => (steps, (high - low) / steps),
        Stepping::Decade => {
            if low <= 0. {
                low = 1e-3;
            }
            if high <= low {
                high = 10. * low;
            }
            (
                (steps * (high / low).log10() + 1.01).trunc(),
                10f64.powf(1. / steps),
            )
        }
        Stepping::Octave => {
            if low <= 0. {
                low = 1e-3;
            }
            if high <= low {
                high = 2. * low;
            }
            (
                (steps * (high / low).ln() / LOG2E + 1.01).trunc(),
                2f64.powf(1. / steps),
            )
        }
    };
    if !count.is_finite() || count > MAX_POINTS as Real {
        return Err(unsupported(format!(
            ".sens AC sweep has more than {MAX_POINTS} frequencies"
        )));
    }
    let count = if count <= 0. { 1 } else { count as usize };
    let mut frequencies = Vec::with_capacity(count);
    let mut frequency = sweep.start;
    for _ in 0..count {
        if !(frequency.is_finite() && frequency > 0.) {
            return Err(unsupported(format!(
                ".sens AC frequency {frequency} is not positive and finite (C would \
                 silently use the DC operating-point matrix for a zero frequency)"
            )));
        }
        frequencies.push(frequency);
        frequency *= factor;
    }
    Ok(frequencies)
}

/// `scan()` of `cktsens.c`: `*` matches any run, `?` any one character.
fn scan(filter: &[u8], name: &[u8]) -> bool {
    let (mut f, mut n) = (0, 0);
    while f < filter.len() && n < name.len() {
        if filter[f] == b'*' {
            if f + 1 == filter.len() {
                return true;
            }
            while n < name.len() && !scan(&filter[f + 1..], &name[n..]) {
                n += 1;
            }
            return n < name.len();
        }
        if filter[f] == name[n] || filter[f] == b'?' {
            f += 1;
            n += 1;
        } else {
            return false;
        }
    }
    n == name.len() && (f == filter.len() || (filter[f] == b'*' && f + 1 == filter.len()))
}

/// Where the output is read.
#[derive(Debug, Clone, Copy)]
enum Probe {
    Voltage {
        pos: Option<usize>,
        neg: Option<usize>,
    },
    Current {
        row: usize,
    },
}

impl Probe {
    fn read<T: Copy + std::ops::Sub<Output = T>>(&self, x: &[T], zero: T) -> T {
        let at = |row: Option<usize>| row.and_then(|row| x.get(row).copied()).unwrap_or(zero);
        match *self {
            Self::Voltage { pos, neg } => at(pos) - at(neg),
            Self::Current { row } => at(Some(row)),
        }
    }
}

fn not_found(message: String) -> SpiceError {
    SpiceError::Circuit {
        message: format!("{message} (C: sens_sens in cktsens.c)"),
    }
}

fn node_row(circuit: &Circuit, name: &str) -> SpiceResult<Option<usize>> {
    let canonical = NodeTable::canonical_name(name, circuit.nodes().auto_gnd());
    let id = circuit
        .nodes()
        .get(&canonical)
        .ok_or_else(|| not_found(format!(".sens output node {name} is not in the circuit")))?;
    let row = circuit.unknowns().node_row(id);
    let ground = circuit
        .nodes()
        .node(id)
        .is_some_and(|node| node.kind == NodeKind::Ground);
    if row.is_none() && !ground {
        return Err(not_found(format!(
            ".sens output node {name} has no matrix row"
        )));
    }
    Ok(row)
}

fn resolve_output(circuit: &Circuit, output: &Output) -> SpiceResult<Probe> {
    match output {
        Output::Voltage { pos, neg } => Ok(Probe::Voltage {
            pos: node_row(circuit, pos)?,
            neg: match neg {
                Some(neg) => node_row(circuit, neg)?,
                None => None,
            },
        }),
        Output::Current { source } => {
            let device = circuit
                .devices()
                .iter()
                .position(|device| device.name().eq_ignore_ascii_case(source))
                .ok_or_else(|| {
                    not_found(format!(
                        ".sens output source {source} is not in the circuit"
                    ))
                })?;
            let branch = circuit.devices()[device].findable_branch().ok_or_else(|| {
                unsupported(format!(
                    ".sens i({source}): the device has no findable branch current (only \
                     independent voltage sources, E/H sources and voltage B sources can be \
                     sensed, as by CKTfndBranch; C would silently read ground)"
                ))
            })?;
            let row = circuit
                .branch_rows(device)
                .and_then(|mut rows| rows.nth(branch))
                .ok_or_else(|| not_found(format!("{source}: missing branch row")))?;
            Ok(Probe::Current { row })
        }
    }
}

/// One device in `sgen` order.
struct Member<'a> {
    index: usize,
    name: String,
    sensitivity: Box<dyn DeviceSensitivity + 'a>,
    /// The key of its model in [`Records::models`].
    model: usize,
}

/// One perturbable parameter, in output order.
#[derive(Debug, Clone)]
struct Entry {
    member: usize,
    scope: ParameterScope,
    keyword: &'static str,
    /// The output vector name, `None` when a filter excludes it (C skips the
    /// parameter entirely: it is neither perturbed nor restored).
    name: Option<String>,
}

/// The live records, which C mutates as the analysis proceeds.
struct Records {
    models: Vec<SensitivityRecord>,
    instances: Vec<SensitivityRecord>,
}

impl Records {
    fn pair(
        &mut self,
        member: &Member<'_>,
        slot: usize,
    ) -> (&mut SensitivityRecord, &mut SensitivityRecord) {
        (&mut self.models[member.model], &mut self.instances[slot])
    }
}

/// Every device's sensitivity description in C's `sgen` order: by device type
/// in `DEVices[]` order, then by model in reverse order of model creation (a
/// model is created when the first instance naming it is parsed; instances
/// without a model share the type's default model), then by instance in
/// reverse deck order.
fn members<'a>(
    circuit: &'a Circuit,
    context: &crate::devices::ModelContext,
) -> SpiceResult<(Vec<Member<'a>>, Records)> {
    let mut keys: Vec<(SensitivityFamily, Option<String>)> = Vec::new();
    let mut models: Vec<SensitivityRecord> = Vec::new();
    let mut ordered: Vec<(
        SensitivityFamily,
        usize,
        usize,
        Member<'a>,
        SensitivityRecord,
    )> = Vec::new();
    for (index, device) in circuit.devices().iter().enumerate() {
        let sensitivity = device.sensitivity(context)?;
        let (model_record, instance_record) = sensitivity.records()?;
        let key = (
            sensitivity.family(),
            sensitivity.model().map(str::to_ascii_lowercase),
        );
        let rank = match keys.iter().position(|known| *known == key) {
            Some(rank) => rank,
            None => {
                keys.push(key.clone());
                models.push(model_record);
                keys.len() - 1
            }
        };
        ordered.push((
            key.0,
            rank,
            index,
            Member {
                index,
                name: device.name().to_ascii_lowercase(),
                sensitivity,
                model: rank,
            },
            instance_record,
        ));
    }
    ordered.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)).then(b.2.cmp(&a.2)));
    let mut members = Vec::with_capacity(ordered.len());
    let mut instances = Vec::with_capacity(ordered.len());
    for (_, _, _, member, record) in ordered {
        members.push(member);
        instances.push(record);
    }
    Ok((members, Records { models, instances }))
}

/// The parameters C perturbs, with their output names (`sens_sens()`):
/// `<inst>:<kw>` for a model parameter, `<inst>` for the first principal
/// instance parameter, `<inst>_<kw>` otherwise.
fn entries(members: &[Member<'_>], ac: bool, filters: &[String]) -> Vec<Entry> {
    let mut entries = Vec::new();
    for (slot, member) in members.iter().enumerate() {
        let tables = [
            (ParameterScope::Model, member.sensitivity.model_parameters()),
            (
                ParameterScope::Instance,
                member.sensitivity.instance_parameters(),
            ),
        ];
        let mut principals = 0;
        for (scope, table) in tables {
            for parameter in table.iter().filter(|parameter| ac || !parameter.ac) {
                let name = match scope {
                    ParameterScope::Model => format!("{}:{}", member.name, parameter.keyword),
                    ParameterScope::Instance => {
                        if parameter.principal {
                            principals += 1;
                        }
                        if parameter.principal && principals == 1 {
                            member.name.clone()
                        } else {
                            format!("{}_{}", member.name, parameter.keyword)
                        }
                    }
                };
                let kept = filters.is_empty()
                    || filters
                        .iter()
                        .any(|filter| scan(filter.as_bytes(), name.as_bytes()));
                entries.push(Entry {
                    member: slot,
                    scope,
                    keyword: parameter.keyword,
                    name: kept.then_some(name),
                });
            }
        }
    }
    entries
}

/// The difference `b - a` of two triplet matrices, entry by entry.
fn difference(a: &SparseMatrix, b: &SparseMatrix) -> BTreeMap<(usize, usize), Real> {
    let mut sum: BTreeMap<(usize, usize), (Real, Real)> = BTreeMap::new();
    for triplet in a.triplets() {
        sum.entry((triplet.row, triplet.col)).or_default().0 += triplet.value;
    }
    for triplet in b.triplets() {
        sum.entry((triplet.row, triplet.col)).or_default().1 += triplet.value;
    }
    sum.into_iter()
        .map(|(position, (a, b))| (position, -a + b))
        .collect()
}

/// The device a sensitivity load stamps.
fn stand_in<'d>(circuit: &'d Circuit, index: usize, load: &'d SensitivityLoad) -> &'d dyn Device {
    match load {
        SensitivityLoad::Original => &*circuit.devices()[index],
        SensitivityLoad::Replacement(device) => &**device,
    }
}

/// The DC state of the analysis: the operating-point factors and solution.
struct DcState<'a> {
    factors: crate::maths::linear::SparseLu,
    solution: Vector,
    model: crate::devices::ModelContext,
    history: crate::devices::StateHistory,
    trial: TrialState,
    circuit: &'a Circuit,
}

impl DcState<'_> {
    /// `(dI, dY)` contributions of one load: the device's rhs and matrix.
    fn load(&self, index: usize, load: &SensitivityLoad) -> SpiceResult<(SparseMatrix, Vector)> {
        let n = self.circuit.unknown_count();
        let mut matrix = SparseMatrix::new(n, n);
        let mut rhs = Vector::zeros(n);
        let mut trial = self.trial.clone();
        self.circuit.load_device(
            index,
            stand_in(self.circuit, index, load),
            &LoadRequest {
                mode: AnalysisMode::OperatingPoint,
                solution: &self.solution,
                model_context: &self.model,
                integration: None,
                history: &self.history,
                forcing: None,
            },
            &mut matrix,
            &mut rhs,
            &mut trial,
        )?;
        Ok((matrix, rhs))
    }

    /// `Y^-1 (dI - dY x)` of a base and a perturbed load.
    fn delta(
        &self,
        index: usize,
        base: &SensitivityLoad,
        perturbed: &SensitivityLoad,
    ) -> SpiceResult<Vec<Real>> {
        let (y0, i0) = self.load(index, base)?;
        let (y1, i1) = self.load(index, perturbed)?;
        let n = self.circuit.unknown_count();
        let mut rhs = vec![0.; n];
        for (row, value) in rhs.iter_mut().enumerate() {
            *value = i1.get(row).unwrap_or(0.) - i0.get(row).unwrap_or(0.);
        }
        let mut product = vec![0.; n];
        for ((row, col), value) in difference(&y0, &y1) {
            product[row] += value * self.solution.get(col).unwrap_or(0.);
        }
        for (value, product) in rhs.iter_mut().zip(product) {
            *value -= product;
        }
        // A nonfinite load difference (C's division by a zero knee current
        // in `dioload.c`) propagates through C's solve as NaN.
        if rhs.iter().any(|value| !value.is_finite()) {
            return Ok(vec![Real::NAN; n]);
        }
        let solved = self.factors.solve(&Vector::from_slice(&rhs))?;
        Ok(solved.as_slice().to_vec())
    }
}

/// The AC state at one frequency.
struct AcState<'a> {
    omega: Real,
    factors: crate::maths::complex::ComplexLu,
    solution: Vec<Complex>,
    model: crate::devices::ModelContext,
    bias: Vector,
    state: Vec<Real>,
    circuit: &'a Circuit,
}

impl AcState<'_> {
    fn assemble(&self, index: usize, load: &SensitivityLoad) -> SpiceResult<LinearSystem> {
        let mut system = LinearSystem::new(self.circuit.unknown_count());
        self.circuit.assemble_small_signal_device(
            index,
            stand_in(self.circuit, index, load),
            &self.model,
            &self.bias,
            Some(&self.state),
            &mut system,
        )?;
        Ok(system)
    }

    fn delta(
        &self,
        index: usize,
        base: &SensitivityLoad,
        perturbed: &SensitivityLoad,
    ) -> SpiceResult<Vec<Complex>> {
        let s0 = self.assemble(index, base)?;
        let s1 = self.assemble(index, perturbed)?;
        let (r0, r1) = (s0.ac_rhs(), s1.ac_rhs());
        let mut rhs: Vec<Complex> = r1.iter().zip(&r0).map(|(a, b)| *a - *b).collect();
        let x = &self.solution;
        for ((row, col), value) in difference(&s0.a, &s1.a) {
            rhs[row] = rhs[row] - Complex::real(value) * x[col];
        }
        for ((row, col), value) in difference(&s0.e, &s1.e) {
            rhs[row] = rhs[row] - Complex::new(0., self.omega * value) * x[col];
        }
        self.factors.solve(&rhs)
    }
}

/// Runs `.sens` on `circuit`; see the module documentation.
///
/// # Errors
/// Malformed cards, unknown outputs, devices whose sensitivity is not ported,
/// operating-point failures and singular or nonfinite solves.
pub(crate) fn run(
    circuit: &mut Circuit,
    request: &AnalysisRequest,
    context: &AnalysisContext,
) -> SpiceResult<Plot> {
    let settings = crate::analysis::bias::DcSettings::from_request(request)?;
    let card = parse(&request.arguments)?;
    let grid = card.sweep.as_ref().map(frequencies).transpose()?;
    let ac = grid.is_some();
    circuit.finalize()?;
    let circuit: &Circuit = circuit;
    let probe = resolve_output(circuit, &card.output)?;
    let model = context.model_context();

    let (members, mut records) = members(circuit, &model)?;
    if ac {
        if let Some(device) = circuit
            .devices()
            .iter()
            .find(|device| device.is_nonlinear() || device.designator() == 'k')
        {
            return Err(unsupported(format!(
                ".sens ac with {} (designator '{}'): C's AC sensitivity re-runs CKTsetup and a \
                 DC load before every frequency, which resets device state, so nonlinear and \
                 switch devices are linearized at a reset state rather than at the operating \
                 point (and the K coupling acts through the inductors' loads); the port refuses \
                 instead of reproducing that (docs/port/SENSITIVITY.md)",
                device.name(),
                device.designator()
            )));
        }
    }
    let entries = entries(&members, ac, &card.filters);
    if entries.iter().all(|entry| entry.name.is_none()) {
        return Err(unsupported(
            ".sens perturbs no parameter (every name is filtered out, or the circuit has no \
             perturbable parameter); C writes no plot in that case",
        ));
    }

    // The operating point, exactly as `.op` solves it (`CKTop`).
    let hints = crate::analysis::initial::resolve(circuit, request)?;
    let nodes = crate::analysis::bias::NodeForcing {
        initial: Vec::new(),
        nodesets: crate::analysis::initial::forced_nodesets(circuit, &hints.nodesets, &[]),
    };
    let n = circuit.unknown_count();
    let mut seed = Vector::zeros(n);
    for hint in hints.nodesets {
        seed.as_mut_slice()[hint.row] = hint.value;
    }
    let solved = crate::analysis::bias::solve_dc_forced(
        circuit,
        &model,
        &settings,
        &[],
        Some(&seed),
        None,
        &nodes,
    )?
    .solution;

    let mut plot = Plot::new(
        "sens1",
        PLOTNAME,
        if ac {
            PlotFlags::Complex
        } else {
            PlotFlags::Real
        },
    );
    if ac {
        plot.push_variable(Variable::complex("frequency", "frequency"));
    }
    for entry in &entries {
        if let Some(name) = &entry.name {
            let name = format!("v({name})");
            plot.push_variable(if ac {
                Variable::complex(name, "voltage")
            } else {
                Variable::new(name, "voltage")
            });
        }
    }

    if let Some(grid) = grid {
        let history = circuit.state_history();
        let state = history
            .accepted(1)
            .map_or_else(|| vec![0.; history.len()], <[Real]>::to_vec);
        for frequency in grid {
            let omega = 2. * std::f64::consts::PI * frequency;
            // CKTsetup, CKTtemp and the AC load of the whole circuit in its
            // present (perturbed and restored) state.
            let mut system = LinearSystem::new(n);
            for (slot, member) in members.iter().enumerate() {
                let (model_record, instance_record) = records.pair(member, slot);
                member.sensitivity.setup(model_record, instance_record)?;
                member
                    .sensitivity
                    .temperature(model_record, instance_record)?;
                let pair = RecordPair {
                    model: model_record,
                    instance: instance_record,
                };
                let load = member.sensitivity.load(pair, pair, SensitivityMode::Ac)?;
                circuit.assemble_small_signal_device(
                    member.index,
                    stand_in(circuit, member.index, &load),
                    &model,
                    &solved.values,
                    Some(&state),
                    &mut system,
                )?;
            }
            system.a.fold_duplicates();
            system.e.fold_duplicates();
            let factors =
                ComplexMatrix::from_operators(&system.a, &system.e, omega)?.factorize()?;
            let solution = factors.solve(&system.ac_rhs())?;
            let at = AcState {
                omega,
                factors,
                solution,
                model,
                bias: solved.values.clone(),
                state: state.clone(),
                circuit,
            };
            let mut point = vec![Complex::real(frequency)];
            sweep_parameters(
                &members,
                &mut records,
                &entries,
                SensitivityMode::Ac,
                |index, base, perturbed, delta| {
                    let dx = at.delta(index, base, perturbed)?;
                    let value = probe.read(&dx, Complex::ZERO);
                    point.push(Complex::new(value.re / delta, value.im / delta));
                    Ok(())
                },
            )?;
            plot.push_point(point)?;
        }
    } else {
        // The Newton Jacobian at the operating point (C's matrix), as `.tf`.
        let history = circuit.state_history();
        let trial = history
            .trial_in(IterationPhase::Float, Some(&solved.trial))?
            .with_device_limiting(settings.newton.limiting.is_device())
            .with_c_jacobian(true);
        let mut jacobian = SparseMatrix::new(n, n);
        circuit.load(
            &LoadRequest {
                mode: AnalysisMode::OperatingPoint,
                solution: &solved.values,
                model_context: &model,
                integration: None,
                history: &history,
                forcing: None,
            },
            &mut jacobian,
            &mut Vector::zeros(n),
            &mut trial.clone(),
        )?;
        jacobian.fold_duplicates();
        let at = DcState {
            factors: jacobian.factorize()?,
            solution: solved.values.clone(),
            model,
            history,
            trial,
            circuit,
        };
        let mut point = Vec::new();
        sweep_parameters(
            &members,
            &mut records,
            &entries,
            SensitivityMode::Dc,
            |index, base, perturbed, delta| {
                let dx = at.delta(index, base, perturbed)?;
                point.push(Complex::real(probe.read(&dx, 0.) / delta));
                Ok(())
            },
        )?;
        plot.push_point(point)?;
    }
    Ok(plot)
}

/// The per-parameter loop of `sens_sens()`: setup, base load, perturbation,
/// perturbed load and restoration, on the live records; `measure` turns a
/// pair of loads into the output column.
fn sweep_parameters(
    members: &[Member<'_>],
    records: &mut Records,
    entries: &[Entry],
    mode: SensitivityMode,
    mut measure: impl FnMut(usize, &SensitivityLoad, &SensitivityLoad, Real) -> SpiceResult<()>,
) -> SpiceResult<()> {
    for entry in entries {
        if entry.name.is_none() {
            continue;
        }
        let member = &members[entry.member];
        let device = &member.sensitivity;
        let (model, instance) = records.pair(member, entry.member);
        // `sgen_next()` asks the value before this parameter's DEVsetup.
        let value = device.ask(entry.scope, entry.keyword, RecordPair { model, instance })?;
        device.setup(model, instance)?;
        device.temperature(model, instance)?;
        let snapshot = (model.clone(), instance.clone());
        let setup = RecordPair {
            model: &snapshot.0,
            instance: &snapshot.1,
        };
        let base = device.load(setup, setup, mode)?;
        let delta = if value == 0. {
            ABSOLUTE_DELTA
        } else {
            value * RELATIVE_DELTA
        };
        device.set(entry.scope, entry.keyword, value + delta, model, instance)?;
        device.temperature(model, instance)?;
        let perturbed = device.load(
            setup,
            RecordPair {
                model: &*model,
                instance: &*instance,
            },
            mode,
        )?;
        measure(member.index, &base, &perturbed, delta)?;
        device.set(entry.scope, entry.keyword, value, model, instance)?;
        device.temperature(model, instance)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Output, Stepping, frequencies, parse, scan};

    fn tokens(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn cards_parse_outputs_filters_and_sweeps() {
        let card = parse(&tokens("V ( Out ) R* d1:is ac DEC 10 1 1k")).unwrap();
        assert_eq!(
            card.output,
            Output::Voltage {
                pos: "out".into(),
                neg: None
            }
        );
        assert_eq!(card.filters, ["r*", "d1:is"]);
        let sweep = card.sweep.unwrap();
        assert_eq!(sweep.stepping, Stepping::Decade);
        assert_eq!(sweep.steps, 10);
        let card = parse(&tokens("v ( a , b ) dc")).unwrap();
        assert!(card.sweep.is_none() && card.filters.is_empty());
        let card = parse(&tokens("i ( vm )")).unwrap();
        assert_eq!(
            card.output,
            Output::Current {
                source: "vm".into()
            }
        );
        for (text, message) in [
            ("", "no output variable"),
            ("x ( a )", "neither a voltage"),
            ("v ( a", "is missing"),
            ("v ( a ) ac foo 1 1 2", "dec, oct or lin"),
            ("v ( a ) ac dec 1 1", "stop frequency"),
            ("v ( a ) dc extra", "unexpected argument"),
            ("v ( a ) ( b", "unexpected '('"),
        ] {
            let error = parse(&tokens(text)).expect_err(text);
            assert!(error.to_string().contains(message), "{text}: {error}");
        }
    }

    #[test]
    fn filters_match_like_cktsens_scan() {
        assert!(scan(b"r*", b"r1_m"));
        assert!(scan(b"*", b"anything"));
        assert!(scan(b"r?", b"r1"));
        assert!(!scan(b"r?", b"r12"));
        assert!(scan(b"d1:*s", b"d1:is"));
        assert!(!scan(b"v1", b"v1_z0"));
        assert!(scan(b"v1", b"v1"));
        assert!(scan(b"*_m", b"r2_m"));
        assert!(!scan(b"*_m", b"r2_ms"));
    }

    #[test]
    fn sweeps_step_like_count_steps_and_inc_freq() {
        let sweep = |text: &str| {
            parse(&tokens(&format!("v ( a ) ac {text}")))
                .unwrap()
                .sweep
                .unwrap()
        };
        let f = frequencies(&sweep("dec 1 1k 100k")).unwrap();
        assert_eq!(f.len(), 3);
        assert_eq!(f[0], 1e3);
        assert!((f[2] / 1e5 - 1.).abs() < 1e-12);
        // `lin` multiplies by (fstop - fstart)/pts (C's inc_freq defect).
        assert_eq!(frequencies(&sweep("lin 2 1k 2k")).unwrap(), [1e3, 5e5]);
        // `oct` divides by M_LOG2E: 1 per octave over 1..16 gives 2 points.
        assert_eq!(frequencies(&sweep("oct 1 1 16")).unwrap().len(), 2);
        assert!(frequencies(&sweep("lin 2 1k 1k")).is_err());
        assert!(frequencies(&sweep("dec 1 0 1k")).is_err());
    }
}
