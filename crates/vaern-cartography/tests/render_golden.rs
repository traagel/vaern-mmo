//! Determinism: same inputs → same SVG bytes. Required for the
//! schema-driven generator pipeline to be reproducible.

use std::path::PathBuf;

use vaern_cartography::{
    load_cartography_style, render_world_svg, render_zone_svg, RenderOptions,
};
use vaern_data::{
    load_all_connections, load_all_geography, load_all_landmarks, load_world, load_world_layout,
};

fn world_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../src/generated/world")
        .canonicalize()
        .expect("world root")
}

#[test]
fn dalewatch_zone_render_is_byte_deterministic() {
    let root = world_root();
    let world = load_world(&root).unwrap();
    let landmarks = load_all_landmarks(&root).unwrap();
    let geography = load_all_geography(&root).unwrap();
    let connections = load_all_connections(&root).unwrap();
    let layout = load_world_layout(&root).unwrap();
    let (style, glyphs) = load_cartography_style(root.join("style")).unwrap();

    let zone = world.zone("dalewatch_marches").expect("dalewatch zone");
    let opts = RenderOptions::default();

    let a = render_zone_svg(
        zone,
        &world,
        &landmarks,
        geography.get("dalewatch_marches"),
        &connections,
        &style,
        &glyphs,
        &layout,
        &opts,
    );
    let b = render_zone_svg(
        zone,
        &world,
        &landmarks,
        geography.get("dalewatch_marches"),
        &connections,
        &style,
        &glyphs,
        &layout,
        &opts,
    );
    assert_eq!(
        a, b,
        "two consecutive renders of dalewatch_marches must be byte-identical"
    );
    assert!(
        a.contains("Dalewatch"),
        "rendered SVG must contain the zone title"
    );
    // Hillshade overlay should be present and follow the biome regions
    // (which produce the relief). With include_hillshade=true (default)
    // we expect the hillshade group + at least one shadow rect.
    assert!(
        a.contains("id=\"hillshade\""),
        "rendered SVG should carry the hillshade overlay"
    );
    let rect_count = a.matches("<rect").count();
    assert!(
        rect_count > 100,
        "hillshade should emit many shadow rects, got {rect_count}"
    );
}

#[test]
fn dalewatch_zone_render_without_hillshade_is_smaller_and_byte_deterministic() {
    let root = world_root();
    let world = load_world(&root).unwrap();
    let landmarks = load_all_landmarks(&root).unwrap();
    let geography = load_all_geography(&root).unwrap();
    let connections = load_all_connections(&root).unwrap();
    let layout = load_world_layout(&root).unwrap();
    let (style, glyphs) = load_cartography_style(root.join("style")).unwrap();

    let zone = world.zone("dalewatch_marches").expect("dalewatch zone");
    let opts = RenderOptions {
        include_hillshade: false,
        ..Default::default()
    };

    let a = render_zone_svg(
        zone,
        &world,
        &landmarks,
        geography.get("dalewatch_marches"),
        &connections,
        &style,
        &glyphs,
        &layout,
        &opts,
    );
    let b = render_zone_svg(
        zone,
        &world,
        &landmarks,
        geography.get("dalewatch_marches"),
        &connections,
        &style,
        &glyphs,
        &layout,
        &opts,
    );
    assert_eq!(a, b, "no-hillshade renders must also be byte-identical");
    assert!(!a.contains("id=\"hillshade\""));
}

#[test]
fn world_render_is_byte_deterministic() {
    let root = world_root();
    let world = load_world(&root).unwrap();
    let connections = load_all_connections(&root).unwrap();
    let layout = load_world_layout(&root).unwrap();
    let (style, _glyphs) = load_cartography_style(root.join("style")).unwrap();

    let opts = RenderOptions {
        canvas_width: 2400,
        canvas_height: 2400,
        ..Default::default()
    };

    let a = render_world_svg(&world, &layout, &connections, &style, &opts);
    let b = render_world_svg(&world, &layout, &connections, &style, &opts);
    assert_eq!(
        a, b,
        "two consecutive renders of world.svg must be byte-identical"
    );
    assert!(
        a.contains("World of Vaern"),
        "rendered world SVG must contain the title"
    );
}

#[test]
fn validator_runs_clean_on_committed_data() {
    use vaern_cartography::{validate, Severity, WorldBundle};
    let root = world_root();
    let world = load_world(&root).unwrap();
    let landmarks = load_all_landmarks(&root).unwrap();
    let geography = load_all_geography(&root).unwrap();
    let connections = load_all_connections(&root).unwrap();
    let layout = load_world_layout(&root).unwrap();
    let (style, glyphs) = load_cartography_style(root.join("style")).unwrap();
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
    let errors: Vec<_> = report
        .issues
        .iter()
        .filter(|i| i.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "validator must run clean against the committed corpus, got: {:?}",
        errors
            .iter()
            .map(|e| format!("[{}] {}", e.kind, e.message))
            .collect::<Vec<_>>()
    );
}
