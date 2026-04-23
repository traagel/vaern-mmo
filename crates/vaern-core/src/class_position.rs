use std::fmt;

use crate::pillar::PillarValue;

/// A valid barycentric position: (might, arcana, finesse) each ∈ {0,25,50,75,100}, summing to 100.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClassPosition {
    pub might: PillarValue,
    pub arcana: PillarValue,
    pub finesse: PillarValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidPosition;

pub const INVALID_POSITION: InvalidPosition = InvalidPosition;

impl ClassPosition {
    pub const fn try_new(m: u8, a: u8, f: u8) -> Result<Self, InvalidPosition> {
        if m as u16 + a as u16 + f as u16 != 100 {
            return Err(INVALID_POSITION);
        }
        let (Some(might), Some(arcana), Some(finesse)) =
            (PillarValue::new(m), PillarValue::new(a), PillarValue::new(f))
        else {
            return Err(INVALID_POSITION);
        };
        Ok(Self { might, arcana, finesse })
    }

    pub const fn label(self) -> ClassLabel {
        match (self.might.get(), self.arcana.get(), self.finesse.get()) {
            (100, 0, 0) => ClassLabel::Fighter,
            (75, 25, 0) => ClassLabel::Paladin,
            (50, 50, 0) => ClassLabel::Cleric,
            (25, 75, 0) => ClassLabel::Druid,
            (0, 100, 0) => ClassLabel::Wizard,
            (0, 75, 25) => ClassLabel::Sorcerer,
            (0, 50, 50) => ClassLabel::Warlock,
            (0, 25, 75) => ClassLabel::Bard,
            (0, 0, 100) => ClassLabel::Rogue,
            (25, 0, 75) => ClassLabel::Ranger,
            (50, 0, 50) => ClassLabel::Monk,
            (75, 0, 25) => ClassLabel::Barbarian,
            (50, 25, 25) => ClassLabel::Duskblade,
            (25, 50, 25) => ClassLabel::Mystic,
            (25, 25, 50) => ClassLabel::Warden,
            _ => ClassLabel::Unknown,
        }
    }
}

impl fmt::Display for ClassPosition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({},{},{})", self.might, self.arcana, self.finesse)
    }
}

/// Dev-facing D&D-adjacent labels. Never shown to players.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClassLabel {
    Fighter,
    Paladin,
    Cleric,
    Druid,
    Wizard,
    Sorcerer,
    Warlock,
    Bard,
    Rogue,
    Ranger,
    Monk,
    Barbarian,
    Duskblade,
    Mystic,
    Warden,
    Unknown,
}

/// All 15 valid class positions, in README order.
pub const VALID_POSITIONS: [ClassPosition; 15] = {
    // Unwrap via const-unfriendly route avoided by re-declaring via tuple + const construction.
    const fn p(m: u8, a: u8, f: u8) -> ClassPosition {
        match ClassPosition::try_new(m, a, f) {
            Ok(pos) => pos,
            Err(_) => panic!("invalid position in VALID_POSITIONS"),
        }
    }
    [
        p(100, 0, 0),
        p(75, 25, 0),
        p(50, 50, 0),
        p(25, 75, 0),
        p(0, 100, 0),
        p(0, 75, 25),
        p(0, 50, 50),
        p(0, 25, 75),
        p(0, 0, 100),
        p(25, 0, 75),
        p(50, 0, 50),
        p(75, 0, 25),
        p(50, 25, 25),
        p(25, 50, 25),
        p(25, 25, 50),
    ]
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_valid_positions_enumerated() {
        assert_eq!(VALID_POSITIONS.len(), 15);
    }

    #[test]
    fn every_position_has_a_label() {
        for p in VALID_POSITIONS {
            assert_ne!(p.label(), ClassLabel::Unknown, "unlabeled: {p}");
        }
    }

    #[test]
    fn invalid_sum_rejected() {
        assert!(ClassPosition::try_new(50, 50, 50).is_err());
    }

    #[test]
    fn invalid_quantization_rejected() {
        assert!(ClassPosition::try_new(10, 40, 50).is_err());
    }
}
