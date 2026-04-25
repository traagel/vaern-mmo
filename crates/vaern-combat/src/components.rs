use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use vaern_core::{ClassPosition, Faction};

use crate::effects::EffectSpec;

#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Health {
    pub current: f32,
    pub max: f32,
}

impl Health {
    pub fn full(max: f32) -> Self {
        Self { current: max, max }
    }

    pub fn is_dead(self) -> bool {
        self.current <= 0.0
    }
}

#[derive(Component, Debug, Clone, Copy)]
pub struct FactionTag(pub Faction);

#[derive(Component, Debug, Clone, Copy)]
pub struct Position3Pillar(pub ClassPosition);

/// Shape of an ability's effect. Drives both range checks and how damage
/// fans out on resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AbilityShape {
    /// Single target at up to `range` away from caster. Default.
    Target,
    /// Single target must be within `range` to cast. On resolution, damages
    /// every eligible entity within `aoe_radius` of the primary target.
    /// Friendly fire ON: only the caster is excluded.
    AoeOnTarget,
    /// Self-centered radial AoE. `range` ignored (caster IS the origin).
    AoeOnSelf,
    /// Cone from caster in the direction of the primary target. Uses
    /// `range` for reach and `cone_half_angle_deg` for spread. Hits every
    /// entity inside the wedge (friendly fire on).
    Cone,
    /// Rectangular line from caster in the direction of the primary target.
    /// Uses `range` for length and `line_width` for total width. Hits
    /// every entity inside the rectangle.
    Line,
    /// Travelling projectile. Spawned at cast resolution; flies at
    /// `projectile_speed` along the caster→target direction for up to
    /// `range` units. First hostile entity it touches within
    /// `projectile_radius` takes damage and stops the projectile.
    Projectile,
}

impl Default for AbilityShape {
    fn default() -> Self {
        Self::Target
    }
}

/// Static description of a single ability. Lives on an ability entity, not on
/// the caster — a caster can have N abilities, each as its own entity linked
/// back via `Caster`.
#[derive(Component, Debug, Clone)]
pub struct AbilitySpec {
    pub damage: f32,
    pub cooldown_secs: f32,
    /// 0.0 = instant; otherwise the caster enters a Casting state for this
    /// duration before damage resolves.
    pub cast_secs: f32,
    pub resource_cost: f32,
    /// School id (e.g. "fire", "blade", "poison"). Keyed against the Schools
    /// resource for damage-type/morality/etc. lookups.
    pub school: String,
    /// Threat generated = `damage * threat_multiplier`. Taunts/auras set >1.0
    /// so tanks hold aggro without outdamaging the group. 1.0 is baseline.
    pub threat_multiplier: f32,
    /// Max distance from caster to primary target at cast time, in world
    /// units. Melee ~ 3.0, ranged spells/bows ~ 30.0. Self-targeted abilities
    /// (shape == AoeOnSelf) ignore this.
    pub range: f32,
    /// Target/AoE shape.
    pub shape: AbilityShape,
    /// Radius for `AoeOnTarget` / `AoeOnSelf`. Ignored for other shapes.
    pub aoe_radius: f32,
    /// Cone half-angle in degrees. E.g. 30 = 60° total wedge. Cone only.
    pub cone_half_angle_deg: f32,
    /// Total rectangle width for `Line` shape. The rectangle extends
    /// `line_width / 2` on each side of the caster→target line.
    pub line_width: f32,
    /// Projectile speed in world-units/second. Projectile only.
    pub projectile_speed: f32,
    /// Projectile hitbox radius. Swept-sphere collision against entities
    /// along the flight path. Projectile only.
    pub projectile_radius: f32,
    /// Optional status-effect rider. Applied to the primary target (or
    /// to every AoE victim) whenever this ability lands with non-zero
    /// damage — parried hits don't attach riders. Populated from
    /// `applies_effect` in flavored YAML.
    pub applies_effect: Option<EffectSpec>,
}

impl Default for AbilitySpec {
    fn default() -> Self {
        Self {
            damage: 0.0,
            cooldown_secs: 1.0,
            cast_secs: 0.0,
            resource_cost: 0.0,
            school: String::new(),
            threat_multiplier: 1.0,
            range: 30.0,
            shape: AbilityShape::Target,
            aoe_radius: 0.0,
            cone_half_angle_deg: 30.0, // 60° wedge
            line_width: 2.0,
            projectile_speed: 30.0,
            projectile_radius: 0.6,
            applies_effect: None,
        }
    }
}

/// Per-ability cooldown state. Seconds until the ability is ready again.
/// Zero means ready.
#[derive(Component, Debug, Clone, Copy)]
pub struct AbilityCooldown {
    pub remaining_secs: f32,
}

impl AbilityCooldown {
    pub fn ready() -> Self {
        Self { remaining_secs: 0.0 }
    }

    pub fn is_ready(self) -> bool {
        self.remaining_secs <= 0.0
    }
}

/// Who owns this ability. Ability entities point back at their caster.
#[derive(Component, Debug, Clone, Copy)]
pub struct Caster(pub Entity);

/// Higher wins. Selection picks the highest-priority ready+affordable ability
/// per caster per tick. Missing component is treated as priority 0.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AbilityPriority(pub u8);

/// Entity being attacked. Lives on the caster. Simplest possible targeting
/// model — no threat, no range checks, no line of sight.
#[derive(Component, Debug, Clone, Copy)]
pub struct Target(pub Entity);

/// In-flight cast state. Attached to a caster when a non-instant ability
/// begins; removed when the cast resolves. Damage / school / shape and the
/// horizontal aim direction are all snapshotted at cast start so the effect
/// resolves correctly even if the caster moves or the ability entity is
/// despawned mid-cast.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Casting {
    pub ability: Entity,
    pub target: Entity,
    pub remaining_secs: f32,
    pub total_secs: f32,
    pub damage: f32,
    pub school: String,
    pub threat_multiplier: f32,
    #[serde(default)]
    pub shape: AbilityShape,
    /// Snapshotted from the ability spec so Cone/Line/Projectile resolutions
    /// stay bounded. Without this, channeled casts would fan to infinity
    /// because the ability entity can be despawned mid-cast.
    #[serde(default)]
    pub range: f32,
    #[serde(default)]
    pub aoe_radius: f32,
    #[serde(default)]
    pub cone_half_angle_deg: f32,
    #[serde(default)]
    pub line_width: f32,
    #[serde(default)]
    pub projectile_speed: f32,
    #[serde(default)]
    pub projectile_radius: f32,
    /// XZ-plane aim direction (unit vector) at cast start. `(0,0,0)` means
    /// "no valid aim" — e.g. caster has no target or target is at the same
    /// position; Cone/Line/Projectile shapes fizzle when this is zero.
    #[serde(default)]
    pub aim: Vec3,
    /// Status-effect rider snapshotted from `AbilitySpec` so the
    /// channeled cast still attaches the right effect on completion
    /// even if the ability entity is despawned mid-cast.
    #[serde(default)]
    pub applies_effect: Option<EffectSpec>,
}

impl bevy::ecs::entity::MapEntities for Casting {
    fn map_entities<M: bevy::ecs::entity::EntityMapper>(&mut self, mapper: &mut M) {
        self.ability = mapper.get_mapped(self.ability);
        self.target = mapper.get_mapped(self.target);
    }
}

/// Marker: this caster does not auto-select abilities. It fires only when a
/// CastRequest is attached (by input, AI, or scripting).
#[derive(Component, Debug, Clone, Copy)]
pub struct ManualCast;

/// If present, `apply_deaths` resets the entity to full HP / resource pool
/// and warps it back to `home` instead of despawning it. Lets the server
/// keep a player's replicated entity alive through death cycles so the
/// client's Predicted copy doesn't vanish (which would freeze the HP bar
/// at its last-rendered value).
#[derive(Component, Debug, Clone, Copy)]
pub struct Respawnable {
    pub home: Vec3,
}

/// If present, `apply_deaths` skips this entity entirely — death handling
/// is delegated to a server-side corpse-run system. Used for player
/// entities so the death position can be captured before teleport-home,
/// a corpse marker spawned, and HP reduced to a penalty fraction.
#[derive(Component, Debug, Clone, Copy)]
pub struct CorpseOnDeath;

/// Player-facing display name replicated to clients for nameplates /
/// interaction prompts. Set once at spawn, not mutated after.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayName(pub String);

/// Replicated marker on projectile entities. Carries the school id so
/// clients can pick a color; actual motion comes from the Transform stream.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectileVisual {
    pub school: String,
}

/// Hint to the client for how to render/interact with an NPC: plain enemy,
/// quest giver, vendor, etc. Drives nameplate color and interaction cursor.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum NpcKind {
    Combat,
    QuestGiver,
    Named,
    Elite,
    /// Non-combat shopkeeper. F-key opens the vendor UI (client) /
    /// sends `VendorOpenRequest` (server). Stocks are authored in
    /// `src/generated/vendors.yaml` and attached as `VendorStock`
    /// components on spawn.
    Vendor,
}

/// Replicated marker on quest-giver NPCs. Identifies the hub they belong
/// to + optional chain binding: `chain_id` points at the specific chain
/// this NPC hosts, and `step_index` (0-indexed) says which step of that
/// chain this NPC represents. Step 0 is the main giver (Accept happens
/// here); later steps are "talk-to" contact points for progress.
#[derive(Component, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestGiverHub {
    pub hub_id: String,
    pub hub_role: String,
    pub zone_id: String,
    /// Which chain this NPC hosts. `None` = ambient greeter with no quests.
    #[serde(default)]
    pub chain_id: Option<String>,
    /// Zero-indexed step in the chain. 0 = main giver (Accept button).
    /// Higher values = mid-chain contacts (Progress button when the
    /// player's current step matches).
    #[serde(default)]
    pub step_index: Option<u32>,
}

/// Explicit cast request from input / AI. Points at the ability entity the
/// caster wants to fire. Consumed (removed) by select_and_fire whether the
/// cast succeeds or is rejected (cooldown, resource, mid-cast).
#[derive(Component, Debug, Clone, Copy)]
pub struct CastRequest(pub Entity);

/// Generic resource pool consumed by abilities. Math layer is flavor-neutral:
/// mana / stamina / focus flavoring lives in the presentation layer later.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ResourcePool {
    pub current: f32,
    pub max: f32,
    pub regen_per_sec: f32,
}

impl ResourcePool {
    pub fn full(max: f32, regen_per_sec: f32) -> Self {
        Self { current: max, max, regen_per_sec }
    }

    pub fn can_afford(self, cost: f32) -> bool {
        self.current >= cost
    }
}

/// Stamina pool. Separate from `ResourcePool` (mana) so both can live
/// on the same entity. Drained by active stances (Block continuously,
/// Parry per-hit), spent by sprint / dodge in future slices. Passive
/// regen happens every frame via `effects::regen_stamina`.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Stamina {
    pub current: f32,
    pub max: f32,
    pub regen_per_sec: f32,
}

impl Stamina {
    pub fn full(max: f32, regen_per_sec: f32) -> Self {
        Self { current: max, max, regen_per_sec }
    }
}
