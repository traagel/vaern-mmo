//! Small presentation helpers shared by NPC spawning and hotbar snapshot
//! building. Stay string-level — no Bevy or data deps.

/// Clean up a chain step's `target_hint` for use as an NPC display name.
/// Capitalize the first letter, preserve the rest (already natural prose
/// like "the Dalewatch Warden" or "Old Brenn the shepherd").
pub fn prettify_npc_name(raw: &str) -> String {
    let raw = raw.trim();
    let mut chars = raw.chars();
    match chars.next() {
        Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Turn a snake_case / lowercase variant name into a display form.
pub fn prettify_ability_name(raw: &str) -> String {
    raw.split(&['_', ' '][..])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
