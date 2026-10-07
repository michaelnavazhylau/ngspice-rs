//! `dump DECK OUT`: writes [`spice_wasm::simulate_rawfile`]'s rawfile for diffing.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let text = std::fs::read_to_string(&args[1]).unwrap();
    std::fs::write(&args[2], spice_wasm::simulate_rawfile(&text, &[]).unwrap()).unwrap();
}
