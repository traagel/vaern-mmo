use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Morality {
    Good,
    Neutral,
    Evil,
}

impl Morality {
    /// Gating rule: a school is teachable by a faction iff
    /// school is neutral OR school morality equals faction morality.
    pub const fn teachable_by(self, faction_alignment: Morality) -> bool {
        matches!(
            (self, faction_alignment),
            (Morality::Neutral, _)
                | (Morality::Good, Morality::Good)
                | (Morality::Evil, Morality::Evil)
        )
    }
}

impl fmt::Display for Morality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Morality::Good => "good",
            Morality::Neutral => "neutral",
            Morality::Evil => "evil",
        })
    }
}
