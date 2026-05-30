
default:
   just --list

[env("RUST_LOG", "info")]
run:
   cargo run


