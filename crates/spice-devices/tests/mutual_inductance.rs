//! K mutual inductance through the device layer (GitHub #80): registry entry,
//! elaboration and reference resolution, the inductive-system check, and the
//! coupled-flux DC/companion/linear stamps of `Circuit::load` and
//! `Circuit::linear_system` (C `indload.c`, `mutacld.c`, `muttemp.c`).
use std::path::Path;

use spice_core::{SpiceError, SpiceResult};
use spice_devices::{AnalysisMode, Circuit, LoadRequest, ModelContext, MutualInductance, Registry};
use spice_maths::{IntegrationMethod, SparseMatrix, StepHistory, Vector, integrator::DEFAULT_XMU};
use spice_netlist::{Parser, source::parse_deck_text};

fn circuit(body: &str) -> SpiceResult<Circuit> {
    let netlist = Parser::new().parse_deck(&parse_deck_text(
        Path::new("k.cir"),
        &format!("mutual\n{body}\n.end\n"),
    ))?;
    Circuit::from_netlist(&netlist)
}

fn error(body: &str) -> SpiceError {
    circuit(body).expect_err("expected a failure")
}

fn close(got: f64, want: f64) {
    assert!(
        (got - want).abs() <= 1e-12 * want.abs().max(1e-12),
        "{got} != {want}"
    );
}

/// `l1 a 0 1m`, `l2 b 0 4m` with resistors, coupled by `k`: M = k 2m.
const PAIR: &str = "r1 a 0 1\nr2 b 0 1\nl1 a 0 1m\nl2 b 0 4m";

#[test]
fn the_registry_ports_k_with_the_ind_references() {
    let registry = Registry::with_builtins();
    let entry = registry.get('K').unwrap();
    assert!(entry.ported);
    assert!(entry.c_reference.contains("inp2k.c"), "{entry:?}");
    assert!(entry.c_reference.contains("ind/mutsetup.c"), "{entry:?}");
    assert!(!entry.c_reference.contains("cpl"), "{entry:?}");
}

#[test]
fn k_devices_have_no_terminals_branches_or_state() {
    let c = circuit(&format!("{PAIR}\nk1 l1 l2 0.5")).unwrap();
    let k = c.device("k1").unwrap();
    assert_eq!(k.designator(), 'k');
    assert!(k.terminals().is_empty());
    assert_eq!(k.branch_currents(), 0);
    assert_eq!(k.state_count(), 0);
    let coupling = k.mutual_coupling().unwrap();
    assert_eq!(coupling.coefficient, 0.5);
    let names: Vec<_> = coupling.inductors.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["l1", "l2"]);
    assert!(coupling.location.is_some());
    // M = k sqrt(L1 L2) = 0.5 * 2m on both branch rows.
    let terms = c.mutual_terms(&ModelContext::default()).unwrap();
    let rows = |name: &str| {
        let index = c.devices().iter().position(|d| d.name() == name).unwrap();
        (index, c.branch_rows(index).unwrap().start)
    };
    let ((i1, r1), (i2, r2)) = (rows("l1"), rows("l2"));
    assert_eq!(terms[i1].len(), 1);
    assert_eq!(terms[i1][0].row, r2);
    close(terms[i1][0].inductance, 1e-3);
    assert_eq!(terms[i2][0].row, r1);
    close(terms[i2][0].inductance, 1e-3);
}

#[test]
fn constructor_validation() {
    let reference = |name: &str| spice_devices::ControlReference {
        name: name.to_owned(),
        location: None,
    };
    assert!(MutualInductance::new("k1", vec![reference("l1")], 0.5).is_err());
    assert!(MutualInductance::new("k1", vec![reference("l1"), reference("l2")], f64::NAN).is_err());
    let itself = MutualInductance::new("k1", vec![reference("l1"), reference("L1")], 0.5);
    assert!(matches!(itself, Err(SpiceError::Unsupported { .. })));
    let k = MutualInductance::new("k1", vec![reference("l1"), reference("l2")], -1.0).unwrap();
    assert_eq!(k.coefficient(), -1.0);
}

#[test]
fn references_must_name_existing_inductors() {
    let missing = error(&format!("{PAIR}\nk1 l1 l3 0.5"));
    assert!(matches!(missing, SpiceError::Parse { .. }), "{missing}");
    assert!(
        missing
            .to_string()
            .contains("k1: coupling to non-existent inductor l3"),
        "{missing}"
    );
    let resistor = error(&format!("{PAIR}\nk1 l1 r2 0.5")).to_string();
    assert!(resistor.contains("r2 is not an inductor"), "{resistor}");
    let itself = error(&format!("{PAIR}\nk1 l1 L1 0.5")).to_string();
    assert!(itself.contains("to itself"), "{itself}");
    // A K card after its inductors or before them resolves the same way.
    circuit("k1 l1 l2 0.5\nr1 a 0 1\nr2 b 0 1\nl1 a 0 1m\nl2 b 0 4m").unwrap();
}

#[test]
fn inductive_systems_must_be_positive_semidefinite() {
    let message = |body: &str| error(&format!("{PAIR}\nl3 c 0 2m\nr3 c 0 1\n{body}")).to_string();
    // |k| > 1 stores negative energy (C warns "is not positive definite").
    let big = message("k1 l1 l2 1.01");
    assert!(big.contains("not positive semidefinite"), "{big}");
    assert!(big.contains("l1 l2") && big.contains("k1"), "{big}");
    let negative = message("k1 l1 l2 -1.5");
    assert!(negative.contains("not positive semidefinite"), "{negative}");
    // Each |k| < 1 but the set is inconsistent: k12 = k13 = 0.9, k23 = -0.9.
    let set = message("k1 l1 l2 0.9\nk2 l1 l3 0.9\nk3 l2 l3 -0.9");
    assert!(set.contains("system l1 l2 l3 coupled"), "{set}");
    assert!(set.contains("k1 k2 k3"), "{set}");
    // Duplicate couplings of one pair are summed (C sums its loads).
    let summed = message("k1 l1 l2 0.6\nk2 l2 l1 0.6");
    assert!(summed.contains("not positive semidefinite"), "{summed}");
    // Ideal (|k| = 1) and partial couplings are accepted.
    for body in [
        "k1 l1 l2 1",
        "k1 l1 l2 -1",
        "k1 l1 l2 l3 1",
        "k1 l1 l2 0.5\nk2 l2 l3 0.5",
        "k1 l1 l2 0.5\nk2 l1 l2 0.4",
    ] {
        circuit(&format!("{PAIR}\nl3 c 0 2m\nr3 c 0 1\n{body}"))
            .unwrap_or_else(|e| panic!("{body}: {e}"));
    }
}

#[test]
fn multi_inductor_cards_couple_every_pair() {
    let c = circuit(&format!("{PAIR}\nl3 c 0 2m\nr3 c 0 1\nk1 l1 l2 l3 0.5")).unwrap();
    let terms = c.mutual_terms(&ModelContext::default()).unwrap();
    let index = |name: &str| c.devices().iter().position(|d| d.name() == name).unwrap();
    assert_eq!(terms[index("l1")].len(), 2);
    assert_eq!(terms[index("l2")].len(), 2);
    assert_eq!(terms[index("l3")].len(), 2);
    assert!(terms[index("k1")].is_empty());
}

#[test]
fn subcircuit_k_cards_use_hierarchical_names() {
    let body = ".subckt xf p s\nl1 p 0 1m\nl2 s 0 4m\nk1 l1 l2 0.9\n.ends\n\
                r1 a 0 1\nr2 b 0 1\nr3 c 0 1\nr4 d 0 1\nx1 a b xf\nx2 c d xf";
    let c = circuit(body).unwrap();
    for k in ["k.x1.k1", "k.x2.k1"] {
        let coupling = c.device(k).unwrap().mutual_coupling().unwrap();
        let prefix = &k[2..4];
        for reference in coupling.inductors {
            assert!(
                reference.name.starts_with(&format!("l.{prefix}.")),
                "{k}: {reference:?}"
            );
        }
    }
    // A body K card cannot reach a top-level inductor: its name is renamed too.
    let outer = error(
        ".subckt xf p\nla p 0 1m\nk1 la lt 0.5\n.ends\nr1 a 0 1\nlt t 0 1m\nrt t 0 1\nx1 a xf",
    );
    assert!(
        outer.to_string().contains("non-existent inductor l.x1.lt"),
        "{outer}"
    );
    // A top-level K card may name a flattened inductor explicitly.
    circuit(&format!(
        ".subckt xf p\nla p 0 1m\n.ends\nrx x 0 1\nx1 x xf\n{PAIR}\nk1 l1 l.x1.la 0.5"
    ))
    .unwrap();
}

#[test]
fn model_backed_inductors_couple_through_their_undivided_inductance() {
    // MUTtemp uses INDinduct (after TC and scale, before /m): M = k sqrt(2m 4m),
    // while l1 itself stamps 2m / 2.
    let c =
        circuit("r1 a 0 1\nr2 b 0 1\nl1 a 0 lm m=2\nl2 b 0 4m\nk1 l1 l2 0.5\n.model lm l(ind=2m)")
            .unwrap();
    let index = |name: &str| c.devices().iter().position(|d| d.name() == name).unwrap();
    let value = c.devices()[index("l1")]
        .inductance(&ModelContext::default())
        .unwrap()
        .unwrap();
    close(value.effective, 1e-3);
    close(value.coupling_base, 2e-3);
    let terms = c.mutual_terms(&ModelContext::default()).unwrap();
    close(
        terms[index("l1")][0].inductance,
        0.5 * (2e-3_f64 * 4e-3).sqrt(),
    );
}

fn load(
    circuit: &Circuit,
    history: &spice_devices::StateHistory,
    solution: &[f64],
    integration: Option<&spice_maths::Coefficients>,
) -> (SparseMatrix, Vector, spice_devices::TrialState) {
    let n = circuit.unknown_count();
    let mut matrix = SparseMatrix::new(n, n);
    let mut rhs = Vector::zeros(n);
    let mut trial = history.trial();
    let mode = integration.map_or(AnalysisMode::OperatingPoint, |c| AnalysisMode::Transient {
        time: 1.0,
        dt: c.dt(),
    });
    circuit
        .load(
            &LoadRequest {
                mode,
                solution: &Vector::from_slice(solution),
                model_context: &ModelContext::default(),
                integration,
                history,
                forcing: None,
            },
            &mut matrix,
            &mut rhs,
            &mut trial,
        )
        .unwrap();
    matrix.fold_duplicates();
    (matrix, rhs, trial)
}

#[test]
fn coupled_flux_drives_dc_state_companions_and_linear_assembly() {
    // Only the two inductors: nodes a, b; branch rows 2 (l1) and 3 (l2).
    let mut c = circuit("l1 a 0 1m\nl2 b 0 4m\nk1 l1 l2 0.5").unwrap();
    let m = 1e-3;
    let index =
        |c: &Circuit, name: &str| c.devices().iter().position(|d| d.name() == name).unwrap();
    let (b1, b2) = (
        c.branch_rows(index(&c, "l1")).unwrap().start,
        c.branch_rows(index(&c, "l2")).unwrap().start,
    );
    let (s1, s2) = (
        c.state_rows(index(&c, "l1")).unwrap().start,
        c.state_rows(index(&c, "l2")).unwrap().start,
    );
    let mut x = vec![0.0; c.unknown_count()];
    x[b1] = 2.0;
    x[b2] = -1.0;
    // DC: shorts, no mutual matrix entry; the recorded flux is coupled.
    let mut history = c.state_history();
    let (matrix, _, trial) = load(&c, &history, &x, None);
    assert_eq!(matrix.get(b1, b2), 0.0);
    close(trial.values()[s1], 1e-3 * 2.0 - m);
    close(trial.values()[s2], -4e-3 + m * 2.0);
    c.accept_point(&Vector::from_slice(&x), None, &mut history, trial)
        .unwrap();
    // Backward Euler from that point: the cross entries are -ag0 M and the
    // history source uses the coupled flux, so v = ag0 (flux - flux_prev).
    let steps = StepHistory::new();
    let be = steps
        .trial(IntegrationMethod::Trapezoidal, 1, 1e-6, DEFAULT_XMU)
        .unwrap();
    let ag0 = be.ag()[0];
    let mut y = x.clone();
    y[b1] = 3.0;
    let (matrix, rhs, trial) = load(&c, &history, &y, Some(&be));
    close(matrix.get(b1, b2), -ag0 * m);
    close(matrix.get(b2, b1), -ag0 * m);
    close(matrix.get(b1, b1), -ag0 * 1e-3);
    let flux1 = 1e-3 * 3.0 - m;
    close(trial.values()[s1], flux1);
    // Row b1: v(a) - ag0 L1 i1 - ag0 M i2 = derivative - ag0 flux1.
    close(trial.values()[s1 + 1], ag0 * (flux1 - (1e-3 * 2.0 - m)));
    close(rhs.as_slice()[b1], trial.values()[s1 + 1] - ag0 * flux1);
    // Truncation control sees the coupled flux slot.
    assert_eq!(c.devices()[index(&c, "l1")].truncation_slots(), [0]);
    // E x' + A x = b: -M between the branch rows, -L on the diagonal.
    let system = c.linear_system().unwrap();
    close(system.e.get(b1, b2), -m);
    close(system.e.get(b2, b1), -m);
    close(system.e.get(b1, b1), -1e-3);
    close(system.e.get(b2, b2), -4e-3);
}
