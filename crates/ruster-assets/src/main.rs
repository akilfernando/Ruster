use ruster_assets::AssetStore;
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let usage = "usage: ruster-assets import ROOT ID SOURCE.obj | inspect ROOT | refresh ROOT ID | rename ROOT ID SOURCE.obj";
    if args.len() < 2 {
        return Err(usage.into());
    }
    let mut store = AssetStore::open(&args[1])?;
    let output = match (args[0].as_str(), args.len()) {
        ("inspect", 2) => serde_json::json!({"revision":store.revision(),"assets":store.inspect()}),
        ("import", 4) => serde_json::to_value(store.import(&args[2], &args[3])?)?,
        ("refresh", 3) => serde_json::to_value(store.refresh(&args[2])?)?,
        ("rename", 4) => serde_json::to_value(store.rename(&args[2], &args[3])?)?,
        _ => return Err(usage.into()),
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
