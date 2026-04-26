//! Left side panel — Poly Haven catalog browser, grouped by category.
//!
//! V1: clicking a slug only logs to the console; place-mode (V2) will
//! read this resource to know which slug to spawn.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use vaern_assets::{PolyHavenCatalog, PolyHavenCategory};

use crate::ui::console::ConsoleLog;

/// User-selected slug from the palette. Read by Place mode to know
/// which prop to spawn at the cursor. `None` = nothing selected.
#[derive(Resource, Debug, Default, Clone)]
pub struct SelectedPaletteSlug(pub Option<String>);

impl SelectedPaletteSlug {
    pub fn slug(&self) -> Option<&str> {
        self.0.as_deref()
    }
}

/// Order of categories in the panel.
const PALETTE_GROUPS: &[(PolyHavenCategory, &str)] = &[
    (PolyHavenCategory::HubProp, "Hub Props"),
    (PolyHavenCategory::Tree, "Trees"),
    (PolyHavenCategory::Rock, "Rocks"),
    (PolyHavenCategory::DeadWood, "Dead Wood"),
    (PolyHavenCategory::Shrub, "Shrubs"),
    (PolyHavenCategory::GroundCover, "Ground Cover"),
    (PolyHavenCategory::WeaponRackDressing, "Weapon Rack"),
];

/// Draw the palette panel. Optional `PolyHavenCatalog` because the
/// editor binary inserts it directly; if missing, the panel renders a
/// "catalog not loaded" notice instead of crashing.
pub fn draw_palette(
    mut egui: EguiContexts,
    catalog: Option<Res<PolyHavenCatalog>>,
    mut selected: ResMut<SelectedPaletteSlug>,
    mut log: ResMut<ConsoleLog>,
) {
    let Ok(egui_ctx) = egui.ctx_mut() else {
        return;
    };

    egui::SidePanel::left("editor_palette")
        .default_width(220.0)
        .show(egui_ctx, |ui| {
            ui.heading("Palette");
            ui.separator();

            let Some(catalog) = catalog else {
                ui.label("Poly Haven catalog not loaded.");
                ui.label("Insert PolyHavenCatalog::new() before EditorPlugin.");
                return;
            };

            egui::ScrollArea::vertical().show(ui, |ui| {
                for (category, label) in PALETTE_GROUPS {
                    ui.collapsing(*label, |ui| {
                        for entry in catalog.by_category(*category) {
                            let is_selected = selected.0.as_deref() == Some(entry.slug.as_str());
                            if ui
                                .selectable_label(is_selected, &entry.label)
                                .clicked()
                            {
                                selected.0 = Some(entry.slug.clone());
                                log.push(format!("palette: selected {}", entry.slug));
                            }
                        }
                    });
                }
            });

            ui.separator();
            match selected.0.as_deref() {
                Some(slug) => ui.label(format!("Selected: {slug}")),
                None => ui.label("Selected: (none)"),
            };
        });
}
