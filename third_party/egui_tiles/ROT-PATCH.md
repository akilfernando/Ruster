# Rot Engine patch

This is published egui_tiles 0.18.0 (rerun-io/egui_crates, MIT OR Apache-2.0),
copied unchanged from crates.io except for three lines: the divider strips in
`src/container/linear.rs` and `src/container/grid.rs` and the tab-bar
background in `src/container/tabs.rs` sense click and drag without
`FOCUSABLE`, so keyboard Tab skips them (UX-STANDARDS §7). The license files
are the upstream repository's. Remove this copy when upstream makes those
surfaces non-focusable or configurable.
