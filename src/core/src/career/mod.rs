pub mod contract;
pub mod history;
pub mod reputation;

use reputation::ManagerReputation;
use history::ManagerCareerEntry;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagerCareerState {
    pub reputation: ManagerReputation,
    pub history: Vec<ManagerCareerEntry>,
}

impl ManagerCareerState {
    pub fn new(initial_score: u16) -> Self {
        Self {
            reputation: ManagerReputation::new(initial_score),
            history: Vec::new(),
        }
    }
}
