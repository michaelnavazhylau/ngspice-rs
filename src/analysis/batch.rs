//! Batch scheduling of every analysis card in a deck, as `ngspice -b -r` does.
//!
//! A deck may request several analyses; ngspice's batch mode runs **all** of
//! them in one job and writes one plot per analysis into a single rawfile. This
//! module owns the parts of that behaviour that are not a driver's job:
//!
//! * **order** ([`schedule`]): C's `CKTdoJob()` (`src/spicelib/analysis/
//!   cktdojob.c`) walks the analysis table `analInfo[]`
//!   (`src/spicelib/analysis/analysis.c`) in its fixed order — options, `.ac`,
//!   `.dc`, `.op`, `.tran`, `.pz`, `.tf`, `.disto`, `.noise`, `.sens`, `.sp` — and for
//!   each type runs every job of that type. Jobs are *prepended* to the task's
//!   job list by `CKTnewAnal()` (`cktnewan.c`), so two cards of the same type run
//!   in **reverse deck order**. Deck order between different types is
//!   irrelevant: `.tran` before `.op` in the deck still runs `.op` first;
//! * **plot names** ([`ScheduledAnalysis::plot_name`]): `plot_add()`/
//!   `plot_alloc()` (`src/frontend/vectors.c`) name an in-memory plot with the
//!   `ft_plotabbrev()` abbreviation (`op`, `dc`, `ac`, `tran`, …) followed by the
//!   global `plot_num`, which starts at 1 and is incremented — and stays
//!   incremented — whenever the candidate name is already taken. A deck with
//!   `.ac`, two `.dc`, `.op` and `.tran` therefore produces `ac1 dc1 dc2 op2
//!   tran2`. The rawfile itself carries only the `Plotname:` header (`AC
//!   Analysis`, …), which each driver already writes;
//! * **which output card applies to which plot** ([`check_targets`],
//!   [`resolve_outputs`]): `.save` applies to every plot; `.print <type>`
//!   narrows and prints only the plots of that type (C's `ft_cktcoms()` prints
//!   one table per matching plot); `.measure <type>` and `.four` are evaluated
//!   against the **last executed** plot of their type, which is the plot C's
//!   `plot_cur`/`setcplot("tran")` select after the run.
//!
//! Divergences from C, all documented in `docs/port/CLI.md`:
//!
//! * C evaluates only the `.measure` cards whose type matches the **last**
//!   analysis that ran (`dosim()` calls `do_measure(ci_last_an)`) and skips the
//!   others silently; this port evaluates every card against the last plot of
//!   its own type instead of dropping it;
//! * a `.print`, `.measure` or `.four` card naming an analysis type the deck
//!   does not run is an error here (C prints `Error: .print: no ac analysis
//!   found.` and carries on, or skips the measurement);
//! * C keeps running the remaining analyses after one fails and writes the
//!   plots that succeeded; the port's callers publish nothing when any analysis
//!   fails.

use crate::netlist::ast::{AnalysisCard, FourierCard, MeasureCard, OutputCards};
use crate::primitives::{AnalysisKind, SpiceError, SpiceResult};

use crate::analysis::fourier::{self, FourierAnalysis};
use crate::analysis::measure::{self, Measurement};
use crate::analysis::results::Plot;
use crate::analysis::selection::{self, Selection};

/// The C reference for the batch order and plot naming.
pub const C_REFERENCE: &str = "src/spicelib/analysis/cktdojob.c (CKTdoJob), \
                               src/spicelib/analysis/analysis.c (analInfo), \
                               src/frontend/vectors.c (plot_add)";

/// One analysis card in the order a batch run executes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledAnalysis {
    /// The card's index in the deck's analysis list (`Netlist::analyses`).
    pub card_index: usize,
    /// The analysis type.
    pub kind: AnalysisKind,
    /// The in-memory plot name C gives the result, e.g. `tran1`.
    pub plot_name: String,
    /// True for the last executed plot of this type: the plot `.measure` cards
    /// of this type (and `.four`, for `.tran`) are evaluated against.
    pub last_of_kind: bool,
}

/// The position of `kind` in C's `analInfo[]` table, which is the order
/// `CKTdoJob()` runs analysis types in. Index 0 is the options pseudo-analysis.
///
/// `.four` is post-processing of a transient plot, not a job, so it sorts after
/// every real analysis; a driver lookup rejects it before anything runs.
#[must_use]
pub const fn job_order(kind: AnalysisKind) -> usize {
    match kind {
        AnalysisKind::Ac => 1,
        AnalysisKind::DcSweep => 2,
        AnalysisKind::OperatingPoint => 3,
        AnalysisKind::Transient => 4,
        AnalysisKind::PoleZero => 5,
        AnalysisKind::TransferFunction => 6,
        AnalysisKind::Distortion => 7,
        AnalysisKind::Noise => 8,
        AnalysisKind::Sensitivity => 9,
        // RFSPICE appends SPinfo after SENSinfo (and the PSS/SENSE2 entries,
        // which have no card here).
        AnalysisKind::SParameter => 10,
        AnalysisKind::Fourier => 11,
    }
}

/// The plot-type abbreviation `ft_plotabbrev()` (`src/frontend/typesdef.c`)
/// derives from the plot's name for each analysis type.
#[must_use]
pub const fn plot_abbreviation(kind: AnalysisKind) -> &'static str {
    match kind {
        AnalysisKind::OperatingPoint => "op",
        AnalysisKind::DcSweep => "dc",
        AnalysisKind::Ac => "ac",
        AnalysisKind::Transient => "tran",
        AnalysisKind::Noise => "noise",
        AnalysisKind::Distortion => "disto",
        AnalysisKind::PoleZero => "pz",
        AnalysisKind::Sensitivity => "sens",
        AnalysisKind::TransferFunction => "tf",
        AnalysisKind::Fourier => "four",
        // typesdef.c maps the "SP Analysis" plot name to "sp".
        AnalysisKind::SParameter => "sp",
    }
}

/// The deck's analysis cards in batch execution order, with C's plot names.
///
/// See the module documentation for the rules. An empty list schedules
/// nothing; whether that is an error is the caller's decision.
#[must_use]
pub fn schedule(cards: &[AnalysisCard]) -> Vec<ScheduledAnalysis> {
    let mut order: Vec<usize> = (0..cards.len()).collect();
    // Same type: reverse deck order (the job list is built by prepending).
    order.sort_by(|&a, &b| {
        job_order(cards[a].kind)
            .cmp(&job_order(cards[b].kind))
            .then(b.cmp(&a))
    });
    let mut names: Vec<String> = Vec::with_capacity(order.len());
    let mut plot_num = 1_usize;
    let mut scheduled: Vec<ScheduledAnalysis> = order
        .iter()
        .map(|&card_index| {
            let kind = cards[card_index].kind;
            let abbreviation = plot_abbreviation(kind);
            let mut name = format!("{abbreviation}{plot_num}");
            while names.iter().any(|taken| taken.eq_ignore_ascii_case(&name)) {
                plot_num += 1;
                name = format!("{abbreviation}{plot_num}");
            }
            names.push(name.clone());
            ScheduledAnalysis {
                card_index,
                kind,
                plot_name: name,
                last_of_kind: false,
            }
        })
        .collect();
    for index in 0..scheduled.len() {
        let kind = scheduled[index].kind;
        scheduled[index].last_of_kind = !scheduled[index + 1..]
            .iter()
            .any(|later| later.kind == kind);
    }
    scheduled
}

/// Checks, before anything runs, that every `.print`, `.measure` and `.four`
/// card targets an analysis type the schedule actually runs.
///
/// # Errors
///
/// [`SpiceError::Unsupported`], positioned at the card, for a `.print` or
/// `.measure` card whose analysis type is not scheduled, and for a `.four` card
/// in a deck that runs no `.tran`: such a card can never be honoured, and the
/// port never drops it silently.
pub fn check_targets(
    schedule: &[ScheduledAnalysis],
    output: &OutputCards,
    measures: &[MeasureCard],
    fourier: &[FourierCard],
) -> SpiceResult<()> {
    let runs = |kind: AnalysisKind| schedule.iter().any(|entry| entry.kind == kind);
    let scheduled = || {
        let kinds: Vec<String> = schedule
            .iter()
            .map(|entry| format!(".{}", entry.kind.as_str()))
            .collect();
        if kinds.is_empty() {
            "no analysis".to_owned()
        } else {
            kinds.join(" ")
        }
    };
    for print in &output.prints {
        if !runs(print.analysis) {
            return Err(SpiceError::Unsupported {
                feature: format!(
                    ".print {} names a different analysis than the deck runs ({}), so the \
                     request can never be honoured",
                    print.analysis.as_str(),
                    scheduled()
                ),
                location: Some(print.analysis_location.clone()),
            });
        }
    }
    for card in measures {
        if !runs(card.analysis) {
            return Err(SpiceError::Unsupported {
                feature: format!(
                    ".measure {} {}: the card names a .{} measurement; the deck runs {}, so \
                     the card can never be honoured",
                    card.analysis.as_str(),
                    card.name,
                    card.analysis.as_str(),
                    scheduled()
                ),
                location: Some(card.location.clone()),
            });
        }
    }
    if let Some(card) = fourier.first()
        && !runs(AnalysisKind::Transient)
    {
        return Err(SpiceError::Unsupported {
            feature: format!(
                ".four: the card transforms a .tran result (C selects the tran plot); the deck \
                 runs {}, so the card can never be honoured",
                scheduled()
            ),
            location: Some(card.location.clone()),
        });
    }
    Ok(())
}

/// What one scheduled plot publishes, resolved against its **full** plot.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotOutputs {
    /// The plot after `.save`/`.print` selection: what the rawfile carries.
    pub written: Plot,
    /// The `.print` table for this plot, when a `.print` card names its type.
    pub printed: Option<String>,
    /// The `.measure` results evaluated against this plot, in card order.
    pub measurements: Vec<Measurement>,
    /// The `.four` results evaluated against this plot, in card order.
    pub fourier: Vec<FourierAnalysis>,
}

/// Resolves the output cards that apply to one scheduled plot.
///
/// * the written selection is every `.save` request plus the `.print` requests
///   naming this plot's type (an empty set keeps the whole plot), through
///   [`Selection::resolve`];
/// * the `.print` table, through [`Selection::to_text`], when any `.print` card
///   names this type (every plot of that type gets its own table);
/// * the `.measure` cards of this type and, for `.tran`, the `.four` cards,
///   only when this is the last executed plot of its type.
///
/// Everything is resolved against the **full** `plot`, so a measured or
/// transformed operand the selection dropped stays available.
///
/// # Errors
///
/// Whatever [`Selection::resolve`], [`measure::resolve`] or
/// [`fourier::resolve`] report for this plot.
pub fn resolve_outputs(
    plot: &Plot,
    scheduled: &ScheduledAnalysis,
    output: &OutputCards,
    measures: &[MeasureCard],
    fourier_cards: &[FourierCard],
) -> SpiceResult<PlotOutputs> {
    let kind = scheduled.kind;
    let own = OutputCards {
        saves: output.saves.clone(),
        prints: output
            .prints
            .iter()
            .filter(|print| print.analysis == kind)
            .cloned()
            .collect(),
    };
    let requests = selection::write_requests(&own, kind)?;
    let selection = Selection::resolve(plot, kind, &requests)?;
    let print_requests = selection::print_requests(&own, kind)?;
    let printed = if print_requests.is_empty() {
        None
    } else {
        Some(Selection::resolve(plot, kind, &print_requests)?.to_text(plot)?)
    };
    let measurements = if scheduled.last_of_kind {
        let own: Vec<MeasureCard> = measures
            .iter()
            .filter(|card| card.analysis == kind)
            .cloned()
            .collect();
        measure::resolve(plot, kind, &own)?
    } else {
        Vec::new()
    };
    let fourier = if scheduled.last_of_kind && kind == AnalysisKind::Transient {
        fourier::resolve(plot, kind, fourier_cards)?
    } else {
        Vec::new()
    };
    let written = selection.apply(plot)?;
    Ok(PlotOutputs {
        written,
        printed,
        measurements,
        fourier,
    })
}

#[cfg(test)]
mod tests {
    use super::{job_order, schedule};
    use crate::netlist::ast::AnalysisCard;
    use crate::primitives::{AnalysisKind, SourceLoc};

    fn card(kind: AnalysisKind, line: u32) -> AnalysisCard {
        AnalysisCard {
            kind,
            arguments: Vec::new(),
            expressions: Vec::new(),
            uic: false,
            uic_location: None,
            location: SourceLoc::new(std::path::PathBuf::from("deck.cir"), line, 1),
        }
    }

    fn names(cards: &[AnalysisCard]) -> Vec<(usize, String, bool)> {
        schedule(cards)
            .into_iter()
            .map(|entry| (entry.card_index, entry.plot_name, entry.last_of_kind))
            .collect()
    }

    #[test]
    fn types_run_in_analinfo_order_whatever_the_deck_order() {
        use AnalysisKind::{Ac, DcSweep, OperatingPoint, Transient};
        let cards = [
            card(Transient, 2),
            card(OperatingPoint, 3),
            card(Ac, 4),
            card(DcSweep, 5),
        ];
        assert_eq!(
            names(&cards),
            [
                (2, "ac1".to_owned(), true),
                (3, "dc1".to_owned(), true),
                (1, "op1".to_owned(), true),
                (0, "tran1".to_owned(), true),
            ]
        );
    }

    #[test]
    fn same_type_cards_run_in_reverse_deck_order_and_bump_the_plot_number() {
        // Measured with ngspice-47: `.tran`, `.ac`, `.dc a`, `.op`, `.dc b`
        // lists `ac1 dc1 dc2 op2 tran2`, and `dc1` is the `.dc b` sweep.
        use AnalysisKind::{Ac, DcSweep, OperatingPoint, Transient};
        let cards = [
            card(Transient, 2),
            card(Ac, 3),
            card(DcSweep, 4),
            card(OperatingPoint, 5),
            card(DcSweep, 6),
        ];
        assert_eq!(
            names(&cards),
            [
                (1, "ac1".to_owned(), true),
                (4, "dc1".to_owned(), false),
                (2, "dc2".to_owned(), true),
                (3, "op2".to_owned(), true),
                (0, "tran2".to_owned(), true),
            ]
        );
    }

    #[test]
    fn a_single_card_and_an_empty_deck() {
        assert_eq!(
            names(&[card(AnalysisKind::OperatingPoint, 2)]),
            [(0, "op1".to_owned(), true)]
        );
        assert!(schedule(&[]).is_empty());
    }

    #[test]
    fn the_job_order_is_c_analinfo() {
        let mut kinds = AnalysisKind::ALL.to_vec();
        kinds.sort_by_key(|kind| job_order(*kind));
        assert_eq!(
            kinds,
            [
                AnalysisKind::Ac,
                AnalysisKind::DcSweep,
                AnalysisKind::OperatingPoint,
                AnalysisKind::Transient,
                AnalysisKind::PoleZero,
                AnalysisKind::TransferFunction,
                AnalysisKind::Distortion,
                AnalysisKind::Noise,
                AnalysisKind::Sensitivity,
                AnalysisKind::SParameter,
                AnalysisKind::Fourier,
            ]
        );
    }
}
