use cozy_syzygy::{Tablebase, Wdl};

fn check_position(tb: &Tablebase, fen: &str, expected: Wdl, capture: bool) {
    println!("{fen}");
    match tb.probe_wdl(&fen.parse().unwrap()) {
        Some((wdl, true)) => println!("  TB says:  {wdl:?} with a capture"),
        Some((wdl, false)) => println!("  TB says:  {wdl:?} without a capture"),
        None => println!("  TB doesn't have any data for this position"),
    }
    match capture {
        true => println!("  Expected: {expected:?} with a capture"),
        false => println!("  Expected: {expected:?} without a capture"),
    }
}

fn main() {
    let syzygy_path = std::env::args_os().nth(1).unwrap_or_else(|| {
        eprintln!("First argument should be the path to the Syzygy tablebase files");
        std::process::exit(1)
    });

    let tb = Tablebase::new(syzygy_path);
    println!("Always have answers for up to {} men", tb.min_pieces());
    println!("Might have answers for up to {} men", tb.max_pieces());

    check_position(&tb, "4k3/8/8/1R6/4K3/8/8/8 w - - 0 1", Wdl::Win, false);
    check_position(&tb, "4k3/8/8/1R6/4K3/8/8/8 b - - 0 1", Wdl::Loss, false);
    check_position(&tb, "7k/5KR1/8/8/8/8/8/8 b - - 0 1", Wdl::Draw, false);
    check_position(&tb, "7k/5KR1/8/8/8/8/8/r7 w - - 0 1", Wdl::Draw, false);
    check_position(&tb, "7k/5KR1/8/8/8/8/8/r7 b - - 0 1", Wdl::Win, false);
    check_position(&tb, "7k/5KR1/8/8/8/8/8/6r1 w - - 0 1", Wdl::Win, true);
    check_position(&tb, "7k/5KR1/8/8/8/2R5/8/r7 w - - 0 1", Wdl::Win, false);
    check_position(&tb, "7k/2Q2K2/8/8/8/3r4/8/r7 w - - 0 1", Wdl::Win, false);
    check_position(&tb, "7k/2Q2K2/8/8/8/3r4/8/r7 b - - 0 1", Wdl::Win, false);
    check_position(&tb, "7k/2Q2K2/4n3/4r3/8/8/8/8 w - - 0 1", Wdl::Win, true);
    check_position(&tb, "7k/2Q2K2/4n3/4r3/8/8/8/8 b - - 0 1", Wdl::Win, true);
    check_position(&tb, "8/6B1/8/8/B7/8/K2k4/2n5 w - - 0 1", Wdl::CursedWin, false);
    check_position(&tb, "8/6B1/8/8/B7/1K6/3kn3/8 b - - 0 1", Wdl::BlessedLoss, false);
}
