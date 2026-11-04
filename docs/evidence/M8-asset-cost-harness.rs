use ruster_assets::{AssetStatus, AssetStore, MAX_RESOLVED_BYTES};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const TRIALS: usize = 5;

fn project(triangles: usize, variant: usize, trial: usize) -> (PathBuf, String) {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ruster-asset-cost-{nonce}-{trial}"));
    fs::create_dir(&root).unwrap();
    let mut source = String::from("v 0 0 0\nv 1 0 0\nv 0 1 0\n");
    source.push_str(&"f 1 2 3\n".repeat(triangles));
    if variant != 0 {
        source = source.replacen("v 1 0 0", "v 2 0 0", 1);
    }
    fs::write(root.join("mesh.obj"), &source).unwrap();
    (root, source)
}

fn elapsed<T>(f: impl FnOnce() -> T) -> (Duration, T) {
    let start = Instant::now();
    let value = f();
    (start.elapsed(), value)
}

fn summarize(label: &str, samples: &[Duration]) {
    let mut ns: Vec<u128> = samples.iter().map(Duration::as_nanos).collect();
    ns.sort_unstable();
    println!("{label}: ns={ns:?} median_ns={}", ns[ns.len() / 2]);
}

fn measure(triangles: usize) {
    println!("WORKLOAD triangles={triangles} trials={TRIALS}");
    let mut imports = Vec::new();
    let mut opens = Vec::new();
    let mut unchanged_refreshes = Vec::new();
    let mut changed_refreshes = Vec::new();
    let mut failed_refreshes = Vec::new();
    let mut recovery_refreshes = Vec::new();
    let mut cache_bytes = 0u64;
    let mut source_bytes = 0usize;
    let mut payload_bytes = 0usize;
    let mut revision = 0u64;

    for trial in 0..TRIALS {
        let (root, source) = project(triangles, 0, trial);
        source_bytes = source.len();
        let mut store = AssetStore::open(&root).unwrap();
        let (duration, info) = elapsed(|| store.import("mesh", "mesh.obj").unwrap());
        assert_eq!(info.status, AssetStatus::Ready);
        imports.push(duration);
        let mesh = store.mesh("mesh").unwrap().clone();
        payload_bytes = mesh.vertices.len() * std::mem::size_of_val(&mesh.vertices[0])
            + mesh.indices.len() * std::mem::size_of_val(&mesh.indices[0]);
        assert!(payload_bytes <= MAX_RESOLVED_BYTES);
        let cache = root
            .join(".ruster-cache/meshes")
            .join(format!("v1-{}.json", info.hash.unwrap()));
        cache_bytes = fs::metadata(cache).unwrap().len();

        let (duration, reopened) = elapsed(|| AssetStore::open(&root).unwrap());
        assert_eq!(reopened.mesh("mesh"), Some(&mesh));
        assert_eq!(reopened.revision(), 1);
        opens.push(duration);

        let (duration, _) = elapsed(|| store.refresh("mesh").unwrap());
        assert_eq!(store.revision(), 1);
        unchanged_refreshes.push(duration);

        let changed = source.replacen("v 1 0 0", "v 2 0 0", 1);
        fs::write(root.join("mesh.obj"), changed).unwrap();
        let invalid_open = AssetStore::open(&root).unwrap();
        assert_eq!(invalid_open.inspect()[0].status, AssetStatus::Invalid);
        assert_eq!(invalid_open.mesh("mesh"), Some(&mesh));
        let (duration, refreshed) = elapsed(|| store.refresh("mesh").unwrap());
        assert_eq!(refreshed.status, AssetStatus::Ready);
        assert_eq!(store.revision(), 2);
        changed_refreshes.push(duration);

        fs::write(root.join("mesh.obj"), "v NaN 0 0\n").unwrap();
        let (duration, failed) = elapsed(|| store.refresh("mesh"));
        assert!(failed.is_err());
        assert_eq!(store.revision(), 2);
        assert_eq!(store.mesh("mesh").unwrap().vertices.len(), triangles * 3);
        failed_refreshes.push(duration);

        let repaired = source.replacen("v 1 0 0", "v 3 0 0", 1);
        fs::write(root.join("mesh.obj"), repaired).unwrap();
        let (duration, recovered) = elapsed(|| store.refresh("mesh").unwrap());
        assert_eq!(recovered.status, AssetStatus::Ready);
        assert_eq!(store.revision(), 3);
        recovery_refreshes.push(duration);
        revision = store.revision();
        fs::remove_dir_all(root).unwrap();
    }

    println!(
        "FIXTURE source_bytes={source_bytes} resolved_logical_payload_bytes={payload_bytes} cache_json_bytes={cache_bytes} final_revision={revision}"
    );
    summarize("import_api_after_open", &imports);
    summarize("reopen_api", &opens);
    summarize("refresh_unchanged_api", &unchanged_refreshes);
    summarize("refresh_changed_api", &changed_refreshes);
    summarize("refresh_invalid_failure_api", &failed_refreshes);
    summarize("refresh_recovery_api", &recovery_refreshes);
}

fn main() {
    println!("ruster-assets public API release timing; warmups=0; fixed trials={TRIALS}");
    measure(1_000);
    measure(32_768);
}
