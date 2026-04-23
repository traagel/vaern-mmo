use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Pillar {
    Might,
    Arcana,
    Finesse,
}

impl fmt::Display for Pillar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Pillar::Might => "might",
            Pillar::Arcana => "arcana",
            Pillar::Finesse => "finesse",
        })
    }
}

/// A pillar value quantized to {0, 25, 50, 75, 100}.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PillarValue(u8);

impl PillarValue {
    pub const ZERO: Self = Self(0);
    pub const Q1: Self = Self(25);
    pub const Q2: Self = Self(50);
    pub const Q3: Self = Self(75);
    pub const FULL: Self = Self(100);

    pub const fn new(v: u8) -> Option<Self> {
        match v {
            0 | 25 | 50 | 75 | 100 => Some(Self(v)),
            _ => None,
        }
    }

    pub const fn get(self) -> u8 {
        self.0
    }
}

impl fmt::Display for PillarValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
