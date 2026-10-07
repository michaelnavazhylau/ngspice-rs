//! Native counterpart of `web/bench.mjs`: `bench REPS DECK...` prints, per deck,
//! the median wall time of [`spice_wasm::simulate_rawfile`] and an FNV-1a hash of the
//! rawfile so wasm and native outputs can be compared byte for byte.
use std::time::Instant;

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn main() {
    let mut args = std::env::args().skip(1);
    let reps: usize = args.next().and_then(|r| r.parse().ok()).unwrap_or(5);
    for path in args {
        let text = std::fs::read_to_string(&path).unwrap();
        let name = std::path::Path::new(&path)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let mut times = Vec::new();
        let mut result = Err(String::new());
        for _ in 0..reps {
            let start = Instant::now();
            result = spice_wasm::simulate_rawfile(&text, &[]);
            times.push(start.elapsed().as_secs_f64() * 1e3);
        }
        times.sort_by(f64::total_cmp);
        match result {
            Ok(raw) => println!(
                "{name}\t{:.3}\t{}\t{:016x}",
                times[reps / 2],
                raw.len(),
                fnv(&raw)
            ),
            Err(e) => println!("{name}\tERR\t{}", e.lines().next().unwrap_or("")),
        }
    }
}
