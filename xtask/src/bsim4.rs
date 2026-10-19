//! Extract BSIM4 interface facts, not C implementation code. The committed
//! tables are offline input-schema artifacts; Cargo never needs the C tree.
use crate::ifparm::{self, Spec};

const USAGE: &str = "usage: cargo xtask bsim4-schema --source <ngspice-checkout> [--check]";

const SPEC: Spec = Spec {
    directory: "bsim4",
    prefix: "BSIM4_",
    command: "bsim4-schema",
    interface_inputs: "{b4.c,bsim4def.h,b4mpar.c,b4par.c}",
    model_table: "BSIM4mPTable",
    instance_table: "BSIM4pTable",
    setup_file: "b4set.c",
    instance_boundary: "for (here = BSIM4instances",
    model_expected: 897,
    instance_expected: 98,
    literal_expected: 777,
};

pub(crate) fn main(args: &[String]) -> Result<(), String> {
    let parsed = ifparm::args(args, USAGE)?;
    let source = parsed.source;
    let files = [
        "b4.c",
        "bsim4def.h",
        "b4mpar.c",
        "b4par.c",
        "b4set.c",
        "b4temp.c",
    ]
    .map(|name| format!("src/spicelib/devices/bsim4/{name}"));
    let revision = ifparm::revision(&source, &files)?;
    let read = |name: &str| ifparm::read(&source, &SPEC, name);
    let text = ifparm::parameters(
        &SPEC,
        &read("b4.c")?,
        &read("bsim4def.h")?,
        &read("b4mpar.c")?,
        &read("b4par.c")?,
        &revision,
    )?;
    let defaults = ifparm::literal_defaults(
        &SPEC,
        &read("b4.c")?,
        &read("b4mpar.c")?,
        &read("b4set.c")?,
        &revision,
    )?;
    let fields = crate::bsim4_fields::fields(&read("bsim4def.h")?, &revision)?;
    for (path, text) in [
        ("src/devices/bsim4/parameters.rs", text),
        ("src/devices/bsim4/defaults_data.rs", defaults),
        ("src/devices/bsim4/fields.rs", fields),
    ] {
        ifparm::emit(path, &text, parsed.check)?;
    }
    if parsed.check {
        println!("BSIM4 schema/default facts match {revision}");
    }
    Ok(())
}
