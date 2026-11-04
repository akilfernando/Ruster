use std::error::Error;
use std::path::Path;

use ruster_scene::{Preview, Scene, Session};

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: ruster-scene demo FILE | inspect FILE | transform FILE EXPECTED_REV OBJECT X Y Z [RX RY RZ SX SY SZ] | mesh FILE EXPECTED_REV OBJECT ASSET_ID|none | undo FILE EXPECTED_REV | redo FILE EXPECTED_REV | run FILE TICKS (0..10000)";
    let Some(command) = args.first() else {
        return Err(usage.into());
    };
    match (command.as_str(), args.len()) {
        ("demo", 2) => {
            let mut session = Session::new(Scene::demo())?;
            session.save(&args[1])?;
            println!("created {} revision 0", args[1]);
        }
        ("inspect", 2) => {
            let session = Session::open(&args[1])?;
            println!("{}", serde_json::to_string_pretty(session.scene())?);
            println!("undo={} redo={}", session.undo_len(), session.redo_len());
            println!("loaded_format_version={}", session.loaded_format_version());
            if session.loaded_format_version() == 1 {
                println!("v1 opened; next save upgrades to v2, preserving transform history");
            }
        }
        ("transform", 7 | 13) => {
            let path = Path::new(&args[1]);
            let mut session = Session::open(path)?;
            let expected = args[2].parse()?;
            let object = session
                .scene()
                .objects
                .iter()
                .find(|o| o.id == args[3])
                .ok_or_else(|| format!("object {} does not exist", args[3]))?;
            let mut transform = object.transform.clone();
            transform.translation = [args[4].parse()?, args[5].parse()?, args[6].parse()?];
            if args.len() == 13 {
                transform.rotation = [args[7].parse()?, args[8].parse()?, args[9].parse()?];
                transform.scale = [args[10].parse()?, args[11].parse()?, args[12].parse()?];
            }
            let revision = session.edit(expected, &args[3], &transform)?;
            session.save(path)?;
            println!("revision {revision}");
        }
        ("mesh", 5) => {
            let path = Path::new(&args[1]);
            let mut session = Session::open(path)?;
            let mesh = if args[4] == "none" {
                None
            } else {
                Some(args[4].clone())
            };
            let revision = session.set_mesh(args[2].parse()?, &args[3], mesh)?;
            session.save(path)?;
            println!("revision {revision}");
        }
        ("undo" | "redo", 3) => {
            let path = Path::new(&args[1]);
            let mut session = Session::open(path)?;
            let expected = args[2].parse()?;
            let revision = if command == "undo" {
                session.undo(expected)?
            } else {
                session.redo(expected)?
            };
            session.save(path)?;
            println!("revision {revision}");
        }
        ("run", 3) => {
            let ticks: u64 = args[2].parse()?;
            if ticks > 10_000 {
                return Err("preview is bounded to 10000 ticks".into());
            }
            let scene = Scene::load(&args[1])?;
            let mut preview = Preview::new(&scene);
            for _ in 0..ticks {
                preview.step()?;
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "scene_id": preview.scene.id,
                    "revision": preview.scene.revision,
                    "tick": preview.tick,
                    "ticks": preview.tick,
                    "object_count": preview.scene.objects.len(),
                    "fingerprint": preview.scene.fingerprint(),
                }))?
            );
        }
        _ => return Err(usage.into()),
    }
    Ok(())
}
