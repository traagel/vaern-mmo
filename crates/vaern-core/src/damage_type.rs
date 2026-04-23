use serde::{Deserialize, Serialize};

/// Canonical damage-type taxonomy — audited from
/// `src/generated/schools/*/*.yaml` (every `damage_type` value across
/// the 27 schools reduces to one of these 12).
///
/// Physical (3): slashing, piercing, bludgeoning — Armor handles bulk
/// mitigation; per-type resist lets layered-armor profiles differentiate
/// (plate resists slashing, bludgeoning bypasses mail, etc.).
/// Magical (9): fire, cold, lightning, force, radiant, necrotic, blood,
/// poison, acid — one resist channel each for hardcore-prep gear swaps.
///
/// `as usize` is the stable index for `[f32; DAMAGE_TYPE_COUNT]` arrays
/// in `vaern-stats`. Do not reorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum DamageType {
    Slashing = 0,
    Piercing = 1,
    Bludgeoning = 2,
    Fire = 3,
    Cold = 4,
    Lightning = 5,
    Force = 6,
    Radiant = 7,
    Necrotic = 8,
    Blood = 9,
    Poison = 10,
    Acid = 11,
}

/// Number of canonical damage types. Used to size resist arrays.
/// Stable — don't change without auditing every `[f32; DAMAGE_TYPE_COUNT]` site.
pub const DAMAGE_TYPE_COUNT: usize = 12;

impl DamageType {
    pub const ALL: [DamageType; DAMAGE_TYPE_COUNT] = [
        DamageType::Slashing,
        DamageType::Piercing,
        DamageType::Bludgeoning,
        DamageType::Fire,
        DamageType::Cold,
        DamageType::Lightning,
        DamageType::Force,
        DamageType::Radiant,
        DamageType::Necrotic,
        DamageType::Blood,
        DamageType::Poison,
        DamageType::Acid,
    ];

    pub fn is_physical(self) -> bool {
        matches!(
            self,
            DamageType::Slashing | DamageType::Piercing | DamageType::Bludgeoning
        )
    }

    pub fn is_magical(self) -> bool {
        !self.is_physical()
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// Parse from the string form used in school YAMLs
    /// (`damage_type: fire`). Returns `None` for the non-damaging
    /// case (e.g. tonics, which have `damage_type: null`).
    pub fn from_str(s: &str) -> Option<Self> {
        Some(match s {
            "slashing" => DamageType::Slashing,
            "piercing" => DamageType::Piercing,
            "bludgeoning" => DamageType::Bludgeoning,
            "fire" => DamageType::Fire,
            "cold" => DamageType::Cold,
            "lightning" => DamageType::Lightning,
            "force" => DamageType::Force,
            "radiant" => DamageType::Radiant,
            "necrotic" => DamageType::Necrotic,
            "blood" => DamageType::Blood,
            "poison" => DamageType::Poison,
            "acid" => DamageType::Acid,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indices_are_stable_zero_through_eleven() {
        for (i, dt) in DamageType::ALL.iter().enumerate() {
            assert_eq!(dt.index(), i);
        }
    }

    #[test]
    fn physical_subset_is_exactly_three() {
        let count = DamageType::ALL.iter().filter(|d| d.is_physical()).count();
        assert_eq!(count, 3);
    }

    #[test]
    fn magical_subset_is_exactly_nine() {
        let count = DamageType::ALL.iter().filter(|d| d.is_magical()).count();
        assert_eq!(count, 9);
    }

    #[test]
    fn from_str_covers_every_school_damage_type() {
        // Values harvested from src/generated/schools/*/*.yaml.
        for s in [
            "slashing",
            "piercing",
            "bludgeoning",
            "fire",
            "cold",
            "lightning",
            "force",
            "radiant",
            "necrotic",
            "blood",
            "poison",
            "acid",
        ] {
            assert!(DamageType::from_str(s).is_some(), "missed: {s}");
        }
        assert!(DamageType::from_str("null").is_none());
        assert!(DamageType::from_str("nonsense").is_none());
    }
}
