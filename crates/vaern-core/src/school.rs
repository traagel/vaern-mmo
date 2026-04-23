use serde::{Deserialize, Serialize};

use crate::{morality::Morality, pillar::Pillar};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub struct School {
    #[serde(rename = "name")]
    pub id: String,
    pub pillar: Pillar,
    pub morality: Morality,
    pub family: String,
    #[serde(default)]
    pub tag: String,
    pub damage_type: Option<String>,
    #[serde(default)]
    pub applies_to_categories: Vec<String>,
}
