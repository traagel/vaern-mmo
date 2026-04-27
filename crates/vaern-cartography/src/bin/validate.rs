//! `cargo run -p vaern-cartography --bin vaern-validate`
//!
//! Loads the entire world data corpus, runs the cross-file validator,
//! and prints a structured report. Exit 0 if no errors; 1 otherwise.

use std::{path::PathBuf, process::ExitCode};

use vaern_cartography::{load_cartography_style, validate, Severity, WorldBundle};
use vaern_data::{
    load_all_connections, load_all_geography, load_all_landmarks, load_world, load_world_layout,
};

fn world_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/generated/world")
        .canonicalize()
        .expect("world root not found")
}

fn main() -> ExitCode {
    let root = world_root();

    let world = match load_world(&root) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("load_world failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let landmarks = load_all_landmarks(&root).unwrap_or_else(|e| {
        eprintln!("load_all_landmarks failed: {e}");
        std::process::exit(1);
    });
    let geography = load_all_geography(&root).unwrap_or_else(|e| {
        eprintln!("load_all_geography failed: {e}");
        std::process::exit(1);
    });
    let connections = load_all_connections(&root).unwrap_or_else(|e| {
        eprintln!("load_all_connections failed: {e}");
        std::process::exit(1);
    });
    let layout = load_world_layout(&root).unwrap_or_else(|e| {
        eprintln!("load_world_layout failed: {e}");
        std::process::exit(1);
    });
    let (style, glyphs) = load_cartography_style(root.join("style")).unwrap_or_else(|e| {
        eprintln!("load_cartography_style failed: {e}");
        std::process::exit(1);
    });
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

    let mut errors = 0_usize;
    let mut warnings = 0_usize;
    for issue in &report.issues {
        let tag = match issue.severity {
            Severity::Error => {
                errors += 1;
                "ERROR"
            }
            Severity::Warning => {
                warnings += 1;
                "warn "
            }
        };
        println!("[{tag}] {} — {}", issue.kind, issue.message);
    }

    println!(
        "\n{} zones, {} hubs, {} landmarks, {} geography files, {} connection edges, {} placements",
        world.zones.len(),
        world.hubs.len(),
        landmarks.by_id.len(),
        geography.by_zone.len(),
        connections.all_edges().count(),
        layout.zone_placements.len()
    );
    println!("validation: {} errors, {} warnings", errors, warnings);

    if errors > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
