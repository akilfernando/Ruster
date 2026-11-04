use serde::Deserialize;
use std::error::Error;
use std::io::Read;
use std::path::Path;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TransformEdit {
    object_id: String,
    transform: ruster_scene::Transform,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DuplicateEdit {
    object_id: String,
    new_object_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateBehavior {
    source: ruster_scene::BehaviorSource,
    binding: ruster_scene::BehaviorBinding,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, Box<dyn Error>> {
    if !path.symlink_metadata()?.is_file() {
        return Err("JSON input must be a regular file.".into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(ruster_scene::MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > ruster_scene::MAX_FILE_BYTES {
        return Err("JSON input exceeds1MiB.".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

use ruster_scene::{Object, Preview, Scene, Session, FORMAT_VERSION};

fn report_revision(session: &Session, revision: u64) {
    println!("revision {revision}");
    let removed = session.history_evictions();
    if removed > 0 {
        eprintln!("Removed {removed} oldest Undo steps to keep this edit saveable.");
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let usage = "usage: ruster-scene demo FILE | new FILE SCENE_ID | create FILE REV ID [NAME] | rename FILE REV ID [NAME] | duplicate FILE REV SOURCE NEW_ID [NAME] | delete FILE REV ID | inspect FILE | transform FILE EXPECTED_REV OBJECT X Y Z [RX RY RZ SX SY SZ] | mesh FILE EXPECTED_REV OBJECT ASSET_ID|none | transforms|duplicates|deletes FILE EXPECTED_REV BATCH.json | undo FILE EXPECTED_REV | redo FILE EXPECTED_REV | run FILE TICKS (0..10000)\nAuthoring: behavior-create FILE REV SOURCE_ID BINDING_ID JSON_FILE | sources|bindings|diagnostics FILE [OFFSET] | source|binding FILE ID | source-set|binding-set FILE REV ID JSON_FILE | source-remove|binding-remove FILE REV ID\nSources are embedded text. JSON files describe BehaviorSource/BehaviorBinding; unfinished code and unknown languages remain saveable. List commands page16 summaries; source/binding returns the complete requested value.";
    let Some(command) = args.first() else {
        return Err(usage.into());
    };
    match (command.as_str(), args.len()) {
        ("help" | "--help" | "-h", 1) => println!("{usage}"),
        ("behavior-create", 6) => {
            let path = Path::new(&args[1]);
            let mut session = Session::open(path)?;
            let input: CreateBehavior = read_json(Path::new(&args[5]))?;
            let revision = session.create_behavior(
                args[2].parse()?,
                &args[3],
                input.source,
                &args[4],
                input.binding,
            )?;
            session.save(path)?;
            report_revision(&session, revision);
        }
        ("sources" | "bindings" | "diagnostics", 2 | 3) => {
            let session = Session::open(&args[1])?;
            let offset = args.get(2).map(|s| s.parse()).transpose()?.unwrap_or(0);
            let page = match command.as_str() {
                "sources" => serde_json::to_value(session.inspect_sources(offset, 16))?,
                "bindings" => serde_json::to_value(session.inspect_bindings(offset, 16))?,
                _ => serde_json::to_value(session.authoring_diagnostics(offset, 16))?,
            };
            println!("{}", serde_json::to_string_pretty(&page)?);
        }
        ("source" | "binding", 3) => {
            let session = Session::open(&args[1])?;
            let value = if command == "source" {
                serde_json::to_value(session.source(&args[2]).ok_or("Source does not exist.")?)?
            } else {
                serde_json::to_value(session.binding(&args[2]).ok_or("Binding does not exist.")?)?
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        ("source-set" | "binding-set", 5) | ("source-remove" | "binding-remove", 4) => {
            let path = Path::new(&args[1]);
            let mut session = Session::open(path)?;
            let expected = args[2].parse()?;
            let revision = match command.as_str() {
                "source-set" => {
                    session.set_source(expected, &args[3], Some(read_json(Path::new(&args[4]))?))?
                }
                "binding-set" => session.set_binding(
                    expected,
                    &args[3],
                    Some(read_json(Path::new(&args[4]))?),
                )?,
                "source-remove" => session.set_source(expected, &args[3], None)?,
                _ => session.set_binding(expected, &args[3], None)?,
            };
            session.save(path)?;
            report_revision(&session, revision);
        }
        ("demo", 2) => {
            let mut session = Session::new(Scene::demo())?;
            session.save(&args[1])?;
            println!("created {} revision 0", args[1]);
        }
        ("new" | "empty", 3) => {
            let mut session = Session::new(Scene::empty(&args[2]))?;
            session.save(&args[1])?;
            println!("created empty {} revision 0", args[1]);
        }
        ("create" | "rename", 4 | 5) | ("duplicate", 5 | 6) | ("delete", 4) => {
            let path = Path::new(&args[1]);
            let mut session = Session::open(path)?;
            let expected = args[2].parse()?;
            let revision = match command.as_str() {
                "create" => {
                    session.create(expected, Object::cube(&args[3], args.get(4).cloned()))?
                }
                "rename" => session.rename(expected, &args[3], args.get(4).cloned())?,
                "duplicate" => {
                    session.duplicate(expected, &args[3], &args[4], args.get(5).cloned())?
                }
                "delete" => session.delete(expected, &args[3])?,
                _ => unreachable!(),
            };
            session.save(path)?;
            report_revision(&session, revision);
        }
        ("inspect", 2) => {
            let session = Session::open(&args[1])?;
            println!("{}", serde_json::to_string_pretty(session.scene())?);
            println!("undo={} redo={}", session.undo_len(), session.redo_len());
            println!("loaded_format_version={}", session.loaded_format_version());
            println!(
                "sources={} bindings={}",
                session.authoring().sources.len(),
                session.authoring().bindings.len()
            );
            if session.loaded_format_version() != FORMAT_VERSION {
                println!("older scene opened; next save upgrades to v{FORMAT_VERSION}, preserving history");
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
            report_revision(&session, revision);
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
            report_revision(&session, revision);
        }
        ("transforms" | "duplicates" | "deletes", 4) => {
            let path = Path::new(&args[1]);
            let mut session = Session::open(path)?;
            let expected = args[2].parse()?;
            let input = Path::new(&args[3]);
            let revision = match command.as_str() {
                "transforms" => {
                    let edits: Vec<TransformEdit> = read_json(input)?;
                    let edits: Vec<_> = edits
                        .into_iter()
                        .map(|e| (e.object_id, e.transform))
                        .collect();
                    session.edit_transforms(expected, &edits)?
                }
                "duplicates" => {
                    let edits: Vec<DuplicateEdit> = read_json(input)?;
                    let edits: Vec<_> = edits
                        .into_iter()
                        .map(|e| (e.object_id, e.new_object_id))
                        .collect();
                    session.duplicate_many(expected, &edits)?
                }
                _ => {
                    let ids: Vec<String> = read_json(input)?;
                    session.delete_many(expected, &ids)?
                }
            };
            session.save(path)?;
            report_revision(&session, revision);
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
            report_revision(&session, revision);
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
