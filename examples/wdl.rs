use cozy_syzygy::{Tablebase, Wdl};

fn main() {
    let syzygy_path = std::env::args_os().nth(1).unwrap_or_else(|| {
        eprintln!("First argument should be the path to the Syzygy tablebase files");
        std::process::exit(1)
    });

    let tb = Tablebase::new(syzygy_path);
    println!("Always have answers for up to {} men", tb.min_pieces());
    println!("Might have answers for up to {} men", tb.max_pieces());

    let mut fails = 0;
    let mut tests = 0;

    let mut check_pos = |fen: &str, expected, capture| {
        println!("{fen}");
        let result = tb.probe_wdl(&fen.parse().unwrap());
        match result {
            Some((wdl, true)) => println!("  TB says:  {wdl:?} with a capture"),
            Some((wdl, false)) => println!("  TB says:  {wdl:?} without a capture"),
            None => println!("  TB doesn't have any data for this position"),
        }
        match capture {
            true => println!("  Expected: {expected:?} with a capture"),
            false => println!("  Expected: {expected:?} without a capture"),
        }
        tests += 1;
        fails += (result != Some((expected, capture))) as usize;
    };

    println!("Testing some pawnless positions");
    check_pos("4k3/8/8/1R6/4K3/8/8/8 w - - 0 1", Wdl::Win, false);
    check_pos("4k3/8/8/1R6/4K3/8/8/8 b - - 0 1", Wdl::Loss, false);
    check_pos("7k/5KR1/8/8/8/8/8/8 b - - 0 1", Wdl::Draw, false);
    check_pos("7k/5KR1/8/8/8/8/8/r7 w - - 0 1", Wdl::Draw, false);
    check_pos("7k/5KR1/8/8/8/8/8/r7 b - - 0 1", Wdl::Win, false);
    check_pos("7k/5KR1/8/8/8/8/8/6r1 w - - 0 1", Wdl::Win, true);
    check_pos("7k/5KR1/8/8/8/2R5/8/r7 w - - 0 1", Wdl::Win, false);
    check_pos("7k/2Q2K2/8/8/8/3r4/8/r7 w - - 0 1", Wdl::Win, false);
    check_pos("7k/2Q2K2/8/8/8/3r4/8/r7 b - - 0 1", Wdl::Win, false);
    check_pos("7k/2Q2K2/4n3/4r3/8/8/8/8 w - - 0 1", Wdl::Win, true);
    check_pos("7k/2Q2K2/4n3/4r3/8/8/8/8 b - - 0 1", Wdl::Win, true);
    check_pos("8/6B1/8/8/B7/8/K2k4/2n5 w - - 0 1", Wdl::CursedWin, false);
    check_pos("8/6B1/8/8/B7/1K6/3kn3/8 b - - 0 1", Wdl::BlessedLoss, false);
    println!();

    println!("Testing some pawnful positions");
    check_pos("4k3/8/8/3K4/7p/8/8/8 w - - 0 1", Wdl::Draw, false);
    check_pos("8/8/8/4K3/1P5p/8/8/4k3 b - - 0 1", Wdl::Win, false);
    check_pos("8/8/8/4K3/1P5p/8/8/4k3 w - - 0 1", Wdl::Win, false);
    check_pos("8/8/3K4/6R1/7k/7p/8/8 b - - 0 1", Wdl::Win, true);
    check_pos("8/6B1/8/8/B7/8/K1pk4/8 b - - 0 1", Wdl::BlessedLoss, false);

    // Stalemate if no EP
    check_pos("K7/1r6/1k6/1Pp5/8/8/8/8 w - c6 0 1", Wdl::Loss, true);
    check_pos("K7/1r6/1k6/1Pp5/8/8/8/8 w - - 0 1", Wdl::Draw, false);

    // EP is best move but not only move
    check_pos("5K2/8/5k2/8/pP6/B7/8/8 b - b3 0 1", Wdl::Draw, true);
    check_pos("5K2/8/5k2/8/pP6/B7/8/8 b - - 0 1", Wdl::Loss, false);

    println!("{tests} tests, {fails} fails");
}
