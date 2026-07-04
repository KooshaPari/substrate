//! Colored startup banner.
const CYAN: &str = "\x1b[96m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

pub fn print_banner(port: u16) {
    println!("{CYAN} ╔═══════════════════════════════════╗{RESET}");
    println!("{CYAN} ║   SUBSTRATE  GATEWAY  v0.2.0      ║{RESET}");
    println!("{CYAN} ╚═══════════════════════════════════╝{RESET}");
    println!("{DIM}  AI dispatch gateway · port {port}{RESET}");
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn escape_codes_defined() { assert!(CYAN.starts_with("\x1b")); }
    #[test] fn dim_defined() { assert!(DIM.starts_with("\x1b")); }
}