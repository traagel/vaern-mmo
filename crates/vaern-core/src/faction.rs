use crate::morality::Morality;

/// Placeholder identifiers until Concord / Rend naming is locked (README TODO).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Faction {
    A,
    B,
}

impl Faction {
    pub const fn alignment(self) -> Morality {
        match self {
            Faction::A => Morality::Good,
            Faction::B => Morality::Evil,
        }
    }
}
