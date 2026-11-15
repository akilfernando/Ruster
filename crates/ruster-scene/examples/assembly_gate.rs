//! Experimental CPU-only assembly gate; not a serialized scene capability.
#[path = "../src/assembly_prototype.rs"]
mod support;
#[cfg(test)]
#[path = "assembly_gate/tests.rs"]
mod tests;

fn main() {
    support::run_gate();
}

#[test]
fn nested_motion_reaches_flat_scene() {
    support::run_gate();
}
