
default:
   just --list

[env("RUST_LOG", "info")]
run:
   cargo run

[env("RUST_LOG", "debug")]
dbg:
   cargo run


[working-directory: 'shaders']
cmp:
   glslc shader.vert -o vert.spv
   glslc shader.frag -o frag.spv

