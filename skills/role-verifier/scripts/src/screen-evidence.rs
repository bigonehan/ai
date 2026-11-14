use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.len() != 4 || args.iter().any(|value| value.trim().is_empty()) {
        eprintln!("usage: screen-evidence <row> <screenshot.png> <consumer> <observed-value>");
        std::process::exit(2);
    }
    if args[0].parse::<u64>().is_err() || !args[1].ends_with(".png") {
        eprintln!("screen-evidence: row must be numeric and screenshot must be a PNG path");
        std::process::exit(2);
    }
    let clean = |value: &str| value.replace(['\r', '\n'], " ");
    println!("[verify-step] screenshot={}", clean(&args[1]));
    println!("[verify-step] readback consumer={} value={}", clean(&args[2]), clean(&args[3]));
}
