use crate::career::interactive::DecisionPoint;
use crate::r#match::MatchResult;

#[derive(Clone, Copy, Debug, Default)]
pub struct WorldWorkloadCounts {
    pub countries: u64,
    pub leagues: u64,
    pub clubs: u64,
    pub players: u64,
}

pub struct SimulationResult {
    pub match_results: Vec<MatchResult>,
    /// Number of continents whose `simulate` call panicked during this
    /// tick. Surfaces silent failures the orchestrator catches and
    /// substitutes empty results for. Sum across ticks via the
    /// process-global `ContinentPanicMetrics::total()`.
    pub panicked_continents: u32,
    /// When the game runs in interactive mode and the simulator reaches
    /// a decision point (pre-match, transfer window, season end, job
    /// event), this field is set so the caller knows to pause and wait
    /// for user input. Always `None` in autonomous (non-interactive)
    /// mode.
    pub pending_decision: Option<DecisionPoint>,
}

impl Default for SimulationResult {
    fn default() -> Self {
        Self::new()
    }
}

impl SimulationResult {
    pub fn new() -> Self {
        SimulationResult {
            match_results: Vec::new(),
            panicked_continents: 0,
            pending_decision: None,
        }
    }

    pub fn has_match_results(&self) -> bool {
        !self.match_results.is_empty()
    }
}
