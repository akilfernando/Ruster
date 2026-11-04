use fs2::FileExt;
use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use ruster_core::{Config, Scenario, Simulation};

const HELP: &str = "Ruster logistics demo

Usage: ruster-cli [--ticks N] [--production N] [--transfer N]
       ruster-cli create FILE [--id ID] [--initial-stock N] [--production N] [--transfer N]
       ruster-cli inspect FILE
       ruster-cli edit FILE --revision N [--initial-stock N] [--production N] [--transfer N]
       ruster-cli undo FILE --revision N
       ruster-cli redo FILE --revision N
       ruster-cli save FILE OUTPUT
       ruster-cli run FILE [--ticks N]
       ruster-cli replay FILE [--ticks N]
       ruster-cli reset FILE
       ruster-cli --help

  --ticks N       Run N ticks (positive whole number; default: 10)
  --production N  Items produced per tick (zero allowed; default: 3)
  --transfer N    Transport capacity per tick (positive; default: 2)
  -h, --help      Show this help

Each tick produces stock, then transfers up to the transport capacity.
The simulation advances explicitly without waiting for real time.
Edits/undo/redo save atomically and require the revision from inspect.
Save copies a project to a new file. Create/save refuse existing destinations.
Run/replay start fresh; reset prints initial runtime without changing authored data.";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Help,
    Run {
        ticks: u64,
        config: Config,
    },
    Project {
        action: String,
        path: PathBuf,
        output: Option<PathBuf>,
        options: std::collections::BTreeMap<String, String>,
    },
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut args: Vec<_> = args.into_iter().collect();
    let project = args
        .first()
        .and_then(|a| a.to_str())
        .filter(|a| {
            matches!(
                *a,
                "create"
                    | "inspect"
                    | "edit"
                    | "undo"
                    | "redo"
                    | "save"
                    | "run"
                    | "replay"
                    | "reset"
            )
        })
        .map(str::to_owned);
    if let Some(action) = project {
        args.remove(0);
        let mut args = args.into_iter();
        let path = PathBuf::from(
            args.next()
                .ok_or_else(|| format!("{action} needs a scenario FILE"))?,
        );
        let output = if action == "save" {
            Some(PathBuf::from(
                args.next().ok_or("save needs an OUTPUT file")?,
            ))
        } else {
            None
        };
        let allowed: &[&str] = match action.as_str() {
            "create" => &["--id", "--initial-stock", "--production", "--transfer"],
            "edit" => &[
                "--revision",
                "--initial-stock",
                "--production",
                "--transfer",
            ],
            "undo" | "redo" => &["--revision"],
            "run" | "replay" => &["--ticks"],
            _ => &[],
        };
        let mut options = std::collections::BTreeMap::new();
        while let Some(key) = args.next() {
            let key = key.into_string().map_err(|_| "option must be UTF-8")?;
            if !allowed.contains(&key.as_str()) {
                return Err(format!("unknown argument for {action}: {key}"));
            }
            let value = args
                .next()
                .ok_or_else(|| format!("{key} needs a value"))?
                .into_string()
                .map_err(|_| "option value must be UTF-8")?;
            if key != "--id" {
                let number = whole_number(&key, &value)?;
                if number == 0 && matches!(key.as_str(), "--transfer" | "--ticks") {
                    return Err(format!("{key} must be greater than zero"));
                }
            }
            if options.insert(key.clone(), value).is_some() {
                return Err(format!("{key} can only be supplied once"));
            }
        }
        if matches!(action.as_str(), "edit" | "undo" | "redo")
            && !options.contains_key("--revision")
        {
            return Err(format!(
                "{action} requires --revision N; inspect the project first"
            ));
        }
        if action == "edit" && options.len() == 1 {
            return Err("edit needs at least one configuration option".into());
        }
        return Ok(Command::Project {
            action,
            path,
            output,
            options,
        });
    }
    parse_demo(args)
}

fn whole_number(option: &str, value: &str) -> Result<u64, String> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!("{option} needs a whole number"));
    }
    value
        .parse()
        .map_err(|_| format!("{option} is too large (maximum: 18446744073709551615)"))
}

fn parse_demo(args: impl IntoIterator<Item = OsString>) -> Result<Command, String> {
    let mut args = args.into_iter();
    let mut ticks = None;
    let mut help = false;
    let mut production = None;
    let mut transfer = None;
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => help = true,
            Some(option @ ("--ticks" | "--production" | "--transfer")) => {
                let slot = match option {
                    "--production" => &mut production,
                    "--transfer" => &mut transfer,
                    _ => &mut ticks,
                };
                if slot.is_some() {
                    return Err(format!("{option} can only be supplied once"));
                }
                let value = args
                    .next()
                    .ok_or_else(|| format!("{option} needs a whole number"))?;
                let value = value
                    .to_str()
                    .ok_or_else(|| format!("{option} needs a whole number"))?;
                if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                    return Err(format!("{option} needs a whole number, such as 10"));
                }
                let value = value.parse::<u64>().map_err(|_| {
                    format!("{option} is too large (maximum: 18446744073709551615)")
                })?;
                if value == 0 && option != "--production" {
                    return Err(format!("{option} must be greater than zero"));
                }
                *slot = Some(value);
            }
            _ => return Err(format!("unknown argument: {}", arg.to_string_lossy())),
        }
    }
    Ok(if help {
        Command::Help
    } else {
        Command::Run {
            ticks: ticks.unwrap_or(10),
            config: Config {
                production_per_tick: production.unwrap_or(3),
                transfer_per_tick: transfer.unwrap_or(2),
                ..Config::default()
            },
        }
    })
}

fn run(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    let (ticks, config) = match command {
        Command::Help => {
            println!("{HELP}");
            return Ok(());
        }
        Command::Run { ticks, config } => (ticks, config),
        Command::Project {
            action,
            path,
            output,
            options,
        } => return run_project(&action, &path, output.as_deref(), &options),
    };
    println!(
        "Production per tick: {} | Transfer capacity: {}",
        config.production_per_tick, config.transfer_per_tick
    );
    let mut simulation = Simulation::new(config)?;
    for _ in 0..ticks {
        simulation.step()?;
    }
    let state = simulation.state();
    println!("Ruster logistics demo: {} ticks complete", state.tick);
    println!(
        "Produced: {} | Stock: {} | Delivered: {}",
        state.produced, state.stock, state.delivered
    );
    Ok(())
}

// All cooperating CLI writers use a stable sibling lock, because the scenario
// inode itself is replaced at save. Resolve parents and reject file symlinks so
// aliases do not accidentally use different locks for the same destination.
fn lock_project(path: &Path) -> Result<(PathBuf, File), Box<dyn std::error::Error>> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = parent.canonicalize()?;
    let name = path.file_name().ok_or("scenario path needs a file name")?;
    let target = parent.join(name);
    if target
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("scenario destination must not be a symlink".into());
    }
    let mut lock_name = OsString::from(".");
    lock_name.push(name);
    lock_name.push(".ruster-lock");
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(parent.join(lock_name))?;
    lock.lock_exclusive()?;
    Ok((target, lock))
}

fn inspect(scenario: &Scenario) {
    let config = scenario.config();
    println!(
        "Scenario: {} | Revision: {}",
        scenario.id(),
        scenario.revision()
    );
    println!(
        "Initial stock: {} | Production per tick: {} | Transfer capacity: {}",
        config.initial_stock, config.production_per_tick, config.transfer_per_tick
    );
    println!(
        "Authored undo: {} | Redo: {}",
        scenario.undo_len(),
        scenario.redo_len()
    );
}

fn run_project(
    action: &str,
    path: &Path,
    output: Option<&Path>,
    options: &std::collections::BTreeMap<String, String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let value = |key: &str, fallback: u64| -> u64 {
        options
            .get(key)
            .map(|v| v.parse().expect("validated numeric option"))
            .unwrap_or(fallback)
    };
    if action == "create" {
        let (path, _lock) = lock_project(path)?;
        let scenario = Scenario::new(
            options
                .get("--id")
                .map(String::as_str)
                .unwrap_or("logistics-demo"),
            Config {
                initial_stock: value("--initial-stock", 0),
                production_per_tick: value("--production", 3),
                transfer_per_tick: value("--transfer", 2),
            },
        )?;
        scenario.save_new(path)?;
        inspect(&scenario);
        return Ok(());
    }
    if matches!(action, "edit" | "undo" | "redo") {
        let (path, _lock) = lock_project(path)?;
        let mut scenario = Scenario::load(&path)?;
        let revision = value("--revision", 0);
        match action {
            "edit" => {
                let current = scenario.config();
                scenario.edit(
                    revision,
                    Config {
                        initial_stock: value("--initial-stock", current.initial_stock),
                        production_per_tick: value("--production", current.production_per_tick),
                        transfer_per_tick: value("--transfer", current.transfer_per_tick),
                    },
                )?;
            }
            "undo" => {
                scenario.undo(revision)?;
            }
            "redo" => {
                scenario.redo(revision)?;
            }
            _ => unreachable!(),
        }
        scenario.save(path)?;
        inspect(&scenario);
        return Ok(());
    }
    let scenario = Scenario::load(path)?;
    match action {
        "inspect" => inspect(&scenario),
        "save" => {
            let (output, _lock) = lock_project(output.ok_or("missing output path")?)?;
            scenario.save_new(output)?;
            inspect(&scenario);
        }
        "run" | "replay" | "reset" => {
            let ticks = if action == "reset" {
                0
            } else {
                value("--ticks", 10)
            };
            let state = scenario.replay(ticks)?;
            println!(
                "Scenario: {} | Revision: {}",
                scenario.id(),
                scenario.revision()
            );
            println!("Ruster logistics demo: {} ticks complete", state.tick);
            println!(
                "Produced: {} | Stock: {} | Delivered: {}",
                state.produced, state.stock, state.delivered
            );
        }
        _ => unreachable!(),
    }
    Ok(())
}

fn main() -> ExitCode {
    let command = match parse(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("Error: {error}\nRun ruster-cli --help for usage.");
            return ExitCode::from(2);
        }
    };
    match run(command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strings(args: &[&str]) -> Result<Command, String> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn parses_default_help_and_explicit_ticks() {
        assert_eq!(
            parse_strings(&[]),
            Ok(Command::Run {
                ticks: 10,
                config: Config::default()
            })
        );
        assert_eq!(parse_strings(&["--help"]), Ok(Command::Help));
        assert_eq!(parse_strings(&["-h"]), Ok(Command::Help));
        assert_eq!(
            parse_strings(&["--ticks", "23"]),
            Ok(Command::Run {
                ticks: 23,
                config: Config::default()
            })
        );
    }

    #[test]
    fn rejects_invalid_input() {
        for args in [
            vec!["--ticks"],
            vec!["--ticks", "0"],
            vec!["--ticks", "-1"],
            vec!["--ticks", "+1"],
            vec!["--ticks", "1.5"],
            vec!["--ticks", ""],
            vec!["--ticks", "18446744073709551616"],
            vec!["--unknown"],
            vec!["--ticks", "1", "--ticks", "2"],
            vec!["12"],
            vec!["--transfer", "0"],
            vec!["--production", "-1"],
        ] {
            assert!(parse_strings(&args).is_err(), "accepted {args:?}");
        }
    }
}
