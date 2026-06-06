use std::collections::HashMap;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::career::contract::{generate_offer, ManagerOfferTerms};
use crate::career::history::{CareerExitReason, ManagerCareerEntry, MatchRecord};
use crate::career::reputation::{
    apply_season_delta, ManagerReputation, ReputationConfig, SeasonDeltaInput,
};
use crate::career::tactics::TacticalChoice;
use crate::career::transfer::{
    validate_bid_request, SaleListingRequest, TransferBidRequest, TransferBudget,
    TransferDecision,
};
use crate::career::ManagerCareerState;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StartCareerError {
    ClubNotFound { club_id: u32 },
    ManagerNotFound { staff_id: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserManager {
    pub staff_id: u32,
    pub club_id: u32,
    pub manager_name: String,
}

impl UserManager {
    pub fn new(staff_id: u32, club_id: u32, manager_name: String) -> Self {
        Self {
            staff_id,
            club_id,
            manager_name,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobEventType {
    Sacked { reason: String },
    ContractExpiring,
    JobOffer { club_id: u32, club_name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DecisionPoint {
    PreMatch {
        fixture_id: u32,
        opponent: String,
        competition: String,
    },
    TransferWindow {
        season: u16,
        window_open: bool,
    },
    SeasonEnd {
        season: u16,
        league_position: u8,
        expected_position: u8,
    },
    JobEvent {
        event_type: JobEventType,
    },
}

fn default_board_confidence() -> u8 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonSummary {
    pub season: u16,
    pub league_position: u8,
    pub expected_position: u8,
    pub reputation_before: u16,
    pub reputation_after: u16,
    pub board_verdict: BoardVerdict,
    pub match_record: MatchRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BoardVerdict {
    Satisfied { confidence: u8, contract_extension: bool },
    Neutral { confidence: u8 },
    Warning { confidence: u8 },
    Sacked { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SeasonResolution {
    Continuing { summary: SeasonSummary },
    Sacked { summary: SeasonSummary },
    ContractExpired { summary: SeasonSummary },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub user_manager: Option<UserManager>,
    pub interactive_mode: bool,
    pub pending_decision: Option<DecisionPoint>,
    pub pending_tactics: Option<TacticalChoice>,
    pub pending_lineup: Option<Vec<u32>>,
    pub current_season: u16,
    pub current_date: u16,
    pub transfer_budget: Option<TransferBudget>,
    pub pending_transfer_decisions: HashMap<u32, TransferDecision>,
    pub pending_sale_listings: Vec<SaleListingRequest>,
    pub pending_bids: Vec<TransferBidRequest>,
    #[serde(default)]
    pub career_state: Option<ManagerCareerState>,
    #[serde(default)]
    pub last_season_summary: Option<SeasonSummary>,
    #[serde(default)]
    pub pending_job_offers: Vec<PendingJobOffer>,
    #[serde(default = "default_board_confidence")]
    pub board_confidence: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingJobOffer {
    pub club_id: u32,
    pub club_name: String,
    pub terms: ManagerOfferTerms,
}

impl GameState {
    pub fn new_autonomous() -> Self {
        Self {
            user_manager: None,
            interactive_mode: false,
            pending_decision: None,
            pending_tactics: None,
            pending_lineup: None,
            current_season: 0,
            current_date: 0,
            transfer_budget: None,
            pending_transfer_decisions: HashMap::new(),
            pending_sale_listings: Vec::new(),
            pending_bids: Vec::new(),
            career_state: None,
            last_season_summary: None,
            pending_job_offers: Vec::new(),
            board_confidence: 50,
        }
    }

    pub fn new_interactive() -> Self {
        Self {
            user_manager: None,
            interactive_mode: true,
            pending_decision: None,
            pending_tactics: None,
            pending_lineup: None,
            current_season: 0,
            current_date: 0,
            transfer_budget: None,
            pending_transfer_decisions: HashMap::new(),
            pending_sale_listings: Vec::new(),
            pending_bids: Vec::new(),
            career_state: None,
            last_season_summary: None,
            pending_job_offers: Vec::new(),
            board_confidence: 50,
        }
    }

    pub fn start_career(&mut self, staff_id: u32, club_id: u32, name: String) {
        self.user_manager = Some(UserManager::new(staff_id, club_id, name));
        self.career_state = Some(ManagerCareerState::new(250));
    }

    pub fn set_decision(&mut self, decision: DecisionPoint) {
        self.pending_decision = Some(decision);
    }

    pub fn clear_decision(&mut self) {
        self.pending_decision = None;
    }

    pub fn set_pending_tactics(&mut self, choice: TacticalChoice) {
        self.pending_tactics = Some(choice);
    }

    pub fn take_pending_tactics(&mut self) -> Option<TacticalChoice> {
        self.pending_tactics.take()
    }

    pub fn set_pending_lineup(&mut self, player_ids: Vec<u32>) {
        self.pending_lineup = Some(player_ids);
    }

    pub fn take_pending_lineup(&mut self) -> Option<Vec<u32>> {
        self.pending_lineup.take()
    }

    pub fn resolve_pre_match(&mut self, choice: TacticalChoice) {
        self.set_pending_tactics(choice);
        self.pending_decision = None;
    }

    pub fn resolve_transfer_window(&mut self) {
        self.pending_decision = None;
    }

    pub fn is_waiting_for_input(&self) -> bool {
        self.interactive_mode && self.pending_decision.is_some()
    }

    pub fn career_state(&self) -> Option<&ManagerCareerState> {
        self.career_state.as_ref()
    }

    pub fn career_state_mut(&mut self) -> Option<&mut ManagerCareerState> {
        self.career_state.as_mut()
    }

    pub fn ensure_career_entry(&mut self, club_name: String, season: u16, expected_position: u8) {
        if let Some(ref mut cs) = self.career_state {
            let has_active = cs.history.iter().any(|e| e.is_active());
            if !has_active {
                let entry = ManagerCareerEntry::new(
                    club_name,
                    season,
                    MatchRecord {
                        wins: 0,
                        draws: 0,
                        losses: 0,
                    },
                    expected_position,
                    0,
                    cs.reputation.score(),
                );
                cs.history.push(entry);
            }
        }
    }

    pub fn update_match_record(&mut self, win: bool, draw: bool) {
        if let Some(ref mut cs) = self.career_state {
            if let Some(entry) = cs.history.iter_mut().find(|e| e.is_active()) {
                if win {
                    entry.match_record.wins += 1;
                } else if draw {
                    entry.match_record.draws += 1;
                } else {
                    entry.match_record.losses += 1;
                }
            }
        }
    }

    pub fn resolve_season_end(
        &mut self,
        league_position: u8,
        expected_position: u8,
        trophies_won: u8,
        club_name: String,
    ) -> SeasonResolution {
        let config = ReputationConfig::default();

        let verdict = self.evaluate_board(league_position, expected_position);

        if let Some(ref mut cs) = self.career_state {
            let rep_before = cs.reputation.score();

            let delta_input = SeasonDeltaInput {
                expected_position,
                actual_position: league_position,
                trophies_won,
                board_confidence: self.board_confidence,
                was_sacked: matches!(verdict, BoardVerdict::Sacked { .. }),
                sacking_stage: None,
                youth_graduates: 0,
            };

            let new_score = apply_season_delta(rep_before, &delta_input, &config);
            cs.reputation = ManagerReputation::new(new_score);

            if let Some(ref mut entry) = cs.history.iter_mut().find(|e| e.is_active()) {
                entry.actual_position = league_position;
                entry.expected_position = expected_position;

                let exit_reason = match &verdict {
                    BoardVerdict::Sacked { reason } => {
                        Some(CareerExitReason::Sacked { stage: reason.clone() })
                    }
                    _ => None,
                };

                if let Some(reason) = exit_reason {
                    entry.end_season(self.current_season, reason, new_score);
                } else {
                    entry.reputation_end = new_score;
                }
            }

            let summary = SeasonSummary {
                season: self.current_season,
                league_position,
                expected_position,
                reputation_before: rep_before,
                reputation_after: new_score,
                board_verdict: verdict.clone(),
                match_record: cs
                    .history
                    .iter()
                    .find(|e| e.end_season == Some(self.current_season) || e.is_active())
                    .map(|e| e.match_record.clone())
                    .unwrap_or(MatchRecord {
                        wins: 0,
                        draws: 0,
                        losses: 0,
                    }),
            };

            self.last_season_summary = Some(summary.clone());

            self.current_season += 1;

            match &verdict {
                BoardVerdict::Sacked { .. } => SeasonResolution::Sacked { summary },
                _ => {
                    if let Some(ref mut cs) = self.career_state {
                        let new_entry = ManagerCareerEntry::new(
                            club_name,
                            self.current_season,
                            MatchRecord {
                                wins: 0,
                                draws: 0,
                                losses: 0,
                            },
                            expected_position,
                            0,
                            new_score,
                        );
                        cs.history.push(new_entry);
                    }
                    SeasonResolution::Continuing { summary }
                }
            }
        } else {
            let summary = SeasonSummary {
                season: self.current_season,
                league_position,
                expected_position,
                reputation_before: 0,
                reputation_after: 0,
                board_verdict: BoardVerdict::Neutral { confidence: 50 },
                match_record: MatchRecord {
                    wins: 0,
                    draws: 0,
                    losses: 0,
                },
            };
            self.current_season += 1;
            SeasonResolution::Continuing { summary }
        }
    }

    pub fn evaluate_board(
        &mut self,
        league_position: u8,
        expected_position: u8,
    ) -> BoardVerdict {
        let position_delta = expected_position as i16 - league_position as i16;
        let confidence_adjustment = (position_delta * 5).clamp(-30, 30) as i8;
        let new_confidence = (self.board_confidence as i8 + confidence_adjustment).clamp(0, 100);
        self.board_confidence = new_confidence as u8;

        if self.board_confidence <= 15 {
            BoardVerdict::Sacked {
                reason: "poor_performance".to_string(),
            }
        } else if self.board_confidence <= 35 {
            BoardVerdict::Warning {
                confidence: self.board_confidence,
            }
        } else if self.board_confidence >= 75 {
            BoardVerdict::Satisfied {
                confidence: self.board_confidence,
                contract_extension: true,
            }
        } else {
            BoardVerdict::Neutral {
                confidence: self.board_confidence,
            }
        }
    }

    pub fn sack_user(&mut self, reason: String) {
        if let Some(ref mut cs) = self.career_state {
            if let Some(entry) = cs.history.iter_mut().find(|e| e.is_active()) {
                entry.end_season(
                    self.current_season,
                    CareerExitReason::Sacked { stage: reason.clone() },
                    cs.reputation.score(),
                );
            }
            cs.reputation.subtract(20);
        }
        self.user_manager = None;
    }

    pub fn generate_job_offers(
        &mut self,
        available_clubs: &[(u32, String, u16)],
    ) -> Vec<PendingJobOffer> {
        let offers: Vec<PendingJobOffer> = if let Some(ref cs) = self.career_state {
            available_clubs
                .iter()
                .filter(|(_, _, club_rep)| cs.reputation.allows_approach(&ManagerReputation::new(*club_rep)))
                .map(|(club_id, club_name, club_rep)| {
                    let terms = generate_offer(*club_rep, cs.reputation.score());
                    PendingJobOffer {
                        club_id: *club_id,
                        club_name: club_name.clone(),
                        terms,
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        self.pending_job_offers = offers.clone();
        offers
    }

    pub fn accept_job_offer(&mut self, club_id: u32, staff_id: u32, club_name: String) -> Result<(), String> {
        let offer = self
            .pending_job_offers
            .iter()
            .find(|o| o.club_id == club_id)
            .ok_or("Offer not found")?;

        let terms = offer.terms.clone();

        self.user_manager = Some(UserManager::new(staff_id, club_id, String::new()));

        if let Some(ref mut cs) = self.career_state {
            let entry = ManagerCareerEntry::new(
                club_name,
                self.current_season,
                MatchRecord {
                    wins: 0,
                    draws: 0,
                    losses: 0,
                },
                0,
                0,
                cs.reputation.score(),
            );
            cs.history.push(entry);
        }

        self.board_confidence = 50;
        self.transfer_budget = None;
        self.pending_transfer_decisions.clear();
        self.pending_sale_listings.clear();
        self.pending_bids.clear();
        self.pending_job_offers.clear();
        self.pending_decision = None;

        let _ = terms;
        Ok(())
    }

    pub fn reject_job_offers(&mut self) {
        self.pending_job_offers.clear();
        self.pending_decision = None;
    }

    pub fn refresh_transfer_budget(&mut self, club: &crate::club::Club) {
        use crate::career::transfer::derive_budget_from_club;
        self.transfer_budget = Some(derive_budget_from_club(club));
    }

    pub fn transfer_budget(&self) -> Option<&TransferBudget> {
        self.transfer_budget.as_ref()
    }

    pub fn transfer_budget_mut(&mut self) -> Option<&mut TransferBudget> {
        self.transfer_budget.as_mut()
    }

    pub fn set_transfer_decision(&mut self, negotiation_id: u32, decision: TransferDecision) {
        self.pending_transfer_decisions.insert(negotiation_id, decision);
    }

    pub fn take_transfer_decisions(&mut self) -> HashMap<u32, TransferDecision> {
        self.pending_transfer_decisions.drain().collect()
    }

    pub fn clear_transfer_decisions(&mut self) {
        self.pending_transfer_decisions.clear();
    }

    pub fn add_sale_listing(&mut self, request: SaleListingRequest) {
        self.pending_sale_listings.push(request);
    }

    pub fn take_sale_listings(&mut self) -> Vec<SaleListingRequest> {
        self.pending_sale_listings.drain(..).collect()
    }

    pub fn cancel_sale_listing(&mut self, player_id: u32) -> bool {
        let before = self.pending_sale_listings.len();
        self.pending_sale_listings.retain(|r| r.player_id != player_id);
        self.pending_sale_listings.len() < before
    }

    pub fn add_bid_request(&mut self, bid: TransferBidRequest) -> Result<(), String> {
        validate_bid_request(&bid, self.transfer_budget.as_ref())?;

        let amount = bid.offering_amount as u64;
        if let Some(ref mut budget) = self.transfer_budget {
            if !budget.reserve(amount) {
                return Err("failed to reserve budget".to_string());
            }
        }

        self.pending_bids.push(bid);
        Ok(())
    }

    pub fn take_bid_requests(&mut self) -> Vec<TransferBidRequest> {
        self.pending_bids.drain(..).collect()
    }

    pub fn cancel_bid(&mut self, player_id: u32) -> bool {
        if let Some(pos) = self.pending_bids.iter().position(|b| b.target_player_id == player_id) {
            let bid = self.pending_bids.remove(pos);
            let amount = bid.offering_amount as u64;
            if let Some(ref mut budget) = self.transfer_budget {
                budget.release_reserved(amount);
            }
            true
        } else {
            false
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn check_decision_points(
        &self,
        _user_club_id: u32,
        has_user_match_today: bool,
        fixture_id: u32,
        opponent_name: &str,
        competition_name: &str,
        is_transfer_window_opening: bool,
        is_transfer_window_closing: bool,
        is_season_end: bool,
        league_position: u8,
        expected_position: u8,
        was_sacked: bool,
        sacking_reason: Option<String>,
        contract_expiring: bool,
        has_job_offer: bool,
        offer_club_id: Option<u32>,
        offer_club_name: Option<String>,
    ) -> Option<DecisionPoint> {
        if !self.interactive_mode || self.user_manager.is_none() {
            return None;
        }

        // Priority: JobEvent > PreMatch > TransferWindow > SeasonEnd
        let job = detect_job_event(
            was_sacked,
            sacking_reason,
            contract_expiring,
            has_job_offer,
            offer_club_id,
            offer_club_name,
        );
        if job.is_some() {
            return job;
        }

        if has_user_match_today {
            return Some(DecisionPoint::PreMatch {
                fixture_id,
                opponent: opponent_name.to_string(),
                competition: competition_name.to_string(),
            });
        }

        if is_transfer_window_opening {
            return Some(DecisionPoint::TransferWindow {
                season: self.current_season,
                window_open: true,
            });
        }
        if is_transfer_window_closing {
            return Some(DecisionPoint::TransferWindow {
                season: self.current_season,
                window_open: false,
            });
        }

        if is_season_end {
            return Some(DecisionPoint::SeasonEnd {
                season: self.current_season,
                league_position,
                expected_position,
            });
        }

        None
    }
}

pub fn create_user_manager(staff_id: u32, club_id: u32, name: String) -> UserManager {
    UserManager::new(staff_id, club_id, name)
}

pub fn seed_initial_reputation() -> ManagerReputation {
    ManagerReputation::new(250)
}

pub fn create_initial_career_state() -> ManagerCareerState {
    ManagerCareerState::new(250)
}

pub fn detect_pre_match(
    user_club_id: u32,
    fixture_home_club_id: u32,
    fixture_away_club_id: u32,
    fixture_id: u32,
    opponent_name: String,
    competition_name: String,
) -> Option<DecisionPoint> {
    if user_club_id == fixture_home_club_id || user_club_id == fixture_away_club_id {
        Some(DecisionPoint::PreMatch {
            fixture_id,
            opponent: opponent_name,
            competition: competition_name,
        })
    } else {
        None
    }
}

pub fn detect_transfer_window(
    season: u16,
    current_date: NaiveDate,
    window_open: NaiveDate,
    window_close: NaiveDate,
) -> Option<DecisionPoint> {
    if current_date == window_open {
        Some(DecisionPoint::TransferWindow {
            season,
            window_open: true,
        })
    } else if current_date == window_close {
        Some(DecisionPoint::TransferWindow {
            season,
            window_open: false,
        })
    } else {
        None
    }
}

pub fn detect_season_end(
    season: u16,
    current_date: NaiveDate,
    season_end_date: NaiveDate,
    league_position: u8,
    expected_position: u8,
) -> Option<DecisionPoint> {
    if current_date == season_end_date {
        Some(DecisionPoint::SeasonEnd {
            season,
            league_position,
            expected_position,
        })
    } else {
        None
    }
}

pub fn detect_job_event(
    was_sacked: bool,
    sacking_reason: Option<String>,
    contract_expiring: bool,
    has_job_offer: bool,
    offer_club_id: Option<u32>,
    offer_club_name: Option<String>,
) -> Option<DecisionPoint> {
    if was_sacked {
        Some(DecisionPoint::JobEvent {
            event_type: JobEventType::Sacked {
                reason: sacking_reason.unwrap_or_default(),
            },
        })
    } else if contract_expiring {
        Some(DecisionPoint::JobEvent {
            event_type: JobEventType::ContractExpiring,
        })
    } else if has_job_offer {
        Some(DecisionPoint::JobEvent {
            event_type: JobEventType::JobOffer {
                club_id: offer_club_id.unwrap_or(0),
                club_name: offer_club_name.unwrap_or_default(),
            },
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::career::reputation::ManagerReputationTier;

    #[test]
    fn autonomous_mode_is_not_interactive() {
        let state = GameState::new_autonomous();
        assert!(!state.interactive_mode);
    }

    #[test]
    fn interactive_mode_flag_set() {
        let state = GameState::new_interactive();
        assert!(state.interactive_mode);
    }

    #[test]
    fn start_career_sets_user_manager() {
        let mut state = GameState::new_interactive();
        assert!(state.user_manager.is_none());
        state.start_career(42, 7, "Pep Guardiola".to_string());
        let mgr = state.user_manager.unwrap();
        assert_eq!(mgr.staff_id, 42);
        assert_eq!(mgr.club_id, 7);
        assert_eq!(mgr.manager_name, "Pep Guardiola");
    }

    #[test]
    fn set_and_clear_decision_cycle() {
        let mut state = GameState::new_interactive();
        assert!(state.pending_decision.is_none());
        state.set_decision(DecisionPoint::PreMatch {
            fixture_id: 100,
            opponent: "Barcelona".to_string(),
            competition: "La Liga".to_string(),
        });
        assert!(state.pending_decision.is_some());
        state.clear_decision();
        assert!(state.pending_decision.is_none());
    }

    #[test]
    fn autonomous_never_waits_for_input_even_with_decision() {
        let mut state = GameState::new_autonomous();
        state.set_decision(DecisionPoint::SeasonEnd {
            season: 2025,
            league_position: 1,
            expected_position: 4,
        });
        assert!(!state.is_waiting_for_input());
    }

    #[test]
    fn waiting_for_input_only_when_interactive_with_decision() {
        let mut state = GameState::new_interactive();
        assert!(!state.is_waiting_for_input());
        state.set_decision(DecisionPoint::TransferWindow {
            season: 2025,
            window_open: true,
        });
        assert!(state.is_waiting_for_input());
    }

    #[test]
    fn game_state_serde_roundtrip() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 2, "Test Manager".to_string());
        state.current_season = 2025;
        state.current_date = 15;
        state.set_decision(DecisionPoint::PreMatch {
            fixture_id: 99,
            opponent: "Rivals".to_string(),
            competition: "Cup".to_string(),
        });
        let json = serde_json::to_string(&state).unwrap();
        let restored: GameState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.interactive_mode, state.interactive_mode);
        assert_eq!(restored.current_season, state.current_season);
        assert_eq!(restored.current_date, state.current_date);
        assert!(restored.user_manager.is_some());
        assert!(restored.pending_decision.is_some());
    }

    #[test]
    fn decision_point_pre_match_serde_roundtrip() {
        let dp = DecisionPoint::PreMatch {
            fixture_id: 10,
            opponent: "Team A".to_string(),
            competition: "League".to_string(),
        };
        let json = serde_json::to_string(&dp).unwrap();
        let restored: DecisionPoint = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, DecisionPoint::PreMatch { fixture_id: 10, .. }));
    }

    #[test]
    fn decision_point_transfer_window_serde_roundtrip() {
        let dp = DecisionPoint::TransferWindow {
            season: 2026,
            window_open: false,
        };
        let json = serde_json::to_string(&dp).unwrap();
        let restored: DecisionPoint = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, DecisionPoint::TransferWindow { season: 2026, window_open: false }));
    }

    #[test]
    fn decision_point_season_end_serde_roundtrip() {
        let dp = DecisionPoint::SeasonEnd {
            season: 2025,
            league_position: 3,
            expected_position: 8,
        };
        let json = serde_json::to_string(&dp).unwrap();
        let restored: DecisionPoint = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, DecisionPoint::SeasonEnd { season: 2025, league_position: 3, expected_position: 8 }));
    }

    #[test]
    fn decision_point_job_event_serde_roundtrip() {
        let dp = DecisionPoint::JobEvent {
            event_type: JobEventType::Sacked {
                reason: "Poor results".to_string(),
            },
        };
        let json = serde_json::to_string(&dp).unwrap();
        let restored: DecisionPoint = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, DecisionPoint::JobEvent { .. }));
    }

    #[test]
    fn create_user_manager_constructs_correctly() {
        let mgr = create_user_manager(42, 7, "Pep Guardiola".to_string());
        assert_eq!(mgr.staff_id, 42);
        assert_eq!(mgr.club_id, 7);
        assert_eq!(mgr.manager_name, "Pep Guardiola");
    }

    #[test]
    fn seed_initial_reputation_is_lower_league_tier() {
        let rep = seed_initial_reputation();
        assert_eq!(rep.score(), 250);
        assert_eq!(rep.tier(), ManagerReputationTier::LowerLeague);
    }

    #[test]
    fn create_initial_career_state_has_empty_history() {
        let state = create_initial_career_state();
        assert!(state.history.is_empty());
        assert_eq!(state.reputation.score(), 250);
        assert_eq!(state.reputation.tier(), ManagerReputationTier::LowerLeague);
    }

    #[test]
    fn game_state_start_career_sets_user_manager() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 5, "Test Manager".to_string());
        let mgr = state.user_manager.unwrap();
        assert_eq!(mgr.staff_id, 1);
        assert_eq!(mgr.club_id, 5);
        assert_eq!(mgr.manager_name, "Test Manager");
    }

    #[test]
    fn start_career_overwrites_existing() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 5, "First Manager".to_string());
        state.start_career(2, 10, "Second Manager".to_string());
        let mgr = state.user_manager.unwrap();
        assert_eq!(mgr.staff_id, 2);
        assert_eq!(mgr.club_id, 10);
        assert_eq!(mgr.manager_name, "Second Manager");
    }

    #[test]
    fn start_career_error_variants_exist() {
        let e1 = StartCareerError::ClubNotFound { club_id: 99 };
        let e2 = StartCareerError::ManagerNotFound { staff_id: 42 };
        assert!(matches!(e1, StartCareerError::ClubNotFound { club_id: 99 }));
        assert!(matches!(e2, StartCareerError::ManagerNotFound { staff_id: 42 }));
    }

    #[test]
    fn start_career_error_serde_roundtrip() {
        let e = StartCareerError::ClubNotFound { club_id: 42 };
        let json = serde_json::to_string(&e).unwrap();
        let restored: StartCareerError = serde_json::from_str(&json).unwrap();
        assert_eq!(e, restored);
    }

    #[test]
    fn detect_pre_match_user_is_home() {
        let result = detect_pre_match(
            1,
            1,
            2,
            100,
            "Away Team".to_string(),
            "League".to_string(),
        );
        assert!(result.is_some());
        let dp = result.unwrap();
        assert!(matches!(
            dp,
            DecisionPoint::PreMatch {
                fixture_id: 100,
                opponent: _,
                competition: _
            }
        ));
    }

    #[test]
    fn detect_pre_match_user_is_away() {
        let result = detect_pre_match(
            2,
            1,
            2,
            101,
            "Home Team".to_string(),
            "Cup".to_string(),
        );
        assert!(result.is_some());
        if let DecisionPoint::PreMatch {
            fixture_id,
            opponent,
            competition,
        } = result.unwrap()
        {
            assert_eq!(fixture_id, 101);
            assert_eq!(opponent, "Home Team");
            assert_eq!(competition, "Cup");
        } else {
            panic!("Expected PreMatch");
        }
    }

    #[test]
    fn detect_pre_match_user_not_involved() {
        let result = detect_pre_match(
            99,
            1,
            2,
            100,
            "Team A".to_string(),
            "League".to_string(),
        );
        assert!(result.is_none());
    }

    #[test]
    fn detect_pre_match_returns_opponent_name() {
        let result = detect_pre_match(
            5,
            3,
            5,
            200,
            "Real Madrid".to_string(),
            "Champions League".to_string(),
        );
        let dp = result.unwrap();
        if let DecisionPoint::PreMatch { opponent, .. } = &dp {
            assert_eq!(opponent, "Real Madrid");
        }
    }

    #[test]
    fn detect_transfer_window_on_open() {
        let open = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
        let close = NaiveDate::from_ymd_opt(2025, 8, 31).unwrap();
        let result = detect_transfer_window(2025, open, open, close);
        assert!(result.is_some());
        if let DecisionPoint::TransferWindow { season, window_open } = result.unwrap() {
            assert_eq!(season, 2025);
            assert!(window_open);
        }
    }

    #[test]
    fn detect_transfer_window_on_close() {
        let open = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
        let close = NaiveDate::from_ymd_opt(2025, 8, 31).unwrap();
        let result = detect_transfer_window(2025, close, open, close);
        assert!(result.is_some());
        if let DecisionPoint::TransferWindow { season, window_open } = result.unwrap() {
            assert_eq!(season, 2025);
            assert!(!window_open);
        }
    }

    #[test]
    fn detect_transfer_window_random_date() {
        let open = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
        let close = NaiveDate::from_ymd_opt(2025, 8, 31).unwrap();
        let random = NaiveDate::from_ymd_opt(2025, 7, 15).unwrap();
        let result = detect_transfer_window(2025, random, open, close);
        assert!(result.is_none());
    }

    #[test]
    fn detect_transfer_window_open_equals_close_on_open() {
        let date = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
        let result = detect_transfer_window(2025, date, date, date);
        assert!(result.is_some());
        if let DecisionPoint::TransferWindow { window_open, .. } = result.unwrap() {
            assert!(window_open);
        }
    }

    #[test]
    fn detect_season_end_on_date() {
        let end = NaiveDate::from_ymd_opt(2025, 5, 25).unwrap();
        let result = detect_season_end(2025, end, end, 3, 8);
        assert!(result.is_some());
        if let DecisionPoint::SeasonEnd {
            season,
            league_position,
            expected_position,
        } = result.unwrap()
        {
            assert_eq!(season, 2025);
            assert_eq!(league_position, 3);
            assert_eq!(expected_position, 8);
        }
    }

    #[test]
    fn detect_season_end_not_on_date() {
        let end = NaiveDate::from_ymd_opt(2025, 5, 25).unwrap();
        let current = NaiveDate::from_ymd_opt(2025, 5, 24).unwrap();
        let result = detect_season_end(2025, current, end, 1, 4);
        assert!(result.is_none());
    }

    #[test]
    fn detect_season_end_day_after() {
        let end = NaiveDate::from_ymd_opt(2025, 5, 25).unwrap();
        let current = NaiveDate::from_ymd_opt(2025, 5, 26).unwrap();
        let result = detect_season_end(2025, current, end, 1, 1);
        assert!(result.is_none());
    }

    #[test]
    fn detect_season_end_first_position() {
        let end = NaiveDate::from_ymd_opt(2026, 5, 20).unwrap();
        let result = detect_season_end(2026, end, end, 1, 10);
        if let DecisionPoint::SeasonEnd {
            league_position,
            expected_position,
            ..
        } = result.unwrap()
        {
            assert_eq!(league_position, 1);
            assert_eq!(expected_position, 10);
        }
    }

    #[test]
    fn detect_job_event_sacked() {
        let result = detect_job_event(
            true,
            Some("Poor results".to_string()),
            false,
            false,
            None,
            None,
        );
        assert!(result.is_some());
        if let DecisionPoint::JobEvent { event_type } = result.unwrap() {
            assert!(matches!(event_type, JobEventType::Sacked { .. }));
            if let JobEventType::Sacked { reason } = event_type {
                assert_eq!(reason, "Poor results");
            }
        }
    }

    #[test]
    fn detect_job_event_sacked_without_reason() {
        let result = detect_job_event(true, None, false, false, None, None);
        assert!(result.is_some());
        if let DecisionPoint::JobEvent { event_type } = result.unwrap() {
            if let JobEventType::Sacked { reason } = event_type {
                assert_eq!(reason, "");
            }
        }
    }

    #[test]
    fn detect_job_event_contract_expiring() {
        let result = detect_job_event(false, None, true, false, None, None);
        assert!(result.is_some());
        assert!(matches!(
            result.unwrap(),
            DecisionPoint::JobEvent {
                event_type: JobEventType::ContractExpiring
            }
        ));
    }

    #[test]
    fn detect_job_event_job_offer() {
        let result = detect_job_event(
            false,
            None,
            false,
            true,
            Some(42),
            Some("Barcelona".to_string()),
        );
        assert!(result.is_some());
        if let DecisionPoint::JobEvent { event_type } = result.unwrap() {
            if let JobEventType::JobOffer { club_id, club_name } = event_type {
                assert_eq!(club_id, 42);
                assert_eq!(club_name, "Barcelona");
            }
        }
    }

    #[test]
    fn detect_job_event_sacked_wins_over_contract_expiring() {
        let result = detect_job_event(true, Some("Bad".to_string()), true, false, None, None);
        assert!(matches!(
            result.unwrap(),
            DecisionPoint::JobEvent {
                event_type: JobEventType::Sacked { .. }
            }
        ));
    }

    #[test]
    fn detect_job_event_sacked_wins_over_job_offer() {
        let result = detect_job_event(
            true,
            Some("Bad".to_string()),
            false,
            true,
            Some(10),
            Some("Club".to_string()),
        );
        assert!(matches!(
            result.unwrap(),
            DecisionPoint::JobEvent {
                event_type: JobEventType::Sacked { .. }
            }
        ));
    }

    #[test]
    fn detect_job_event_contract_expiring_wins_over_job_offer() {
        let result = detect_job_event(
            false,
            None,
            true,
            true,
            Some(10),
            Some("Club".to_string()),
        );
        assert!(matches!(
            result.unwrap(),
            DecisionPoint::JobEvent {
                event_type: JobEventType::ContractExpiring
            }
        ));
    }

    #[test]
    fn detect_job_event_none() {
        let result = detect_job_event(false, None, false, false, None, None);
        assert!(result.is_none());
    }

    #[test]
    fn detect_job_event_all_false_returns_none() {
        let result = detect_job_event(
            false,
            None,
            false,
            false,
            Some(99),
            Some("Ghost Club".to_string()),
        );
        assert!(result.is_none());
    }

    #[test]
    fn check_decision_points_returns_none_when_not_interactive() {
        let state = GameState::new_autonomous();
        let result = state.check_decision_points(
            1, true, 100, "Opp", "League",
            false, false, false, 1, 4,
            false, None, false, false, None, None,
        );
        assert!(result.is_none());
    }

    #[test]
    fn check_decision_points_returns_none_when_no_user_manager() {
        let state = GameState::new_interactive();
        let result = state.check_decision_points(
            1, true, 100, "Opp", "League",
            false, false, false, 1, 4,
            false, None, false, false, None, None,
        );
        assert!(result.is_none());
    }

    #[test]
    fn check_decision_points_job_event_highest_priority() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 5, "Manager".to_string());
        let result = state.check_decision_points(
            5, true, 100, "Opp", "League",
            true, false, true, 1, 4,
            true, Some("Bad results".to_string()), true, true, Some(10), Some("Club".to_string()),
        );
        assert!(matches!(
            result,
            Some(DecisionPoint::JobEvent {
                event_type: JobEventType::Sacked { .. }
            })
        ));
    }

    #[test]
    fn check_decision_points_pre_match_second_priority() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 5, "Manager".to_string());
        let result = state.check_decision_points(
            5, true, 200, "Rival", "Cup",
            true, false, true, 1, 4,
            false, None, false, false, None, None,
        );
        if let Some(DecisionPoint::PreMatch { fixture_id, opponent, competition }) = result {
            assert_eq!(fixture_id, 200);
            assert_eq!(opponent, "Rival");
            assert_eq!(competition, "Cup");
        } else {
            panic!("Expected PreMatch");
        }
    }

    #[test]
    fn check_decision_points_transfer_window_opening() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 5, "Manager".to_string());
        let result = state.check_decision_points(
            5, false, 0, "", "",
            true, false, false, 0, 0,
            false, None, false, false, None, None,
        );
        if let Some(DecisionPoint::TransferWindow { window_open, .. }) = result {
            assert!(window_open);
        } else {
            panic!("Expected TransferWindow opening");
        }
    }

    #[test]
    fn check_decision_points_transfer_window_closing() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 5, "Manager".to_string());
        let result = state.check_decision_points(
            5, false, 0, "", "",
            false, true, false, 0, 0,
            false, None, false, false, None, None,
        );
        if let Some(DecisionPoint::TransferWindow { window_open, .. }) = result {
            assert!(!window_open);
        } else {
            panic!("Expected TransferWindow closing");
        }
    }

    #[test]
    fn check_decision_points_season_end_lowest_priority() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 5, "Manager".to_string());
        state.current_season = 2025;
        let result = state.check_decision_points(
            5, false, 0, "", "",
            false, false, true, 3, 8,
            false, None, false, false, None, None,
        );
        if let Some(DecisionPoint::SeasonEnd { season, league_position, expected_position }) = result {
            assert_eq!(season, 2025);
            assert_eq!(league_position, 3);
            assert_eq!(expected_position, 8);
        } else {
            panic!("Expected SeasonEnd");
        }
    }

    #[test]
    fn check_decision_points_no_signals_returns_none() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 5, "Manager".to_string());
        let result = state.check_decision_points(
            5, false, 0, "", "",
            false, false, false, 0, 0,
            false, None, false, false, None, None,
        );
        assert!(result.is_none());
    }

    #[test]
    fn set_and_take_pending_tactics_pair() {
        use crate::club::team::tactics::{MatchTacticType, TacticalStyle};
        let mut state = GameState::new_interactive();
        assert!(state.take_pending_tactics().is_none());

        let choice = TacticalChoice {
            formation: MatchTacticType::T442,
            starting_xi: (1..=11).collect(),
            approach: TacticalStyle::Balanced,
            captain_id: Some(1),
            penalty_taker_id: None,
            free_kick_taker_id: None,
        };
        state.set_pending_tactics(choice);
        assert!(state.pending_tactics.is_some());

        let taken = state.take_pending_tactics();
        assert!(taken.is_some());
        assert_eq!(taken.unwrap().formation, MatchTacticType::T442);
        assert!(state.pending_tactics.is_none());
        assert!(state.take_pending_tactics().is_none());
    }

    #[test]
    fn set_and_take_pending_lineup_pair() {
        let mut state = GameState::new_interactive();
        assert!(state.take_pending_lineup().is_none());

        let lineup: Vec<u32> = (1..=11).collect();
        state.set_pending_lineup(lineup.clone());
        assert!(state.pending_lineup.is_some());

        let taken = state.take_pending_lineup();
        assert_eq!(taken.unwrap(), lineup);
        assert!(state.pending_lineup.is_none());
        assert!(state.take_pending_lineup().is_none());
    }

    #[test]
    fn resolve_pre_match_sets_tactics_and_clears_decision() {
        use crate::club::team::tactics::{MatchTacticType, TacticalStyle};
        let mut state = GameState::new_interactive();
        state.start_career(1, 5, "Manager".to_string());
        state.set_decision(DecisionPoint::PreMatch {
            fixture_id: 42,
            opponent: "Rivals".to_string(),
            competition: "League".to_string(),
        });
        assert!(state.pending_decision.is_some());
        assert!(state.pending_tactics.is_none());

        let choice = TacticalChoice {
            formation: MatchTacticType::T433,
            starting_xi: (1..=11).collect(),
            approach: TacticalStyle::Attacking,
            captain_id: Some(1),
            penalty_taker_id: None,
            free_kick_taker_id: None,
        };
        state.resolve_pre_match(choice);

        assert!(state.pending_decision.is_none());
        let tactics = state.pending_tactics.unwrap();
        assert_eq!(tactics.formation, MatchTacticType::T433);
    }

    #[test]
    fn resolve_pre_match_overwrites_previous_tactics() {
        use crate::club::team::tactics::{MatchTacticType, TacticalStyle};
        let mut state = GameState::new_interactive();
        state.set_decision(DecisionPoint::PreMatch {
            fixture_id: 1,
            opponent: "A".to_string(),
            competition: "B".to_string(),
        });

        let first = TacticalChoice {
            formation: MatchTacticType::T442,
            starting_xi: (1..=11).collect(),
            approach: TacticalStyle::Balanced,
            captain_id: None,
            penalty_taker_id: None,
            free_kick_taker_id: None,
        };
        state.resolve_pre_match(first);
        assert_eq!(state.pending_tactics.as_ref().unwrap().formation, MatchTacticType::T442);

        state.set_decision(DecisionPoint::PreMatch {
            fixture_id: 2,
            opponent: "C".to_string(),
            competition: "D".to_string(),
        });
        let second = TacticalChoice {
            formation: MatchTacticType::T352,
            starting_xi: (1..=11).collect(),
            approach: TacticalStyle::Defensive,
            captain_id: None,
            penalty_taker_id: None,
            free_kick_taker_id: None,
        };
        state.resolve_pre_match(second);
        assert_eq!(state.pending_tactics.as_ref().unwrap().formation, MatchTacticType::T352);
        assert!(state.pending_decision.is_none());
    }

    #[test]
    fn game_state_with_pending_lineup_serde_roundtrip() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 2, "Test".to_string());
        state.set_pending_lineup(vec![10, 20, 30]);
        let json = serde_json::to_string(&state).unwrap();
        let restored: GameState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.pending_lineup, Some(vec![10, 20, 30]));
    }

    #[test]
    fn pending_lineup_preserves_order() {
        let mut state = GameState::new_interactive();
        let lineup = vec![5, 3, 1, 9, 7, 2, 8, 4, 6, 11, 10];
        state.set_pending_lineup(lineup.clone());
        let taken = state.take_pending_lineup().unwrap();
        assert_eq!(taken, lineup);
    }

    #[test]
    fn set_transfer_decision_stores_decision() {
        use crate::career::transfer::TransferDecision;
        let mut state = GameState::new_interactive();
        assert!(state.pending_transfer_decisions.is_empty());

        state.set_transfer_decision(1, TransferDecision::Approve);
        state.set_transfer_decision(2, TransferDecision::Reject);
        assert_eq!(state.pending_transfer_decisions.len(), 2);
        assert!(matches!(
            state.pending_transfer_decisions.get(&1),
            Some(TransferDecision::Approve)
        ));
        assert!(matches!(
            state.pending_transfer_decisions.get(&2),
            Some(TransferDecision::Reject)
        ));
    }

    #[test]
    fn take_transfer_decisions_drains_and_returns() {
        use crate::career::transfer::TransferDecision;
        let mut state = GameState::new_interactive();
        state.set_transfer_decision(10, TransferDecision::Approve);
        state.set_transfer_decision(20, TransferDecision::Reject);

        let drained = state.take_transfer_decisions();
        assert_eq!(drained.len(), 2);
        assert!(state.pending_transfer_decisions.is_empty());

        let second = state.take_transfer_decisions();
        assert!(second.is_empty());
    }

    #[test]
    fn clear_transfer_decisions_removes_all() {
        use crate::career::transfer::TransferDecision;
        let mut state = GameState::new_interactive();
        state.set_transfer_decision(1, TransferDecision::Approve);
        state.set_transfer_decision(2, TransferDecision::Reject);
        state.set_transfer_decision(3, TransferDecision::Approve);
        assert_eq!(state.pending_transfer_decisions.len(), 3);

        state.clear_transfer_decisions();
        assert!(state.pending_transfer_decisions.is_empty());
    }

    #[test]
    fn set_transfer_decision_overwrites_previous() {
        use crate::career::transfer::TransferDecision;
        let mut state = GameState::new_interactive();
        state.set_transfer_decision(5, TransferDecision::Approve);
        state.set_transfer_decision(5, TransferDecision::Reject);
        assert_eq!(state.pending_transfer_decisions.len(), 1);
        assert!(matches!(
            state.pending_transfer_decisions.get(&5),
            Some(TransferDecision::Reject)
        ));
    }

    #[test]
    fn pending_transfer_decisions_serde_roundtrip() {
        use crate::career::transfer::TransferDecision;
        let mut state = GameState::new_interactive();
        state.start_career(1, 2, "Manager".to_string());
        state.set_transfer_decision(100, TransferDecision::Approve);
        state.set_transfer_decision(200, TransferDecision::Reject);
        let json = serde_json::to_string(&state).unwrap();
        let restored: GameState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.pending_transfer_decisions.len(), 2);
        assert!(matches!(
            restored.pending_transfer_decisions.get(&100),
            Some(TransferDecision::Approve)
        ));
        assert!(matches!(
            restored.pending_transfer_decisions.get(&200),
            Some(TransferDecision::Reject)
        ));
    }

    #[test]
    fn add_sale_listing_appends_request() {
        use crate::career::transfer::{SaleListingRequest, SaleListingType};
        let mut state = GameState::new_interactive();
        assert!(state.pending_sale_listings.is_empty());

        state.add_sale_listing(SaleListingRequest {
            player_id: 10,
            asking_price: 5_000_000.0,
            listing_type: SaleListingType::Transfer,
        });
        state.add_sale_listing(SaleListingRequest {
            player_id: 20,
            asking_price: 1_000_000.0,
            listing_type: SaleListingType::Loan,
        });
        assert_eq!(state.pending_sale_listings.len(), 2);
        assert_eq!(state.pending_sale_listings[0].player_id, 10);
        assert_eq!(state.pending_sale_listings[1].player_id, 20);
    }

    #[test]
    fn take_sale_listings_drains_and_returns() {
        use crate::career::transfer::{SaleListingRequest, SaleListingType};
        let mut state = GameState::new_interactive();
        state.add_sale_listing(SaleListingRequest {
            player_id: 5,
            asking_price: 3_000_000.0,
            listing_type: SaleListingType::Transfer,
        });

        let drained = state.take_sale_listings();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].player_id, 5);
        assert!(state.pending_sale_listings.is_empty());

        let second = state.take_sale_listings();
        assert!(second.is_empty());
    }

    #[test]
    fn cancel_sale_listing_removes_by_player_id() {
        use crate::career::transfer::{SaleListingRequest, SaleListingType};
        let mut state = GameState::new_interactive();
        state.add_sale_listing(SaleListingRequest {
            player_id: 10,
            asking_price: 1.0,
            listing_type: SaleListingType::Transfer,
        });
        state.add_sale_listing(SaleListingRequest {
            player_id: 20,
            asking_price: 2.0,
            listing_type: SaleListingType::Loan,
        });

        assert!(state.cancel_sale_listing(10));
        assert_eq!(state.pending_sale_listings.len(), 1);
        assert_eq!(state.pending_sale_listings[0].player_id, 20);
    }

    #[test]
    fn cancel_sale_listing_returns_false_when_not_found() {
        use crate::career::transfer::{SaleListingRequest, SaleListingType};
        let mut state = GameState::new_interactive();
        state.add_sale_listing(SaleListingRequest {
            player_id: 1,
            asking_price: 1.0,
            listing_type: SaleListingType::Transfer,
        });

        assert!(!state.cancel_sale_listing(999));
        assert_eq!(state.pending_sale_listings.len(), 1);
    }

    #[test]
    fn pending_sale_listings_serde_roundtrip() {
        use crate::career::transfer::{SaleListingRequest, SaleListingType};
        let mut state = GameState::new_interactive();
        state.start_career(1, 2, "Manager".to_string());
        state.add_sale_listing(SaleListingRequest {
            player_id: 42,
            asking_price: 10_000_000.0,
            listing_type: SaleListingType::Loan,
        });
        let json = serde_json::to_string(&state).unwrap();
        let restored: GameState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.pending_sale_listings.len(), 1);
        assert_eq!(restored.pending_sale_listings[0].player_id, 42);
        assert_eq!(restored.pending_sale_listings[0].asking_price, 10_000_000.0);
    }

    #[test]
    fn add_bid_request_valid_stores_and_reserves() {
        use crate::career::transfer::{TransferBidRequest, TransferBudget};
        let mut state = GameState::new_interactive();
        state.transfer_budget = Some(TransferBudget {
            total: 1000,
            spent: 0,
            reserved: 0,
            season: 1,
        });

        let bid = TransferBidRequest {
            target_player_id: 10,
            offering_amount: 300.0,
            is_loan: false,
        };
        assert!(state.add_bid_request(bid).is_ok());
        assert_eq!(state.pending_bids.len(), 1);
        assert_eq!(state.transfer_budget.as_ref().unwrap().reserved, 300);
        assert_eq!(state.transfer_budget.as_ref().unwrap().available(), 700);
    }

    #[test]
    fn add_bid_request_over_budget_rejected() {
        use crate::career::transfer::{TransferBidRequest, TransferBudget};
        let mut state = GameState::new_interactive();
        state.transfer_budget = Some(TransferBudget {
            total: 1000,
            spent: 0,
            reserved: 0,
            season: 1,
        });

        let bid = TransferBidRequest {
            target_player_id: 10,
            offering_amount: 1001.0,
            is_loan: false,
        };
        assert!(state.add_bid_request(bid).is_err());
        assert!(state.pending_bids.is_empty());
        assert_eq!(state.transfer_budget.as_ref().unwrap().reserved, 0);
    }

    #[test]
    fn add_bid_request_zero_amount_rejected() {
        use crate::career::transfer::TransferBidRequest;
        let mut state = GameState::new_interactive();
        let bid = TransferBidRequest {
            target_player_id: 10,
            offering_amount: 0.0,
            is_loan: false,
        };
        assert!(state.add_bid_request(bid).is_err());
        assert!(state.pending_bids.is_empty());
    }

    #[test]
    fn add_bid_request_no_budget_stores_bid() {
        use crate::career::transfer::TransferBidRequest;
        let mut state = GameState::new_interactive();
        let bid = TransferBidRequest {
            target_player_id: 10,
            offering_amount: 500.0,
            is_loan: false,
        };
        assert!(state.add_bid_request(bid).is_ok());
        assert_eq!(state.pending_bids.len(), 1);
    }

    #[test]
    fn cancel_bid_releases_budget() {
        use crate::career::transfer::{TransferBidRequest, TransferBudget};
        let mut state = GameState::new_interactive();
        state.transfer_budget = Some(TransferBudget {
            total: 1000,
            spent: 0,
            reserved: 0,
            season: 1,
        });

        let bid = TransferBidRequest {
            target_player_id: 10,
            offering_amount: 300.0,
            is_loan: false,
        };
        state.add_bid_request(bid).unwrap();
        assert_eq!(state.transfer_budget.as_ref().unwrap().reserved, 300);

        assert!(state.cancel_bid(10));
        assert!(state.pending_bids.is_empty());
        assert_eq!(state.transfer_budget.as_ref().unwrap().reserved, 0);
        assert_eq!(state.transfer_budget.as_ref().unwrap().available(), 1000);
    }

    #[test]
    fn cancel_bid_returns_false_when_not_found() {
        use crate::career::transfer::{TransferBidRequest, TransferBudget};
        let mut state = GameState::new_interactive();
        state.transfer_budget = Some(TransferBudget {
            total: 1000,
            spent: 0,
            reserved: 0,
            season: 1,
        });

        let bid = TransferBidRequest {
            target_player_id: 10,
            offering_amount: 100.0,
            is_loan: false,
        };
        state.add_bid_request(bid).unwrap();
        assert!(!state.cancel_bid(999));
        assert_eq!(state.pending_bids.len(), 1);
    }

    #[test]
    fn take_bid_requests_drains() {
        use crate::career::transfer::{TransferBidRequest, TransferBudget};
        let mut state = GameState::new_interactive();
        state.transfer_budget = Some(TransferBudget {
            total: 5000,
            spent: 0,
            reserved: 0,
            season: 1,
        });

        state.add_bid_request(TransferBidRequest {
            target_player_id: 10,
            offering_amount: 500.0,
            is_loan: false,
        }).unwrap();
        state.add_bid_request(TransferBidRequest {
            target_player_id: 20,
            offering_amount: 300.0,
            is_loan: true,
        }).unwrap();

        let drained = state.take_bid_requests();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].target_player_id, 10);
        assert_eq!(drained[1].target_player_id, 20);
        assert!(state.pending_bids.is_empty());

        let second = state.take_bid_requests();
        assert!(second.is_empty());
    }

    #[test]
    fn pending_bids_serde_roundtrip() {
        use crate::career::transfer::{TransferBidRequest, TransferBudget};
        let mut state = GameState::new_interactive();
        state.start_career(1, 2, "Manager".to_string());
        state.transfer_budget = Some(TransferBudget {
            total: 5000,
            spent: 0,
            reserved: 0,
            season: 1,
        });
        state.add_bid_request(TransferBidRequest {
            target_player_id: 42,
            offering_amount: 1000.0,
            is_loan: false,
        }).unwrap();

        let json = serde_json::to_string(&state).unwrap();
        let restored: GameState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.pending_bids.len(), 1);
        assert_eq!(restored.pending_bids[0].target_player_id, 42);
        assert_eq!(restored.pending_bids[0].offering_amount, 1000.0);
    }

    #[test]
    fn start_career_initializes_career_state() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 10, "Test Manager".to_string());
        assert!(state.career_state.is_some());
        assert_eq!(state.career_state.unwrap().reputation.score(), 250);
    }

    #[test]
    fn ensure_career_entry_creates_first_entry() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 10, "Test".to_string());
        state.ensure_career_entry("Arsenal".to_string(), 2025, 4);
        let cs = state.career_state.unwrap();
        assert_eq!(cs.history.len(), 1);
        assert!(cs.history[0].is_active());
        assert_eq!(cs.history[0].club_name, "Arsenal");
    }

    #[test]
    fn ensure_career_entry_no_duplicate_active() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 10, "Test".to_string());
        state.ensure_career_entry("Arsenal".to_string(), 2025, 4);
        state.ensure_career_entry("Arsenal".to_string(), 2025, 4);
        let cs = state.career_state.unwrap();
        assert_eq!(cs.history.len(), 1);
    }

    #[test]
    fn resolve_season_end_continuing() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 10, "Test".to_string());
        state.ensure_career_entry("Arsenal".to_string(), 2025, 4);
        state.current_season = 2025;
        state.board_confidence = 60;

        let result = state.resolve_season_end(3, 4, 0, "Arsenal".to_string());

        match result {
            SeasonResolution::Continuing { summary } => {
                assert_eq!(summary.season, 2025);
                assert_eq!(summary.league_position, 3);
                assert!(summary.reputation_after >= summary.reputation_before);
            }
            _ => panic!("Expected Continuing"),
        }
        assert_eq!(state.current_season, 2026);
        assert!(state.last_season_summary.is_some());
    }

    #[test]
    fn resolve_season_end_sacked() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 10, "Test".to_string());
        state.ensure_career_entry("Arsenal".to_string(), 2025, 2);
        state.current_season = 2025;
        state.board_confidence = 10;

        let result = state.resolve_season_end(15, 2, 0, "Arsenal".to_string());

        match result {
            SeasonResolution::Sacked { summary } => {
                assert!(matches!(summary.board_verdict, BoardVerdict::Sacked { .. }));
            }
            _ => panic!("Expected Sacked"),
        }
    }

    #[test]
    fn board_verdict_satisfied() {
        let mut state = GameState::new_interactive();
        state.board_confidence = 80;
        let verdict = state.evaluate_board(1, 5);
        assert!(matches!(verdict, BoardVerdict::Satisfied { confidence: _, contract_extension: true }));
    }

    #[test]
    fn board_verdict_sacked() {
        let mut state = GameState::new_interactive();
        state.board_confidence = 10;
        let verdict = state.evaluate_board(20, 5);
        assert!(matches!(verdict, BoardVerdict::Sacked { .. }));
    }

    #[test]
    fn board_verdict_warning() {
        let mut state = GameState::new_interactive();
        state.board_confidence = 40;
        let verdict = state.evaluate_board(8, 5);
        assert!(matches!(verdict, BoardVerdict::Warning { .. }));
    }

    #[test]
    fn board_verdict_neutral() {
        let mut state = GameState::new_interactive();
        state.board_confidence = 55;
        let verdict = state.evaluate_board(5, 5);
        assert!(matches!(verdict, BoardVerdict::Neutral { .. }));
    }

    #[test]
    fn board_confidence_updates() {
        let mut state = GameState::new_interactive();
        state.board_confidence = 50;
        state.evaluate_board(1, 10);
        assert!(state.board_confidence > 50);

        state.board_confidence = 50;
        state.evaluate_board(15, 5);
        assert!(state.board_confidence < 50);
    }

    #[test]
    fn sack_user_clears_manager() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 10, "Test".to_string());
        state.sack_user("poor_results".to_string());
        assert!(state.user_manager.is_none());
    }

    #[test]
    fn generate_job_offers_filters_by_reputation() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 10, "Test".to_string());

        let clubs = vec![
            (1, "Similar Club".to_string(), 200u16),
            (2, "Adjacent Club".to_string(), 500u16),
            (3, "Elite Club".to_string(), 900u16),
        ];

        let offers = state.generate_job_offers(&clubs);
        assert!(offers.iter().any(|o| o.club_name == "Similar Club"));
        assert!(offers.iter().any(|o| o.club_name == "Adjacent Club"));
        assert!(!offers.iter().any(|o| o.club_name == "Elite Club"));
    }

    #[test]
    fn accept_job_offer_switches_club() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 10, "Test".to_string());
        state.current_season = 2025;

        let clubs = vec![(20, "New Club".to_string(), 200u16)];
        state.generate_job_offers(&clubs);

        state.accept_job_offer(20, 5, "New Club".to_string()).unwrap();

        let mgr = state.user_manager.unwrap();
        assert_eq!(mgr.club_id, 20);
        assert!(state.pending_job_offers.is_empty());
    }

    #[test]
    fn reject_job_offers_clears() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 10, "Test".to_string());

        let clubs = vec![(20, "New Club".to_string(), 200u16)];
        state.generate_job_offers(&clubs);
        assert_eq!(state.pending_job_offers.len(), 1);

        state.reject_job_offers();
        assert!(state.pending_job_offers.is_empty());
    }

    #[test]
    fn game_state_with_career_serde_roundtrip() {
        let mut state = GameState::new_interactive();
        state.start_career(1, 10, "Test".to_string());
        state.ensure_career_entry("Arsenal".to_string(), 2025, 4);
        state.current_season = 2025;
        state.board_confidence = 65;

        let json = serde_json::to_string(&state).unwrap();
        let restored: GameState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.career_state.unwrap().reputation.score(), 250);
        assert_eq!(restored.board_confidence, 65);
        assert!(restored.pending_job_offers.is_empty());
    }

    #[test]
    fn old_game_state_deserializes_with_new_defaults() {
        let old_json = r#"{
            "user_manager": null,
            "interactive_mode": false,
            "pending_decision": null,
            "pending_tactics": null,
            "pending_lineup": null,
            "current_season": 0,
            "current_date": 0,
            "transfer_budget": null,
            "pending_transfer_decisions": {},
            "pending_sale_listings": [],
            "pending_bids": []
        }"#;
        let state: GameState = serde_json::from_str(old_json).unwrap();
        assert!(!state.interactive_mode);
        assert!(state.career_state.is_none());
        assert!(state.pending_job_offers.is_empty());
        assert_eq!(state.board_confidence, 50);
    }
}
