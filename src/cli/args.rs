//! Argument parsing and the text the CLI prints.
//!
//! Everything that produces text is a pure function of data, so the output can
//! be tested without capturing stdout.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use crate::analysis::{AnalysisSupport, DRIVERS};
use crate::devices::{DeviceSupport, Registry};
use crate::netlist::{Deck, RawCard, classify_deck, load};
use crate::primitives::{AnalysisKind as Kind, SpiceResult};

/// Process exit codes. See the crate documentation.
pub mod exit_code {
    /// Everything worked.
    pub const SUCCESS: i32 = 0;
    /// The command line was wrong.
    pub const USAGE: i32 = 1;
    /// The deck could not be read, tokenized or understood.
    pub const FAILURE: i32 = 2;
    /// The requested operation is a documented gap in the port.
    pub const NOT_YET_PORTED: i32 = 3;
}

/// What the CLI should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Print what the deck contains and what the port cannot do with it.
    Summary,
    /// List every card, with its line number and classification.
    Cards,
    /// Dump the token stream of every card.
    Tokens,
    /// Build a semantic netlist for supported syntax; report any unported gaps.
    Parse,
    /// Run every analysis of the deck and write one ASCII rawfile.
    Simulate,
    /// List the device designators the registry knows.
    Devices,
    /// List the analyses and whether a driver exists.
    Analyses,
    /// Print usage.
    Help,
    /// Print the version.
    Version,
}

impl Command {
    /// The name as typed on the command line.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Summary => "summary",
            Self::Cards => "cards",
            Self::Tokens => "tokens",
            Self::Parse => "parse",
            Self::Simulate => "simulate",
            Self::Devices => "devices",
            Self::Analyses => "analyses",
            Self::Help => "help",
            Self::Version => "version",
        }
    }

    /// Parses a subcommand name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        [
            Self::Summary,
            Self::Cards,
            Self::Tokens,
            Self::Parse,
            Self::Simulate,
            Self::Devices,
            Self::Analyses,
            Self::Help,
            Self::Version,
        ]
        .into_iter()
        .find(|command| command.name() == name)
    }

    /// Whether the command needs a netlist.
    #[must_use]
    pub const fn needs_netlist(self) -> bool {
        matches!(
            self,
            Self::Summary | Self::Cards | Self::Tokens | Self::Parse | Self::Simulate
        )
    }

    /// Whether the command writes a rawfile and therefore needs `--output`.
    #[must_use]
    const fn needs_output(self) -> bool {
        matches!(self, Self::Simulate)
    }
}

/// A parsed command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    /// What to do.
    pub command: Command,
    /// The deck, for the commands that need one.
    pub netlist: Option<PathBuf>,
    /// Where `simulate` writes its rawfile.
    pub output: Option<PathBuf>,
    /// Whether `gnd` is aliased to node `0`, as ngspice does by default.
    pub auto_gnd: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            command: Command::Summary,
            netlist: None,
            output: None,
            auto_gnd: true,
        }
    }
}

impl Args {
    /// Parses command-line arguments, excluding the program name.
    ///
    /// # Errors
    ///
    /// A message describing the problem, for printing next to the usage text.
    pub fn parse<I, S>(arguments: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut args = Self::default();
        let mut command_seen = false;
        let mut arguments = arguments.into_iter().map(Into::into);

        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "-h" | "--help" => {
                    args.command = Command::Help;
                    return Ok(args);
                }
                "-V" | "--version" => {
                    args.command = Command::Version;
                    return Ok(args);
                }
                "--no-auto-gnd" => args.auto_gnd = false,
                "--auto-gnd" => args.auto_gnd = true,
                "--output" => {
                    let path = arguments
                        .next()
                        .ok_or("--output needs a path: --output <path>")?;
                    if path.is_empty() {
                        return Err("--output needs a path: --output <path>".to_owned());
                    }
                    args.set_output(PathBuf::from(path))?;
                }
                other if other.starts_with("--output=") => {
                    let path = &other["--output=".len()..];
                    if path.is_empty() {
                        return Err("--output needs a path: --output <path>".to_owned());
                    }
                    args.set_output(PathBuf::from(path))?;
                }
                other if other.starts_with("--") => {
                    return Err(format!("unknown option '{other}'"));
                }
                other if other.starts_with('-') && other.len() > 1 => {
                    return Err(format!("unknown option '{other}'"));
                }
                other => {
                    // The first positional argument may be a subcommand. A deck
                    // named after a subcommand has to be written as `./tokens`.
                    match (command_seen, Command::parse(other)) {
                        (false, Some(command)) => {
                            args.command = command;
                            command_seen = true;
                        }
                        _ => {
                            if args.netlist.is_some() {
                                return Err(format!("unexpected argument '{other}'"));
                            }
                            args.netlist = Some(PathBuf::from(other));
                        }
                    }
                }
            }
        }

        if args.command.needs_netlist() && args.netlist.is_none() {
            return Err(format!("'{}' needs a netlist file", args.command.name()));
        }
        if !args.command.needs_netlist() && args.netlist.is_some() {
            return Err(format!(
                "'{}' does not take a netlist file",
                args.command.name()
            ));
        }
        if args.command.needs_output() && args.output.is_none() {
            return Err("'simulate' needs an output path: --output <path> <netlist>".to_owned());
        }
        if !args.command.needs_output() && args.output.is_some() {
            return Err(format!(
                "'{}' does not take --output (only 'simulate' writes a rawfile)",
                args.command.name()
            ));
        }
        Ok(args)
    }

    /// Records `--output`, rejecting a repeated option.
    fn set_output(&mut self, path: PathBuf) -> Result<(), String> {
        if self.output.is_some() {
            return Err("--output is given more than once".to_owned());
        }
        self.output = Some(path);
        Ok(())
    }
}

/// The usage text.
#[must_use]
pub fn usage() -> &'static str {
    "\
spice-rs — a Rust port of ngspice

USAGE:
    spice-rs [OPTIONS] <netlist>          print a summary of the deck (default)
    spice-rs cards [OPTIONS] <netlist>    list the cards and their classification
    spice-rs tokens [OPTIONS] <netlist>   dump the token stream
    spice-rs parse [OPTIONS] <netlist>    parse the deck into the netlist model
    spice-rs simulate (--output <path>) [OPTIONS] <netlist>
                                          run every analysis and write one ASCII rawfile
    spice-rs devices                      list the device designators known to the port
    spice-rs analyses                     list the analyses and their driver status
    spice-rs help | version

OPTIONS:
    --output <path>  where 'simulate' writes the rawfile; also --output=<path>.
                     An existing destination is replaced only after a successful
                     run; a failed run leaves it untouched
    --no-auto-gnd    treat 'gnd' as an ordinary node in the parser, like
                     ngspice's no_auto_gnd (the device node table still folds
                     'gnd'; see docs/port/CLI.md)
    -h, --help       print this text
    -V, --version    print the version

EXIT STATUS:
    0 success, 1 bad command line, 2 deck could not be read, simulation,
    numerical or output failure, 3 not ported yet"
}

/// Runs the parsed command, printing to stdout.
///
/// # Errors
///
/// [`crate::primitives::SpiceError::Io`] when the deck cannot be read,
/// [`crate::primitives::SpiceError::Parse`] when it cannot be understood, and
/// [`crate::primitives::SpiceError::NotYetPorted`] for syntax outside the
/// parser's current subset, mapped to [`exit_code::NOT_YET_PORTED`].
/// `simulate` adds the run and rawfile failures of [`crate::cli::simulate::run`].
pub fn run(args: &Args) -> SpiceResult<()> {
    match args.command {
        Command::Help => {
            println!("{}", usage());
            Ok(())
        }
        Command::Version => {
            println!("spice-rs {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::Devices => {
            print!("{}", devices_text(&Registry::with_builtins()));
            Ok(())
        }
        Command::Analyses => {
            print!("{}", analyses_text());
            Ok(())
        }
        Command::Simulate => {
            let deck = args
                .netlist
                .as_ref()
                .expect("Args::parse guarantees a netlist");
            let output = args
                .output
                .as_ref()
                .expect("Args::parse guarantees --output for 'simulate'");
            let report = crate::cli::simulate::run(deck, output, args.auto_gnd)?;
            print!("{}", crate::cli::simulate::report_text(&report));
            Ok(())
        }
        Command::Summary | Command::Cards | Command::Tokens => {
            let deck = load_deck(args)?;
            let cards = classify_deck(&deck)?;
            let text = match args.command {
                Command::Cards => cards_text(&deck, &cards),
                Command::Tokens => tokens_text(&cards),
                _ => summary_text(&deck, &cards, args.auto_gnd),
            };
            print!("{text}");
            Ok(())
        }
        Command::Parse => {
            let path = args
                .netlist
                .as_ref()
                .expect("Args::parse guarantees a netlist");
            let parser = crate::netlist::Parser::with_auto_gnd(args.auto_gnd);
            let netlist = parser.parse_file(path)?;
            // Options are validated before reporting success: unknown or
            // unsupported settings are errors, never ignored.
            let config = crate::analysis::RunConfig::from_netlist(&netlist)?;
            println!(
                "{}: {} device instance(s), {} model(s), {} subcircuit(s), {} analysis request(s)",
                netlist.title,
                netlist.top_level_device_count(),
                netlist.models.len(),
                netlist.subcircuits.len(),
                netlist.analyses.len()
            );
            // Top-level parameters and `{expr}` sites must evaluate; the CLI
            // parse status never hides an unresolved value.
            let elaborated = crate::netlist::elaborate::literalize(&netlist)?;
            if !netlist.params.is_empty() || !elaborated.sites.is_empty() {
                println!(
                    "parameters: {} definition(s), {} value site(s) evaluated",
                    elaborated.scope.entries().len(),
                    elaborated.sites.len()
                );
            }
            let context = config.context();
            println!(
                "options: {} setting(s); TEMP = {} C, TNOM = {} C; {} global node card(s)",
                config.applied().len(),
                context.temperature,
                context.nominal_temperature,
                netlist.globals.len()
            );
            // Accepted documented no-ops are named, never silently dropped.
            if !config.ignored().is_empty() {
                let names: Vec<_> = config
                    .ignored()
                    .iter()
                    .map(|option| option.name.as_str())
                    .collect();
                println!(
                    "options without effect (documented no-ops): {}",
                    names.join(", ")
                );
            }
            Ok(())
        }
    }
}

fn load_deck(args: &Args) -> SpiceResult<Deck> {
    let path = args
        .netlist
        .as_ref()
        .expect("Args::parse guarantees a netlist for this command");
    load(path)
}

/// Renders the deck summary.
#[must_use]
pub fn summary_text(deck: &Deck, cards: &[RawCard], auto_gnd: bool) -> String {
    let mut out = String::new();
    let title = if deck.title.trim().is_empty() {
        "<empty title line>"
    } else {
        deck.title.trim()
    };
    let _ = writeln!(out, "deck:    {}", deck.path.display());
    let _ = writeln!(out, "title:   {title}");
    let _ = writeln!(out, "cards:   {}", cards.len());

    let devices: Vec<&RawCard> = cards.iter().filter(|card| card.kind.is_device()).collect();
    let commands: Vec<&RawCard> = cards
        .iter()
        .filter(|card| card.kind.is_dot_command())
        .collect();
    let unclassified = cards.len() - devices.len() - commands.len();
    let _ = writeln!(out, "  device instances: {}", devices.len());
    let _ = writeln!(out, "  dot commands:     {}", commands.len());
    let _ = writeln!(out, "  unclassified:     {unclassified}");

    let mut by_designator: BTreeMap<char, usize> = BTreeMap::new();
    for card in &devices {
        if let Some(designator) = card.designator() {
            *by_designator.entry(designator).or_default() += 1;
        }
    }
    if !by_designator.is_empty() {
        let rendered: Vec<String> = by_designator
            .iter()
            .map(|(designator, count)| format!("{designator}({count})"))
            .collect();
        let _ = writeln!(out, "  by designator:    {}", rendered.join(" "));
    }

    let mut analyses: Vec<&'static str> = cards
        .iter()
        .filter_map(RawCard::analysis)
        .map(Kind::as_str)
        .collect();
    analyses.sort_unstable();
    analyses.dedup();
    if analyses.is_empty() {
        let _ = writeln!(out, "analyses: none requested");
    } else {
        let rendered: Vec<String> = analyses.iter().map(|name| format!(".{name}")).collect();
        let _ = writeln!(out, "analyses: {}", rendered.join(" "));
    }

    let registry = Registry::with_builtins();
    let _ = writeln!(
        out,
        "gnd:      {}",
        if auto_gnd {
            "aliased to node 0 (gnd)"
        } else {
            "an ordinary node (no_auto_gnd)"
        }
    );
    let _ = writeln!(
        out,
        "port:     {} ported and {} bounded of {} device designators; {} of {} analysis drivers present",
        registry.ported_count(),
        registry.bounded_count(),
        registry.len(),
        DRIVERS.len(),
        Kind::ALL.len()
    );
    out
}

/// Renders every card, one per line.
#[must_use]
pub fn cards_text(_deck: &Deck, cards: &[RawCard]) -> String {
    let mut out = String::new();
    for card in cards {
        let kind = match &card.kind {
            crate::netlist::CardKind::Device { designator } => format!("device '{designator}'"),
            crate::netlist::CardKind::DotCommand(command) => command.card_name(),
            crate::netlist::CardKind::Unknown => "?".to_owned(),
        };
        let _ = writeln!(out, "{:>5}  {:<16} {}", card.location.line, kind, card.raw);
    }
    out
}

/// Renders the token stream of every card.
#[must_use]
pub fn tokens_text(cards: &[RawCard]) -> String {
    let mut out = String::new();
    for card in cards {
        let _ = writeln!(out, "{:>5}  {}", card.location.line, card.raw);
        for token in &card.tokens {
            let _ = writeln!(
                out,
                "         col {:<4} {:<11} {}",
                token.location.column,
                token.kind.to_string(),
                token.text
            );
        }
    }
    out
}

/// Renders the device registry: `ported` devices build from their card,
/// `bounded` ones only from a deck (with the subset built), `pending` ones are
/// refused with their C reference.
#[must_use]
pub fn devices_text(registry: &Registry) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{} device designator(s): {} ported, {} bounded, {} pending",
        registry.len(),
        registry.ported_count(),
        registry.bounded_count(),
        registry.len() - registry.ported_count() - registry.bounded_count()
    );
    let width = registry
        .entries()
        .map(|entry| entry.description.chars().count())
        .max()
        .unwrap_or(0);
    for entry in registry.entries() {
        let _ = writeln!(
            out,
            "  {}  {:<width$}  {:<7}  {}",
            entry.designator,
            entry.description,
            entry.support.label(),
            entry.c_reference
        );
        if let DeviceSupport::Bounded { scope } = entry.support {
            let _ = writeln!(out, "     {:<width$}  {:<7}  built: {scope}", "", "");
        }
    }
    out
}

/// Renders the analysis coverage.
#[must_use]
pub fn analyses_text() -> String {
    let mut out = String::new();
    let _ = writeln!(out, "{} analysis kind(s)", Kind::ALL.len());
    for kind in Kind::ALL {
        let status = match crate::analysis::support(kind) {
            AnalysisSupport::Driver => "driver (bounded subset)".to_owned(),
            AnalysisSupport::PostProcessor { of } => {
                format!("post-processes the .{} plot", of.as_str())
            }
            AnalysisSupport::Missing => "no driver".to_owned(),
        };
        let _ = writeln!(
            out,
            "  .{:<8} {:<24} {status}",
            kind.as_str(),
            kind_name(kind)
        );
    }
    let _ = writeln!(
        out,
        "\nDevices: see `spice-rs devices` (linear, controlled, behavioural, K, switches and bounded \
         diode/BJT/MOS1). Every analysis card of a deck runs, in ngspice batch order. \
         .tran runs the trap/Gear companion driver (backend=diffsol method=bdf selects BDF \
         for linear decks). See docs/port/CLI.md and docs/port/TRANSIENT.md."
    );
    out
}

pub(crate) fn kind_name(kind: Kind) -> &'static str {
    match kind {
        Kind::OperatingPoint => "DC operating point",
        Kind::DcSweep => "DC sweep",
        Kind::Ac => "AC small-signal",
        Kind::Transient => "transient",
        Kind::Noise => "noise",
        Kind::Distortion => "distortion",
        Kind::PoleZero => "pole-zero",
        Kind::Sensitivity => "sensitivity",
        Kind::TransferFunction => "transfer function",
        Kind::Fourier => "Fourier",
        Kind::SParameter => "S-parameter",
    }
}

#[cfg(test)]
mod tests {
    use super::{Args, Command, analyses_text, cards_text, devices_text, summary_text};
    use crate::devices::Registry;
    use crate::netlist::{classify_deck, source::parse_deck_text};
    use std::path::{Path, PathBuf};

    const DECK: &str = "\
RC divider
v1 in 0 dc 5
r1 in out 1k
r2 out 0 1k
.model dm d(is=1e-14)
.tran 1u 10u
.end
";

    fn args(list: &[&str]) -> Result<Args, String> {
        Args::parse(list.iter().copied())
    }

    #[test]
    fn defaults_to_a_summary_of_one_netlist() {
        let parsed = args(&["rc.cir"]).expect("parses");
        assert_eq!(parsed.command, Command::Summary);
        assert_eq!(parsed.netlist, Some(PathBuf::from("rc.cir")));
        assert!(parsed.auto_gnd);
    }

    #[test]
    fn recognises_subcommands() {
        assert_eq!(args(&["cards", "rc.cir"]).unwrap().command, Command::Cards);
        assert_eq!(
            args(&["tokens", "rc.cir"]).unwrap().command,
            Command::Tokens
        );
        assert_eq!(
            args(&["simulate", "--output", "out.raw", "rc.cir"])
                .unwrap()
                .command,
            Command::Simulate
        );
        assert_eq!(args(&["devices"]).unwrap().command, Command::Devices);
        assert_eq!(args(&["analyses"]).unwrap().command, Command::Analyses);
        assert_eq!(args(&["--help"]).unwrap().command, Command::Help);
        assert_eq!(args(&["-V"]).unwrap().command, Command::Version);
    }

    #[test]
    fn flags_are_recognised_anywhere() {
        let parsed = args(&["cards", "--no-auto-gnd", "rc.cir"]).unwrap();
        assert_eq!(parsed.command, Command::Cards);
        assert!(!parsed.auto_gnd);
    }

    #[test]
    fn a_netlist_named_like_a_subcommand_is_still_a_netlist() {
        let parsed = args(&["parse", "cards"]).unwrap();
        assert_eq!(parsed.command, Command::Parse);
        assert_eq!(parsed.netlist, Some(PathBuf::from("cards")));
    }

    #[test]
    fn bad_command_lines_are_rejected() {
        assert!(args(&[]).is_err(), "a netlist is required");
        assert!(args(&["--nope", "rc.cir"]).is_err());
        assert!(args(&["rc.cir", "other.cir"]).is_err());
        assert!(args(&["devices", "rc.cir"]).is_err());
    }

    #[test]
    fn simulate_needs_an_output_path_and_sets_it() {
        let parsed = args(&["simulate", "--output", "out.raw", "rc.cir"]).unwrap();
        assert_eq!(parsed.command, Command::Simulate);
        assert_eq!(parsed.output, Some(PathBuf::from("out.raw")));
        assert_eq!(parsed.netlist, Some(PathBuf::from("rc.cir")));

        // The option may precede its subcommand, and `--output=<path>` is the
        // same option.
        let parsed = args(&["--output", "out.raw", "simulate", "rc.cir"]).unwrap();
        assert_eq!(parsed.command, Command::Simulate);
        assert_eq!(parsed.output, Some(PathBuf::from("out.raw")));
        let parsed = args(&["simulate", "--output=out.raw", "rc.cir"]).unwrap();
        assert_eq!(parsed.output, Some(PathBuf::from("out.raw")));
    }

    #[test]
    fn a_broken_or_misplaced_output_option_is_a_usage_error() {
        for list in [
            vec!["simulate", "rc.cir"],
            vec!["simulate", "--output"],
            vec!["simulate", "--output=", "rc.cir"],
            vec![
                "simulate", "--output", "a.raw", "--output", "b.raw", "rc.cir",
            ],
            vec!["cards", "--output", "out.raw", "rc.cir"],
            vec!["devices", "--output", "out.raw"],
            vec!["--output", "out.raw", "rc.cir"],
        ] {
            assert!(args(&list).is_err(), "{list:?}");
        }
    }

    fn classified() -> (crate::netlist::Deck, Vec<crate::netlist::RawCard>) {
        let deck = parse_deck_text(Path::new("rc.cir"), DECK);
        let cards = classify_deck(&deck).expect("classifies");
        (deck, cards)
    }

    #[test]
    fn the_summary_counts_what_the_deck_contains() {
        let (deck, cards) = classified();
        let text = summary_text(&deck, &cards, true);
        assert!(text.contains("title:   RC divider"), "{text}");
        assert!(text.contains("cards:   6"), "{text}");
        assert!(text.contains("device instances: 3"), "{text}");
        assert!(text.contains("dot commands:     3"), "{text}");
        assert!(text.contains("by designator:    r(2) v(1)"), "{text}");
        assert!(text.contains("analyses: .tran"), "{text}");
        assert!(
            text.contains("port:     11 ported and 6 bounded of 26 device designators"),
            "{text}"
        );
    }

    #[test]
    fn the_summary_reports_the_gnd_rule() {
        let (deck, cards) = classified();
        assert!(summary_text(&deck, &cards, true).contains("aliased to node 0"));
        assert!(summary_text(&deck, &cards, false).contains("ordinary node"));
    }

    #[test]
    fn the_card_list_names_each_classification() {
        let (deck, cards) = classified();
        let text = cards_text(&deck, &cards);
        assert!(text.contains("device 'r'"), "{text}");
        assert!(text.contains(".tran"), "{text}");
        assert!(text.contains("model dm d(is=1e-14)"), "{text}");
    }

    #[test]
    fn the_device_list_distinguishes_ported_bounded_and_pending() {
        let text = devices_text(&Registry::with_builtins());
        assert!(
            text.starts_with("26 device designator(s): 11 ported, 6 bounded, 9 pending"),
            "{text}"
        );
        let status = |letter: char| {
            text.lines()
                .find(|line| line.starts_with(&format!("  {letter}  ")))
                .and_then(|line| {
                    line.split_whitespace()
                        .find(|word| matches!(*word, "ported" | "bounded" | "pending"))
                })
                .unwrap_or_else(|| panic!("no status for {letter}\n{text}"))
        };
        for letter in ['r', 'c', 'l', 'v', 'i', 'e', 'f', 'g', 'h', 'b', 'k'] {
            assert_eq!(status(letter), "ported", "{letter}");
        }
        for letter in ['d', 'q', 'm', 's', 'w', 'x'] {
            assert_eq!(status(letter), "bounded", "{letter}");
        }
        for letter in ['j', 'z', 't', 'o', 'y', 'u', 'n', 'p', 'a'] {
            assert_eq!(status(letter), "pending", "{letter}");
        }
        assert!(text.contains("built: MOS1 (level 1)"), "{text}");
        assert!(text.contains("inp2r.c"), "{text}");
        assert!(text.contains("devices/ind/mutsetup.c"), "{text}");
    }

    #[test]
    fn the_analysis_list_covers_every_kind() {
        let text = analyses_text();
        for kind in crate::primitives::AnalysisKind::ALL {
            assert!(text.contains(kind.as_str()), "missing {kind:?}\n{text}");
        }
        let line = |name: &str| {
            text.lines()
                .find(|line| line.trim_start().starts_with(name))
                .unwrap_or_else(|| panic!("{name}\n{text}"))
        };
        for name in [".op ", ".dc ", ".ac ", ".tran ", ".pz ", ".tf "] {
            assert!(line(name).ends_with("driver (bounded subset)"), "{text}");
        }
        assert!(
            line(".four ").ends_with("post-processes the .tran plot"),
            "{text}"
        );
        for name in [".noise ", ".disto ", ".sens "] {
            assert!(line(name).ends_with("no driver"), "{text}");
        }
        assert!(!text.contains("Linear R/C/L/V/I only"), "{text}");
    }
}
