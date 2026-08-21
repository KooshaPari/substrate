# CLAUDE.md  
  
## Project Context  
  
Substrate is a Rust workspace providing shared infrastructure for the KooshaPari ecosystem.  
  
## Build  
  
cargo build  
  
## Test  
  
cargo test  
  
## Lint  
  
cargo clippy -- -D warnings  
cargo fmt --check  
  
## Architecture  
  
This is a library crate providing shared utilities. Follow conventional commits.  
All PRs require CI pass. No unsafe code allowed.  
