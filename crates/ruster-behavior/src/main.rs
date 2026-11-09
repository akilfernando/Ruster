//! Optional headless behavior consumer; project files are never modified.
use ruster_behavior::{Diagnostic, Engine, Limits};
use ruster_scene::Session;
use serde_json::{json, Value};

const HELP: &str = "Ruster behavior — provisional Lua5.4 runtime\n\nUsage:\n  ruster-behavior check SCENE.json\n  ruster-behavior run SCENE.json TICKS [DT]\n\nTICKS:0..10000. DT:finite seconds in(0,1], default1/60.\nCheck performs versioned initialization and dt0 update validation. Run resets,\nthen completes atomic fixed steps. Disabled bindings are skipped. Both commands\nread supported scenes through Scene14 and emit JSON; authored files are never changed.\nErrors include source version/binding/object context where available.";

enum Failure {
    Input(String),
    Behavior(Diagnostic),
}
impl Failure {
    fn json(self) -> Value {
        match self {
            Self::Input(message) => {
                json!({"error":{"code":"input","phase":"input","message":message}})
            }
            Self::Behavior(error) => json!({"error":error}),
        }
    }
}
fn run(args: &[String]) -> Result<Option<Value>, Failure> {
    if matches!(args, [value] if matches!(value.as_str(), "--help"|"-h"|"help")) {
        println!("{HELP}");
        return Ok(None);
    }
    let input = |message: &str| Failure::Input(format!("{message}\nUse --help for the workflow."));
    let (path, ticks, dt) = match args {
        [mode, path] if mode == "check" => (path, 0, 1.0 / 60.0),
        [mode, path, ticks, rest @ ..] if mode == "run" && rest.len() <= 1 => {
            let ticks: u64 = ticks.parse().map_err(|_| input("TICKS must be0..10000."))?;
            if ticks > 10_000 {
                return Err(input("TICKS must be0..10000."));
            }
            let dt = rest
                .first()
                .map(|value| value.parse::<f64>())
                .transpose()
                .map_err(|_| input("DT must be a finite number in(0,1]."))?
                .unwrap_or(1.0 / 60.0);
            (path, ticks, dt)
        }
        _ => {
            return Err(input(
                "Expected check SCENE.json or run SCENE.json TICKS [DT].",
            ))
        }
    };
    let mut engine = Engine::new(Limits {
        fixed_dt: dt,
        ..Limits::default()
    })
    .map_err(Failure::Behavior)?;
    let session = Session::open(path).map_err(|e| input(&e.to_string()))?;
    let mut runtime = engine
        .reset(session.scene(), session.authoring())
        .map_err(Failure::Behavior)?;
    for _ in 0..ticks {
        engine.step(&mut runtime).map_err(Failure::Behavior)?;
    }
    Ok(Some(
        json!({"scene_id":session.scene().id,"authored_revision":session.scene().revision.to_string(),"tick":runtime.tick().to_string(),"commit_revision":runtime.commit_revision().to_string(),"last_step_commit_revision":runtime.last_step_commit_revision().map(|value|value.to_string()),"fixed_dt":dt,"enabled_bindings":session.authoring().bindings.values().filter(|binding|binding.enabled).count(),"fingerprint":runtime.scene().fingerprint(),"scene":runtime.scene(),"runtime_state":runtime.state_report()}),
    ))
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(Some(value)) => println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("validated runtime values serialize")
        ),
        Ok(None) => {}
        Err(error) => {
            eprintln!("{}", error.json());
            std::process::exit(1);
        }
    }
}
