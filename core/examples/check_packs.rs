//! Loads all packs of a directory and prints problems – a quick check for
//! pack authors.
//!
//! ```bash
//! cargo run -p tdr-core --example check_packs -- components
//! ```

fn main() {
    let dir = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "components".into());
    let (packs, errors) = tdr_core::pack::load_all(std::path::Path::new(&dir));
    for pack in &packs {
        println!(
            "ok  {} {} ({} types)",
            pack.info.name,
            pack.info.version,
            pack.types.len()
        );
    }
    for error in &errors {
        eprintln!("error: {error}\n");
    }
    std::process::exit(if errors.is_empty() { 0 } else { 1 });
}
