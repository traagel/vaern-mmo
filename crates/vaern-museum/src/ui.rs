//! egui Composer window — every user-facing picker and slider lives here.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use vaern_assets::{
    Beard as QuaterniusBeard, BELT_MAX, BodySlot, FEET_MAX, Gender, HAND_MAX, HeadPiece as QHeadPiece,
    MESHTINT_DS_PALETTES, MeshtintAnimationCatalog, MeshtintAnimations, MeshtintCatalog,
    MeshtintPieceTaxonomy, Outfit as QOutfit, PieceCategory, QuaterniusColor, QuaterniusHair, Rig,
};

use crate::composer::{Composer, EYE_PRESETS, HAIR_PRESETS, MegakitPropList, SKIN_PRESETS, WeaponList};

pub fn ui_panel(
    mut contexts: EguiContexts,
    mut composer: ResMut<Composer>,
    catalog: Res<MeshtintCatalog>,
    weapon_list: Res<WeaponList>,
    prop_list: Res<MegakitPropList>,
    taxonomy: Res<MeshtintPieceTaxonomy>,
    anim_catalog: Res<MeshtintAnimationCatalog>,
    anims: Res<MeshtintAnimations>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    egui::Window::new("Composer")
        .anchor(egui::Align2::LEFT_TOP, [10.0, 10.0])
        .default_width(360.0)
        .default_height(780.0)
        .show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui_rig(ui, &mut composer);
                    ui.separator();
                    if composer.rig == Rig::QuaterniusModular {
                        ui_quaternius_weapon(ui, &mut composer, &prop_list);
                        ui_quaternius_grip(ui, &mut composer);
                        ui.separator();
                    }
                    // Meshtint-only pickers: base variant, outfit piece
                    // nodes, body overlays, weapon grips, palette swap.
                    // These mutate Meshtint-specific Composer state that
                    // has no effect on Quaternius characters, so hide
                    // the whole block when not on the Meshtint rig.
                    if composer.rig == Rig::Meshtint {
                        ui_base(ui, &mut composer, &catalog);
                        ui.separator();
                        ui_outfit(ui, &mut composer, &taxonomy);
                        ui.separator();
                        ui_body_group(
                            ui,
                            &mut composer,
                            &catalog,
                            &taxonomy,
                            "Face",
                            &[
                                BodySlot::Brow,
                                BodySlot::Eyes,
                                BodySlot::Mouth,
                                BodySlot::Beard,
                            ],
                        );
                        ui_body_group(
                            ui,
                            &mut composer,
                            &catalog,
                            &taxonomy,
                            "Hair",
                            &[BodySlot::Hair],
                        );
                        ui_body_group(
                            ui,
                            &mut composer,
                            &catalog,
                            &taxonomy,
                            "Headgear",
                            &[BodySlot::Helmet, BodySlot::Hat, BodySlot::Headband],
                        );
                        ui_body_group(
                            ui,
                            &mut composer,
                            &catalog,
                            &taxonomy,
                            "Accessories",
                            &[BodySlot::Earring, BodySlot::Necklace],
                        );
                        ui_body_group(
                            ui,
                            &mut composer,
                            &catalog,
                            &taxonomy,
                            "Armor plates",
                            &[BodySlot::Pauldron, BodySlot::Bracer, BodySlot::Poleyn],
                        );
                        ui_weapon(ui, &mut composer, &weapon_list);
                        ui_weapon_grip(ui, &mut composer);
                        ui.separator();
                    }
                    ui_animation(ui, &mut composer, &anim_catalog, &anims);
                    ui.separator();
                    if composer.rig == Rig::Meshtint {
                        ui_palette(ui, &mut composer);
                        ui.separator();
                    }
                    ui_debug(ui, &composer);
                });
        });
}

fn ui_rig(ui: &mut egui::Ui, composer: &mut Composer) {
    ui.label("Rig:");
    ui.horizontal(|ui| {
        for &r in Rig::ALL {
            let selected = composer.rig == r;
            if ui.selectable_label(selected, r.label()).clicked() && !selected {
                composer.rig = r;
                // Selected clip is keyed per-rig; clear so sync_selected_clip
                // doesn't try to play a Meshtint clip on the Quaternius rig.
                composer.selected_clip = None;
            }
        }
    });
    if composer.rig == Rig::QuaterniusModular {
        ui.horizontal(|ui| {
            ui.label("Gender:");
            for &g in Gender::ALL {
                let selected = composer.gender == g;
                if ui.selectable_label(selected, g.label()).clicked() && !selected {
                    composer.gender = g;
                    // Beard is male-only; clear on female.
                    if g == Gender::Female {
                        composer.q_beard = None;
                    }
                    // Hair options differ per gender; drop a pick that's
                    // invalid for the new gender.
                    if let Some(h) = composer.q_hair {
                        if !QuaterniusHair::available_for(g).contains(&h) {
                            composer.q_hair = None;
                        }
                    }
                }
            }
        });

        ui_slot_slider(ui, "Body", &mut composer.q_body, QOutfit::ALL, |o| o.label());
        ui_slot_slider(ui, "Legs", &mut composer.q_legs, QOutfit::ALL, |o| o.label());
        ui_slot_slider(ui, "Arms", &mut composer.q_arms, QOutfit::ALL, |o| o.label());
        ui_slot_slider(ui, "Feet", &mut composer.q_feet, QOutfit::ALL, |o| o.label());
        ui_slot_slider(ui, "Head Piece", &mut composer.q_head_piece, QHeadPiece::ALL, |hp| hp.label());

        let hair_options = QuaterniusHair::available_for(composer.gender);
        ui_slot_slider(ui, "Hair", &mut composer.q_hair, hair_options, |h| h.label());

        if composer.gender == Gender::Male {
            ui_slot_slider(ui, "Beard", &mut composer.q_beard, QuaterniusBeard::ALL, |b| b.label());
        }

        ui.horizontal(|ui| {
            ui.label("Color:");
            for &c in QuaterniusColor::ALL {
                let selected = composer.q_color == c;
                if ui.selectable_label(selected, c.label()).clicked() && !selected {
                    composer.q_color = c;
                }
            }
        });
    }
}

/// Quaternius weapon picker: dropdown over every MEGAKIT prop basename,
/// plus a "None" option for empty hands. Picking a new prop despawns
/// the current overlay (if any) and spawns a fresh one parented to the
/// attach hand resolved from [`QuaterniusGrips`] — seeded with the
/// registry's calibrated grip, then live-editable via [`ui_quaternius_grip`].
fn ui_quaternius_weapon(ui: &mut egui::Ui, composer: &mut Composer, prop_list: &MegakitPropList) {
    ui.collapsing("Weapon (Quaternius)", |ui| {
        if prop_list.basenames.is_empty() {
            ui.small("(no MEGAKIT props on disk)");
            return;
        }
        let current_label = composer
            .q_prop_id
            .as_deref()
            .map(|id| id.replace('_', " "))
            .unwrap_or_else(|| "None".to_string());
        egui::ComboBox::from_id_salt("q_prop_combo")
            .selected_text(current_label)
            .show_ui(ui, |ui| {
                let none_selected = composer.q_prop_id.is_none();
                if ui.selectable_label(none_selected, "None").clicked() && !none_selected {
                    composer.q_prop_id = None;
                }
                for id in &prop_list.basenames {
                    let selected = composer.q_prop_id.as_deref() == Some(id.as_str());
                    if ui
                        .selectable_label(selected, id.replace('_', " "))
                        .clicked()
                    {
                        composer.q_prop_id = Some(id.clone());
                    }
                }
            });
        ui.small(
            "Prop basename resolves via assets/extracted/props/*.gltf. Live-tune \
             the grip below; paste the YAML snippet back into \
             assets/quaternius_weapon_grips.yaml when you're happy.",
        );
    });
}

/// Live Quaternius grip sliders — translation (metres), rotation (°),
/// and 180° flips. Pushed onto the current Quaternius weapon overlay
/// each frame via [`crate::composer::push_quaternius_grip`]. Identical
/// layout to the Meshtint grip panel so authoring feels the same.
fn ui_quaternius_grip(ui: &mut egui::Ui, composer: &mut Composer) {
    ui.collapsing("Weapon grip (Quaternius, live)", |ui| {
        ui.small(
            "Calibrated against the UE Mannequin hand bone (hand_r / hand_l). \
             Values differ from the Meshtint grip for the same weapon — \
             different rig, different bone-local axes.",
        );

        let grip = &mut composer.q_grip;

        ui.label("Translation (m, bone-local):");
        ui.add(egui::Slider::new(&mut grip.tx, -0.5..=0.5).text("tx").fixed_decimals(3));
        ui.add(egui::Slider::new(&mut grip.ty, -0.5..=0.5).text("ty").fixed_decimals(3));
        ui.add(egui::Slider::new(&mut grip.tz, -0.5..=0.5).text("tz").fixed_decimals(3));

        ui.separator();
        ui.label("Rotation (° Euler XYZ):");
        ui.add(egui::Slider::new(&mut grip.rx, -180.0..=180.0).text("rx").fixed_decimals(1));
        ui.add(egui::Slider::new(&mut grip.ry, -180.0..=180.0).text("ry").fixed_decimals(1));
        ui.add(egui::Slider::new(&mut grip.rz, -180.0..=180.0).text("rz").fixed_decimals(1));

        ui.separator();
        ui.label("Flip (180° on axis):");
        ui.horizontal(|ui| {
            ui.checkbox(&mut grip.flip_x, "X");
            ui.checkbox(&mut grip.flip_y, "Y");
            ui.checkbox(&mut grip.flip_z, "Z");
        });

        ui.separator();
        if ui.button("Reset (zero)").clicked() {
            *grip = vaern_assets::QuaterniusGripSpec::default();
        }

        ui.separator();
        ui.small("YAML snippet:");
        ui.monospace(format!(
            "{{ tx: {:.3}, ty: {:.3}, tz: {:.3}, rx: {:.1}, ry: {:.1}, rz: {:.1}, flip_x: {}, flip_y: {}, flip_z: {} }}",
            grip.tx, grip.ty, grip.tz, grip.rx, grip.ry, grip.rz,
            grip.flip_x, grip.flip_y, grip.flip_z,
        ));
    });
}

/// Slider over `options` with `0 = None`. Scrubs through an
/// `Option<T>` where T: Copy + PartialEq via an index in
/// `0..=options.len()`.
fn ui_slot_slider<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut Option<T>,
    options: &[T],
    item_label: impl Fn(T) -> &'static str,
) {
    let max = options.len();
    let current = value
        .and_then(|v| options.iter().position(|o| *o == v).map(|i| i + 1))
        .unwrap_or(0);
    let mut idx = current;
    let item_text = if idx == 0 {
        "None".to_string()
    } else {
        options
            .get(idx - 1)
            .map(|v| item_label(*v).to_string())
            .unwrap_or_else(|| format!("#{idx}"))
    };
    ui.add(
        egui::Slider::new(&mut idx, 0..=max)
            .text(format!("{label}: {item_text}"))
            .integer(),
    );
    if idx != current {
        *value = if idx == 0 { None } else { options.get(idx - 1).copied() };
    }
}

fn ui_base(ui: &mut egui::Ui, composer: &mut Composer, catalog: &MeshtintCatalog) {
    ui.label("Base:");
    ui.horizontal(|ui| {
        for &g in Gender::ALL {
            let selected = composer.gender == g;
            if ui.selectable_label(selected, g.label()).clicked() && !selected {
                composer.gender = g;
                composer.outfit.torso = composer.outfit.torso.min(g.torso_max());
                composer.outfit.bottom = composer.outfit.bottom.min(g.bottom_max());
                // Clamp base variant to the new gender's available range.
                let max = catalog.base_variant_max(g);
                composer.base_variant = composer.base_variant.min(max).max(1);
                // Clear body picks — Beard disappears on Female, Hair
                // variant counts differ.
                for (_, v) in composer.body_picks.iter_mut() {
                    *v = 0;
                }
            }
        }
    });
    let base_max = catalog.base_variant_max(composer.gender);
    if base_max > 1 {
        ui.add(
            egui::Slider::new(&mut composer.base_variant, 1..=base_max)
                .text(format!("{} variant", composer.gender.label())),
        );
    } else {
        ui.small(format!(
            "{}_{:02} (only variant shipped in Polygonal Fantasy Pack 1.4)",
            composer.gender.label(),
            composer.base_variant,
        ));
    }
}

fn ui_outfit(ui: &mut egui::Ui, composer: &mut Composer, taxonomy: &MeshtintPieceTaxonomy) {
    ui.label("Outfit:");
    let g = composer.gender;
    ui_outfit_slider(ui, g, taxonomy, PieceCategory::Torso, &mut composer.outfit.torso, 1..=g.torso_max(), "Torso");
    ui_outfit_slider(ui, g, taxonomy, PieceCategory::Bottom, &mut composer.outfit.bottom, 1..=g.bottom_max(), "Bottom");
    ui_outfit_slider(ui, g, taxonomy, PieceCategory::Feet, &mut composer.outfit.feet, 1..=FEET_MAX, "Feet");
    ui_outfit_slider(ui, g, taxonomy, PieceCategory::Hand, &mut composer.outfit.hand, 1..=HAND_MAX, "Hand");
    // Belt allows 0 = no belt — the outfit visibility pass clamps to the
    // gender's torso/bottom range but belt "06 Belt 00" simply matches
    // no piece-node, leaving every belt variant hidden.
    ui_outfit_slider(ui, g, taxonomy, PieceCategory::Belt, &mut composer.outfit.belt, 0..=BELT_MAX, "Belt");
}

fn ui_outfit_slider(
    ui: &mut egui::Ui,
    gender: Gender,
    taxonomy: &MeshtintPieceTaxonomy,
    category: PieceCategory,
    value: &mut u32,
    range: std::ops::RangeInclusive<u32>,
    label: &str,
) {
    let name = taxonomy.label(gender, category, *value);
    ui.add(egui::Slider::new(value, range).text(format!("{label}: {name}")));
}

fn ui_body_group(
    ui: &mut egui::Ui,
    composer: &mut Composer,
    catalog: &MeshtintCatalog,
    taxonomy: &MeshtintPieceTaxonomy,
    heading: &str,
    slots: &[BodySlot],
) {
    ui.collapsing(heading, |ui| {
        for &slot in slots {
            let opts = catalog.body(composer.gender, slot);
            if opts.is_empty() {
                ui.add_enabled_ui(false, |ui| {
                    ui.label(format!("{}: (unavailable)", slot.label()));
                });
                continue;
            }
            let max = opts.iter().map(|v| v.number).max().unwrap_or(0);
            let picked = composer.body_picks.get(&slot).copied().unwrap_or(0);
            // Prefer the authored taxonomy name; fall back to the
            // catalog's file-derived label ("Pauldron 03"), then to
            // "None" for variant 0.
            let label = taxonomy.overlay(composer.gender, slot, picked).map(|t| t.name.clone())
                .or_else(|| {
                    if picked == 0 {
                        Some("None".to_string())
                    } else {
                        opts.iter().find(|v| v.number == picked).map(|v| v.label.clone())
                    }
                })
                .unwrap_or_else(|| format!("#{picked}"));
            let mut value = picked;
            // Slider 0..=max where 0 means "None".
            ui.add(
                egui::Slider::new(&mut value, 0..=max)
                    .text(format!("{}: {}", slot.label(), label)),
            );
            if value != picked {
                // Goes through set_body_pick so conflicting slots (e.g.
                // Helmet picked → Hair/Hat/Headband cleared) get zeroed
                // out automatically.
                composer.set_body_pick(slot, value);
            }
        }
    });
}

fn ui_weapon(ui: &mut egui::Ui, composer: &mut Composer, weapon_list: &WeaponList) {
    ui.collapsing("Weapon", |ui| {
        let total = weapon_list.entries.len();
        if total == 0 {
            ui.label("(no weapons in catalog)");
            return;
        }
        let label = if composer.weapon_idx == 0 {
            "None".to_string()
        } else if let Some((_, _, l)) = weapon_list.entries.get(composer.weapon_idx - 1) {
            l.clone()
        } else {
            format!("#{}", composer.weapon_idx)
        };
        ui.add(
            egui::Slider::new(&mut composer.weapon_idx, 0..=total)
                .text(format!("Weapon: {label}")),
        );
    });
}

fn ui_weapon_grip(ui: &mut egui::Ui, composer: &mut Composer) {
    ui.collapsing("Weapon grip (live)", |ui| {
        ui.small(
            "Auto-synced from assets/meshtint_weapon_grips.yaml when you \
             pick a weapon. Tune freely; the YAML snippet below is ready \
             to paste back into the file.",
        );

        let grip = &mut composer.grip;

        ui.label("Translation (m, bone-local):");
        ui.add(egui::Slider::new(&mut grip.tx, -0.5..=0.5).text("tx").fixed_decimals(3));
        ui.add(egui::Slider::new(&mut grip.ty, -0.5..=0.5).text("ty").fixed_decimals(3));
        ui.add(egui::Slider::new(&mut grip.tz, -0.5..=0.5).text("tz").fixed_decimals(3));

        ui.separator();
        ui.label("Rotation (° Euler XYZ):");
        ui.add(egui::Slider::new(&mut grip.rx, -180.0..=180.0).text("rx").fixed_decimals(1));
        ui.add(egui::Slider::new(&mut grip.ry, -180.0..=180.0).text("ry").fixed_decimals(1));
        ui.add(egui::Slider::new(&mut grip.rz, -180.0..=180.0).text("rz").fixed_decimals(1));

        ui.separator();
        ui.label("Flip (180° on axis):");
        ui.horizontal(|ui| {
            ui.checkbox(&mut grip.flip_x, "X");
            ui.checkbox(&mut grip.flip_y, "Y");
            ui.checkbox(&mut grip.flip_z, "Z");
        });

        ui.separator();
        if ui.button("Reset (zero)").clicked() {
            *grip = vaern_assets::GripSpec::default();
        }

        ui.separator();
        ui.small("YAML snippet:");
        ui.monospace(format!(
            "{{ tx: {:.3}, ty: {:.3}, tz: {:.3}, rx: {:.1}, ry: {:.1}, rz: {:.1}, flip_x: {}, flip_y: {}, flip_z: {} }}",
            grip.tx, grip.ty, grip.tz, grip.rx, grip.ry, grip.rz,
            grip.flip_x, grip.flip_y, grip.flip_z,
        ));
    });
}

fn ui_animation(
    ui: &mut egui::Ui,
    composer: &mut Composer,
    catalog: &MeshtintAnimationCatalog,
    anims: &MeshtintAnimations,
) {
    ui.collapsing("Animation", |ui| {
        let rig = composer.rig;
        let clips: Vec<_> = catalog.iter_rig(rig).collect();
        if clips.is_empty() {
            ui.label(match rig {
                Rig::Meshtint => "(no retargeted clips — run scripts/retarget_ual_to_meshtint.py --all)",
                Rig::QuaterniusModular => "(no UAL clips — check UAL1/UAL2 GLBs in extracted/animations/)",
            });
            return;
        }
        if !anims.is_ready() {
            ui.small("(loading clips…)");
        }

        let current_label = composer
            .selected_clip
            .as_ref()
            .and_then(|k| catalog.get(k))
            .map(|c| c.pretty.clone())
            .unwrap_or_else(|| "None (bind pose)".to_string());

        egui::ComboBox::from_label("Clip")
            .selected_text(current_label)
            .show_ui(ui, |ui| {
                let selected_none = composer.selected_clip.is_none();
                if ui
                    .selectable_label(selected_none, "None (bind pose)")
                    .clicked()
                {
                    composer.selected_clip = None;
                }
                for clip in &clips {
                    let selected = composer.selected_clip.as_deref() == Some(&clip.key);
                    if ui.selectable_label(selected, clip.pretty.clone()).clicked() {
                        composer.selected_clip = Some(clip.key.clone());
                    }
                }
            });

        ui.small(match rig {
            Rig::Meshtint => {
                "Meshtint rig: clips Blender-baked onto the Meshtint skeleton."
            }
            Rig::QuaterniusModular => {
                "Mannequin rig: raw UAL clips (UE5 skeleton, native)."
            }
        });
    });
}

fn ui_palette(ui: &mut egui::Ui, composer: &mut Composer) {
    ui.collapsing("Palette", |ui| {
        ui.small("Palette swap is sticky — no clean revert without respawning the character.");
        let count = MESHTINT_DS_PALETTES.len() as u32;
        let mut idx = composer.palette_pick.map(|i| i as u32 + 1).unwrap_or(0);
        let label = if idx == 0 {
            "(original)".to_string()
        } else {
            MESHTINT_DS_PALETTES
                .get((idx - 1) as usize)
                .copied()
                .unwrap_or("?")
                .to_string()
        };
        ui.add(egui::Slider::new(&mut idx, 0..=count).text(format!("Palette: {label}")));
        composer.palette_pick = if idx == 0 { None } else { Some((idx - 1) as usize) };

        ui.separator();
        ui.label("Skin (also affects brow + beard — same palette region):");
        ui_preset_row(ui, SKIN_PRESETS, &mut composer.skin_color);
        ui_freeform_color(ui, "Override skin color", [66, 31, 21], &mut composer.skin_color);

        ui.separator();
        ui.label("Hair / Brow / Beard (flat override, bypasses palette):");
        ui_preset_row(ui, HAIR_PRESETS, &mut composer.hair_color);
        ui_freeform_color(ui, "Override hair color", [60, 40, 25], &mut composer.hair_color);

        ui.separator();
        ui.label("Eyes (flat override, bypasses palette):");
        ui_preset_row(ui, EYE_PRESETS, &mut composer.eye_color);
        ui_freeform_color(ui, "Override eye color", [70, 130, 180], &mut composer.eye_color);
    });
}

/// Grid of preset swatches — click to pick, hover for the name.
fn ui_preset_row(
    ui: &mut egui::Ui,
    presets: &[(&'static str, [u8; 3])],
    target: &mut Option<[u8; 3]>,
) {
    ui.horizontal_wrapped(|ui| {
        for (name, rgb) in presets {
            let color = egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            let selected = *target == Some(*rgb);
            let mut button = egui::Button::new("").fill(color).min_size(egui::vec2(22.0, 22.0));
            if selected {
                button = button.stroke(egui::Stroke::new(2.0, egui::Color32::WHITE));
            }
            if ui
                .add(button)
                .on_hover_text(format!(
                    "{name} — R{} G{} B{}",
                    rgb[0], rgb[1], rgb[2]
                ))
                .clicked()
            {
                *target = Some(*rgb);
            }
        }
    });
}

fn ui_freeform_color(
    ui: &mut egui::Ui,
    label: &str,
    default_rgb: [u8; 3],
    target: &mut Option<[u8; 3]>,
) {
    let mut enabled = target.is_some();
    if ui.checkbox(&mut enabled, label).changed() {
        *target = if enabled { Some(default_rgb) } else { None };
    }
    if let Some(rgb) = target.as_mut() {
        ui.horizontal(|ui| {
            egui::color_picker::color_edit_button_srgb(ui, rgb);
            ui.label(format!("R{:3} G{:3} B{:3}", rgb[0], rgb[1], rgb[2]));
        });
    }
}

fn ui_debug(ui: &mut egui::Ui, composer: &Composer) {
    ui.collapsing("debug", |ui| {
        ui.label(format!("active overlays: {}", composer.stats_active_overlays));
        ui.label(format!(
            "character entity: {}",
            composer
                .character
                .map(|e| format!("{e:?}"))
                .unwrap_or_else(|| "—".to_string())
        ));
    });
}
