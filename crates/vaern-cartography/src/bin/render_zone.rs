//! `cargo run -p vaern-cartography --bin vaern-render-zone -- <zone_id>`
//!
//! Loads the world, picks the zone by id, runs the validator (refusing
//! to render if errors), and writes `target/maps/<zone_id>.svg`.

use std::{fs, path::PathBuf, process::ExitCode};

use vaern_cartography::{load_cartography_style, render_zone_svg, validate, RenderOptions, Severity, WorldBundle};
use vaern_data::{
    load_all_connections, load_all_geography, load_all_landmarks, load_world, load_world_layout,
};

fn world_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/generated/world")
        .canonicalize()
        .expect("world root not found")
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root not found")
}

fn main() -> ExitCode {
    let zone_id = match std::env::args().nth(1) {
        Some(s) => s,
        None => {
            eprintln!("usage: vaern-render-zone <zone_id>");
            return ExitCode::FAILURE;
        }
    };

    let root = world_root();
    let world = load_world(&root).expect("load_world");
    let landmarks = load_all_landmarks(&root).expect("load_all_landmarks");
    let geography = load_all_geography(&root).expect("load_all_geography");
    let connections = load_all_connections(&root).expect("load_all_connections");
    let layout = load_world_layout(&root).expect("load_world_layout");
    let (style, glyphs) =
        load_cartography_style(root.join("style")).expect("load_cartography_style");
    let glyph_names: Vec<String> = glyphs.by_name.keys().cloned().collect();

    let bundle = WorldBundle {
        world: &world,
        landmarks: &landmarks,
        geography: &geography,
        connections: &connections,
        layout: &layout,
        style: &style,
        glyph_names: &glyph_names,
    };
    let report = validate(&bundle);
    let zone_errors: Vec<_> = report
        .issues
        .iter()
        .filter(|i| {
            i.severity == Severity::Error && i.message.contains(&format!("{:?}", zone_id))
        })
        .collect();
    if !zone_errors.is_empty() {
        eprintln!("validator errors for zone {zone_id}:");
        for e in &zone_errors {
            eprintln!("  [{}] {}", e.kind, e.message);
        }
        return ExitCode::FAILURE;
    }

    let zone = match world.zone(&zone_id) {
        Some(z) => z,
        None => {
            eprintln!("unknown zone: {zone_id}");
            return ExitCode::FAILURE;
        }
    };

    let opts = RenderOptions::default();
    let svg = render_zone_svg(
        zone,
        &world,
        &landmarks,
        geography.get(&zone_id),
        &connections,
        &style,
        &glyphs,
        &layout,
        &opts,
    );

    let out_dir = workspace_root().join("target").join("maps");
    fs::create_dir_all(&out_dir).expect("mkdir target/maps");
    let out_path = out_dir.join(format!("{zone_id}.svg"));
    fs::write(&out_path, svg).expect("write svg");
    println!("wrote {}", out_path.display());

    ExitCode::SUCCESS
}
