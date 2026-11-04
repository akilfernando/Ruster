//! Bounded, function-only Rhai behaviors over copied scene transforms.
//!
//! This is an in-process creator scripting boundary, not process isolation or a
//! universal security sandbox. See the crate README for the supported contract.

use std::collections::BTreeMap;

use rhai::{CallFnOptions, Dynamic, Engine, Map, OptimizationLevel, Scope, AST, FLOAT, INT};
use ruster_scene::Transform;

pub type Properties = BTreeMap<String, f64>;

pub const MAX_SOURCE_BYTES: usize = 16 * 1024;
pub const MAX_PROPERTIES: usize = 16;
pub const MAX_NUMBER: f64 = 1_000_000.0;
const SOURCE: &str = "behavior.rhai";
const TRANSFORM_KEYS: [&str; 9] = ["x", "y", "z", "rx", "ry", "rz", "sx", "sy", "sz"];

/// Compiled behavior and its validated numeric property schema.
///
/// Calls have fresh scopes and copied inputs. Only `properties()` executes at
/// compile time; `update(object, props, dt, time)` executes on explicit updates.
pub struct Behavior {
    engine: Engine,
    ast: AST,
    defaults: Properties,
}

impl Behavior {
    pub fn compile(source: &str) -> Result<Self, String> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(diagnostic("source exceeds 16384 bytes"));
        }
        let engine = bounded_engine();
        let mut ast = engine
            .compile(source)
            .map_err(|error| diagnostic(error.to_string()))?;
        ast.set_source(SOURCE);
        for (name, arity) in [("properties", 0), ("update", 4)] {
            if !ast
                .iter_functions()
                .any(|function| function.name == name && function.params.len() == arity)
            {
                return Err(diagnostic(format!(
                    "required function {name} with {arity} parameters is missing"
                )));
            }
        }
        // Do not execute top-level statements or retain mutable script globals.
        // Public Rhai AST introspection cannot reject statements without enabling
        // `internals`; the function-only contract explicitly ignores them.
        ast.clear_statements();
        let value = engine
            .call_fn_with_options::<Dynamic>(
                CallFnOptions::new().eval_ast(false),
                &mut Scope::new(),
                &ast,
                "properties",
                (),
            )
            .map_err(|error| diagnostic(error.to_string()))?;
        let defaults = read_properties(value)?;
        Ok(Self {
            engine,
            ast,
            defaults,
        })
    }

    pub fn defaults(&self) -> &Properties {
        &self.defaults
    }

    /// Evaluate a single update, validating the complete result before returning.
    /// Errors leave both caller-owned inputs untouched.
    pub fn update(
        &self,
        transform: &Transform,
        properties: &Properties,
        dt: f64,
        elapsed: f64,
    ) -> Result<Transform, String> {
        transform
            .validate()
            .map_err(|error| diagnostic(format!("input {error}")))?;
        if properties.keys().ne(self.defaults.keys()) {
            return Err(diagnostic("properties must have exactly the declared keys"));
        }
        for (name, value) in properties {
            validate_number(*value, name)?;
        }
        for (name, value) in [("dt", dt), ("time", elapsed)] {
            validate_number(value, name)?;
            if value < 0.0 {
                return Err(diagnostic(format!("{name} must be nonnegative")));
            }
        }
        let object: Map = TRANSFORM_KEYS
            .iter()
            .zip(
                transform
                    .translation
                    .iter()
                    .chain(&transform.rotation)
                    .chain(&transform.scale),
            )
            .map(|(name, value)| ((*name).into(), Dynamic::from_float(f64::from(*value))))
            .collect();
        let props: Map = properties
            .iter()
            .map(|(name, value)| (name.as_str().into(), Dynamic::from_float(*value)))
            .collect();
        let value = self
            .engine
            .call_fn_with_options::<Dynamic>(
                CallFnOptions::new().eval_ast(false),
                &mut Scope::new(),
                &self.ast,
                "update",
                (object, props, dt, elapsed),
            )
            .map_err(|error| diagnostic(error.to_string()))?;
        read_transform(value)
    }
}

fn bounded_engine() -> Engine {
    let mut engine = Engine::new();
    engine
        .set_optimization_level(OptimizationLevel::None)
        .set_max_operations(20_000)
        .set_max_call_levels(16)
        .set_max_expr_depths(64, 64)
        .set_max_variables(64)
        .set_max_functions(32)
        .set_max_string_size(1024)
        .set_max_array_size(64)
        .set_max_map_size(32)
        .set_max_strings_interned(128)
        .set_allow_anonymous_fn(false)
        .set_fail_on_invalid_map_property(true)
        .on_print(|_| {})
        .on_debug(|_, _, _| {});
    // No host functions, custom types, module resolver or IO bindings are added.
    // Disable dynamic function construction/calls as well as direct eval/IO.
    for symbol in ["eval", "print", "debug", "Fn", "call", "curry"] {
        engine.disable_symbol(symbol);
    }
    engine
}

fn diagnostic(message: impl std::fmt::Display) -> String {
    format!("{SOURCE}: {message}")
}

fn validate_number(value: f64, name: &str) -> Result<(), String> {
    if value.is_finite() && value.abs() <= MAX_NUMBER {
        Ok(())
    } else {
        Err(diagnostic(format!(
            "{name} must be finite and within ±1000000"
        )))
    }
}

fn read_number(value: Dynamic, name: &str) -> Result<f64, String> {
    let number = if value.is::<FLOAT>() {
        value.cast::<FLOAT>()
    } else if value.is::<INT>() {
        value.cast::<INT>() as f64
    } else {
        return Err(diagnostic(format!("{name} must be numeric")));
    };
    validate_number(number, name)?;
    Ok(number)
}

fn valid_property_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    name.len() <= 32
        && matches!(bytes.next(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn read_properties(value: Dynamic) -> Result<Properties, String> {
    let map = value
        .try_cast::<Map>()
        .ok_or_else(|| diagnostic("properties() must return an object map"))?;
    if map.len() > MAX_PROPERTIES {
        return Err(diagnostic("at most 16 properties may be declared"));
    }
    map.into_iter()
        .map(|(name, value)| {
            if !valid_property_name(&name) {
                return Err(diagnostic(format!(
                    "property name '{name}' must be an ASCII identifier of 1..32 bytes"
                )));
            }
            let number = read_number(value, &name)?;
            Ok((name.into(), number))
        })
        .collect()
}

fn read_transform(value: Dynamic) -> Result<Transform, String> {
    let mut map = value
        .try_cast::<Map>()
        .ok_or_else(|| diagnostic("update() must return the object map"))?;
    if map.len() != TRANSFORM_KEYS.len() || TRANSFORM_KEYS.iter().any(|key| !map.contains_key(*key))
    {
        return Err(diagnostic(
            "update() must return exactly x,y,z,rx,ry,rz,sx,sy,sz",
        ));
    }
    let mut numbers = [0.0_f32; 9];
    for (index, key) in TRANSFORM_KEYS.iter().enumerate() {
        // Presence was checked above; extraction remains fallible at the boundary.
        let value = map
            .remove(*key)
            .ok_or_else(|| diagnostic(format!("missing transform field {key}")))?;
        numbers[index] = read_number(value, key)? as f32;
    }
    let transform = Transform {
        translation: [numbers[0], numbers[1], numbers[2]],
        rotation: [numbers[3], numbers[4], numbers[5]],
        scale: [numbers[6], numbers[7], numbers[8]],
    };
    transform
        .validate()
        .map_err(|error| diagnostic(format!("output {error}")))?;
    Ok(transform)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOB: &str = r#"
fn properties() { #{ speed: 1.0, amplitude: 2 } }
fn update(object, props, dt, time) {
    object.y = sin(time * props.speed) * props.amplitude;
    object.rx += dt;
    object
}
"#;

    fn with_body(body: &str) -> Behavior {
        Behavior::compile(&format!(
            "fn properties() {{ #{{ speed: 1.0 }} }}\nfn update(object, props, dt, time) {{ {body} }}"
        ))
        .unwrap()
    }

    #[test]
    fn defaults_edits_math_and_copied_transform_work() {
        let behavior = Behavior::compile(BOB).unwrap();
        assert_eq!(
            behavior.defaults(),
            &Properties::from([("amplitude".into(), 2.0), ("speed".into(), 1.0)])
        );
        let input = Transform::default();
        let mut props = behavior.defaults().clone();
        props.insert("amplitude".into(), 3.0);
        let result = behavior
            .update(&input, &props, 0.25, std::f64::consts::FRAC_PI_2)
            .unwrap();
        assert!((result.translation[1] - 3.0).abs() < 1e-5);
        assert_eq!(result.rotation[0], 0.25);
        assert_eq!(input, Transform::default());
        assert_eq!(props["amplitude"], 3.0);
        assert_eq!(behavior.defaults()["amplitude"], 2.0);
        assert_eq!(
            behavior
                .update(&input, &props, 0.25, std::f64::consts::FRAC_PI_2)
                .unwrap(),
            result,
            "each call starts without retained globals"
        );
    }

    #[test]
    fn compile_checks_schema_and_signatures_without_running_update_or_globals() {
        Behavior::compile(
            "while true {} fn properties() { #{} } fn update(o,p,d,t) { while true {} }",
        )
        .unwrap();
        let invalid = [
            "fn properties() { #{} }",
            "fn properties() { #{} } fn update(o,p,d) { o }",
            "fn properties(x) { #{} } fn update(o,p,d,t) { o }",
            "fn properties() { 1 } fn update(o,p,d,t) { o }",
            "fn properties() { #{ bad: true } } fn update(o,p,d,t) { o }",
            "fn properties() { #{ \"bad name\": 1 } } fn update(o,p,d,t) { o }",
            "fn properties() { #{ bad: 1000001 } } fn update(o,p,d,t) { o }",
            "fn properties() { #{ bad: sqrt(-1.0) } } fn update(o,p,d,t) { o }",
            "fn properties() { #{ bad: exp(1000.0) } } fn update(o,p,d,t) { o }",
        ];
        for source in invalid {
            assert!(Behavior::compile(source).is_err(), "accepted {source}");
        }
        let properties = (0..17)
            .map(|index| format!("p{index}: 1"))
            .collect::<Vec<_>>()
            .join(",");
        assert!(Behavior::compile(&format!(
            "fn properties() {{ #{{{properties}}} }} fn update(o,p,d,t) {{ o }}"
        ))
        .is_err());
        let long_name = "a".repeat(33);
        assert!(Behavior::compile(&format!(
            "fn properties() {{ #{{{long_name}: 1}} }} fn update(o,p,d,t) {{ o }}"
        ))
        .is_err());
    }

    #[test]
    fn input_property_schema_and_time_are_checked() {
        let behavior = with_body("object");
        let transform = Transform::default();
        for props in [
            Properties::new(),
            Properties::from([("speed".into(), 1.0), ("extra".into(), 1.0)]),
            Properties::from([("speed".into(), f64::NAN)]),
            Properties::from([("speed".into(), f64::INFINITY)]),
            Properties::from([("speed".into(), MAX_NUMBER + 1.0)]),
        ] {
            assert!(behavior.update(&transform, &props, 0.0, 0.0).is_err());
        }
        for (dt, elapsed) in [
            (f64::NAN, 0.0),
            (0.0, f64::INFINITY),
            (-1.0, 0.0),
            (0.0, -1.0),
            (MAX_NUMBER + 1.0, 0.0),
        ] {
            assert!(behavior
                .update(&transform, behavior.defaults(), dt, elapsed)
                .is_err());
        }
        let invalid = Transform {
            scale: [0.0; 3],
            ..transform
        };
        assert!(behavior
            .update(&invalid, behavior.defaults(), 0.0, 0.0)
            .is_err());
    }

    #[test]
    fn failed_output_is_atomic_and_complete_map_required() {
        let input = Transform::default();
        for body in [
            "object.y = 3; throw \"failed\"; object",
            "true",
            "#{ x: 1 }",
            "object.extra = 1; object",
            "object.y = \"bad\"; object",
            "object.y = sqrt(-1.0); object",
            "object.y = exp(1000.0); object",
            "object.y = 1000001; object",
            "object.y = 10001; object",
            "object.sx = 0.0; object",
            "object.sy = 1001.0; object",
            "object.rx = 10001; object",
            "object.y = 3; props.speed = 999; object.sx = -1; object",
        ] {
            let behavior = with_body(body);
            let props = behavior.defaults().clone();
            let error = behavior.update(&input, &props, 0.0, 0.0).unwrap_err();
            assert!(error.starts_with("behavior.rhai:"));
            assert_eq!(input, Transform::default());
            assert_eq!(props, behavior.defaults().clone());
        }
    }

    #[test]
    fn loops_and_recursion_are_bounded_in_both_entry_points() {
        let loop_behavior = with_body("while true {} object");
        let error = loop_behavior
            .update(&Transform::default(), loop_behavior.defaults(), 0.0, 0.0)
            .unwrap_err();
        assert!(error.to_lowercase().contains("operations"), "{error}");
        assert!(Behavior::compile(
            "fn properties() { while true {} #{} } fn update(o,p,d,t) { o }"
        )
        .is_err());
        let recursive = Behavior::compile(
            "fn recurse() { recurse() } fn properties() { #{} } fn update(o,p,d,t) { recurse(); o }",
        )
        .unwrap();
        let error = recursive
            .update(&Transform::default(), recursive.defaults(), 0.0, 0.0)
            .unwrap_err();
        assert!(error.to_lowercase().contains("stack"), "{error}");
        assert!(
            Behavior::compile("fn properties() { properties() } fn update(o,p,d,t) { o }").is_err()
        );
    }

    #[test]
    fn parser_and_dynamic_data_have_resource_limits() {
        assert!(Behavior::compile(&" ".repeat(MAX_SOURCE_BYTES + 1)).is_err());
        let deep = format!("{}1{}", "[".repeat(80), "]".repeat(80));
        let huge_array = format!("[{}]", ["1"; 65].join(","));
        let huge_map = format!(
            "#{{{}}}",
            (0..33)
                .map(|index| format!("p{index}:1"))
                .collect::<Vec<_>>()
                .join(",")
        );
        for expression in [
            deep,
            huge_array,
            huge_map,
            format!("\"{}\"", "x".repeat(1025)),
        ] {
            assert!(Behavior::compile(&format!(
                "fn properties() {{ let data = {expression}; #{{}} }} fn update(o,p,d,t) {{ o }}"
            ))
            .is_err());
        }
        for body in [
            "let a = []; loop { a.push(1); } object",
            "let a = \"x\"; loop { a += a; } object",
            "let a = []; for n in 0..80 { a = [a]; } object",
            "let n = 9223372036854775807; n += 1; object",
        ] {
            let behavior = with_body(body);
            assert!(
                behavior
                    .update(&Transform::default(), behavior.defaults(), 0.0, 0.0)
                    .is_err(),
                "accepted {body}"
            );
        }
        // Rhai 1.27's local bracket insertion does not check map size on each
        // insertion. The operation budget still terminates this growth loop;
        // do not advertise the configured map cap as a process-memory ceiling.
        let growing_map =
            with_body("let a = #{}; let n = 0; loop { a[n.to_string()] = n; n += 1; } object");
        let error = growing_map
            .update(&Transform::default(), growing_map.defaults(), 0.0, 0.0)
            .unwrap_err();
        assert!(error.to_lowercase().contains("operations"), "{error}");
        let many_vars = (0..65)
            .map(|index| format!("let v{index} = {index};"))
            .collect::<String>();
        assert!(Behavior::compile(&format!(
            "fn properties() {{ {many_vars} #{{}} }} fn update(o,p,d,t) {{ o }}"
        ))
        .is_err());
        let many_functions = (0..33)
            .map(|index| format!("fn f{index}() {{}}"))
            .collect::<String>();
        assert!(Behavior::compile(&many_functions).is_err());
    }

    #[test]
    fn dynamic_eval_io_function_pointers_and_modules_are_unavailable() {
        for expression in [
            "eval(\"40 + 2\")",
            "print(\"hello\")",
            "debug(\"hello\")",
            "Fn(\"eval\")",
            "|| 1",
            "import \"/tmp/script.rhai\" as external",
        ] {
            assert!(
                Behavior::compile(&format!(
                    "fn properties() {{ #{{}} }} fn update(o,p,d,t) {{ {expression}; o }}"
                ))
                .is_err(),
                "accepted {expression}"
            );
        }
        let behavior = with_body("read_file(\"/tmp/script.rhai\"); object");
        assert!(behavior
            .update(&Transform::default(), behavior.defaults(), 0.0, 0.0)
            .is_err());
    }

    #[test]
    fn syntax_and_runtime_diagnostics_keep_source_and_position() {
        let error = Behavior::compile("fn properties() {\n let broken = ;\n}")
            .err()
            .unwrap();
        assert!(
            error.contains("behavior.rhai:") && error.contains("line 2"),
            "{error}"
        );
        let behavior = with_body("object.y = unknown; object");
        let error = behavior
            .update(&Transform::default(), behavior.defaults(), 0.0, 0.0)
            .unwrap_err();
        assert!(
            error.contains("behavior.rhai:") && error.contains("line 2"),
            "{error}"
        );
    }
}
