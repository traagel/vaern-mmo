//! `cargo run -p vaern-cartography --bin vaern-render-world`
//!
//! Loads the world, runs the validator, and writes
//! `target/maps/world.svg`.

use std::{fs, path::PathBuf, process::ExitCode};

use vaern_cartography::{
    load_cartography_style, render_world_svg, validate, RenderOptions, Severity, WorldBundle,
};
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
    let layout_errors: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error && i.kind.starts_with("layout_"))
        .collect();
    if !layout_errors.is_empty() {
        eprintln!("validator errors block world render:");
        for e in &layout_errors {
            eprintln!("  [{}] {}", e.kind, e.message);
        }
        return ExitCode::FAILURE;
    }

    let opts = RenderOptions {
        canvas_width: 2400,
        canvas_height: 2400,
        ..Default::default()
    };
    let svg = render_world_svg(&world, &layout, &connections, &style, &opts);

    let out_dir = workspace_root().join("target").join("maps");
    fs::create_dir_all(&out_dir).expect("mkdir target/maps");
    let out_path = out_dir.join("world.svg");
    fs::write(&out_path, svg).expect("write svg");
    println!("wrote {}", out_path.display());

    ExitCode::SUCCESS
}
