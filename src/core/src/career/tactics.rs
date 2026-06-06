use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::club::team::tactics::{MatchTacticType, TacticalStyle, TacticSelectionReason, Tactics};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TacticalChoice {
    pub formation: MatchTacticType,
    pub starting_xi: Vec<u32>,
    pub approach: TacticalStyle,
    pub captain_id: Option<u32>,
    pub penalty_taker_id: Option<u32>,
    pub free_kick_taker_id: Option<u32>,
}

impl TacticalChoice {
    pub fn to_tactics(&self) -> Tactics {
        Tactics::with_style_override(
            self.formation,
            TacticSelectionReason::CoachPreference,
            0.8,
            self.approach.clone(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ValidationError {
    WrongPlayerCount { expected: usize, actual: usize },
    DuplicatePlayers,
    PlayerNotInSquad { player_id: u32 },
    InvalidFormation,
    CaptainNotInStartingXI,
    SetPieceTakerNotInStartingXI,
}

pub fn validate_tactical_choice(
    choice: &TacticalChoice,
    squad_player_ids: &[u32],
) -> Result<(), Vec<ValidationError>> {
    let mut errors = Vec::new();

    if choice.starting_xi.len() != 11 {
        errors.push(ValidationError::WrongPlayerCount {
            expected: 11,
            actual: choice.starting_xi.len(),
        });
    }

    let unique: HashSet<u32> = choice.starting_xi.iter().copied().collect();
    if unique.len() != choice.starting_xi.len() {
        errors.push(ValidationError::DuplicatePlayers);
    }

    let squad_set: HashSet<u32> = squad_player_ids.iter().copied().collect();
    for &id in &choice.starting_xi {
        if !squad_set.contains(&id) {
            errors.push(ValidationError::PlayerNotInSquad { player_id: id });
        }
    }

    let xi_set: HashSet<u32> = choice.starting_xi.iter().copied().collect();
    if let Some(captain) = choice.captain_id
        && !xi_set.contains(&captain)
    {
        errors.push(ValidationError::CaptainNotInStartingXI);
    }

    let check_set_piece = |taker_id: Option<u32>, errors: &mut Vec<ValidationError>| {
        if let Some(id) = taker_id
            && !xi_set.contains(&id)
        {
            errors.push(ValidationError::SetPieceTakerNotInStartingXI);
        }
    };
    check_set_piece(choice.penalty_taker_id, &mut errors);
    check_set_piece(choice.free_kick_taker_id, &mut errors);

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn squad() -> Vec<u32> {
        (1..=25).collect()
    }

    fn valid_starting_xi() -> Vec<u32> {
        (1..=11).collect()
    }

    fn valid_choice() -> TacticalChoice {
        TacticalChoice {
            formation: MatchTacticType::T442,
            starting_xi: valid_starting_xi(),
            approach: TacticalStyle::Balanced,
            captain_id: Some(1),
            penalty_taker_id: Some(10),
            free_kick_taker_id: Some(7),
        }
    }

    #[test]
    fn valid_choice_passes() {
        let choice = valid_choice();
        assert!(validate_tactical_choice(&choice, &squad()).is_ok());
    }

    #[test]
    fn wrong_player_count_too_few() {
        let mut choice = valid_choice();
        choice.starting_xi = (1..=10).collect();
        let result = validate_tactical_choice(&choice, &squad());
        let errors = result.unwrap_err();
        assert!(errors.contains(&ValidationError::WrongPlayerCount {
            expected: 11,
            actual: 10,
        }));
    }

    #[test]
    fn wrong_player_count_too_many() {
        let mut choice = valid_choice();
        choice.starting_xi = (1..=12).collect();
        let result = validate_tactical_choice(&choice, &squad());
        let errors = result.unwrap_err();
        assert!(errors.contains(&ValidationError::WrongPlayerCount {
            expected: 11,
            actual: 12,
        }));
    }

    #[test]
    fn duplicate_players() {
        let mut choice = valid_choice();
        choice.starting_xi = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 10];
        let result = validate_tactical_choice(&choice, &squad());
        let errors = result.unwrap_err();
        assert!(errors.contains(&ValidationError::DuplicatePlayers));
    }

    #[test]
    fn player_not_in_squad() {
        let mut choice = valid_choice();
        choice.starting_xi = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 99];
        let result = validate_tactical_choice(&choice, &squad());
        let errors = result.unwrap_err();
        assert!(errors.contains(&ValidationError::PlayerNotInSquad { player_id: 99 }));
    }

    #[test]
    fn captain_not_in_starting_xi() {
        let mut choice = valid_choice();
        choice.captain_id = Some(20);
        let result = validate_tactical_choice(&choice, &squad());
        let errors = result.unwrap_err();
        assert!(errors.contains(&ValidationError::CaptainNotInStartingXI));
    }

    #[test]
    fn penalty_taker_not_in_starting_xi() {
        let mut choice = valid_choice();
        choice.penalty_taker_id = Some(20);
        let result = validate_tactical_choice(&choice, &squad());
        let errors = result.unwrap_err();
        assert!(errors.contains(&ValidationError::SetPieceTakerNotInStartingXI));
    }

    #[test]
    fn free_kick_taker_not_in_starting_xi() {
        let mut choice = valid_choice();
        choice.free_kick_taker_id = Some(20);
        let result = validate_tactical_choice(&choice, &squad());
        let errors = result.unwrap_err();
        assert!(errors.contains(&ValidationError::SetPieceTakerNotInStartingXI));
    }

    #[test]
    fn multiple_errors_at_once() {
        let choice = TacticalChoice {
            formation: MatchTacticType::T433,
            starting_xi: (1..=10).collect(),
            approach: TacticalStyle::Attacking,
            captain_id: Some(99),
            penalty_taker_id: Some(99),
            free_kick_taker_id: None,
        };
        let result = validate_tactical_choice(&choice, &squad());
        let errors = result.unwrap_err();
        assert!(errors.contains(&ValidationError::WrongPlayerCount {
            expected: 11,
            actual: 10,
        }));
        assert!(errors.contains(&ValidationError::CaptainNotInStartingXI));
        assert!(errors.contains(&ValidationError::SetPieceTakerNotInStartingXI));
        assert_eq!(errors.len(), 3);
    }

    #[test]
    fn captain_and_set_piece_in_xi_passes() {
        let choice = TacticalChoice {
            formation: MatchTacticType::T442,
            starting_xi: valid_starting_xi(),
            approach: TacticalStyle::Defensive,
            captain_id: Some(1),
            penalty_taker_id: Some(5),
            free_kick_taker_id: Some(7),
        };
        assert!(validate_tactical_choice(&choice, &squad()).is_ok());
    }

    #[test]
    fn no_captain_or_set_piece_passes() {
        let choice = TacticalChoice {
            formation: MatchTacticType::T442,
            starting_xi: valid_starting_xi(),
            approach: TacticalStyle::Balanced,
            captain_id: None,
            penalty_taker_id: None,
            free_kick_taker_id: None,
        };
        assert!(validate_tactical_choice(&choice, &squad()).is_ok());
    }

    #[test]
    fn serialize_deserialize_roundtrip() {
        let choice = valid_choice();
        let json = serde_json::to_string(&choice).unwrap();
        let deserialized: TacticalChoice = serde_json::from_str(&json).unwrap();
        assert_eq!(choice.starting_xi, deserialized.starting_xi);
        assert_eq!(choice.captain_id, deserialized.captain_id);
        assert_eq!(choice.penalty_taker_id, deserialized.penalty_taker_id);
        assert_eq!(choice.free_kick_taker_id, deserialized.free_kick_taker_id);
    }

    #[test]
    fn to_tactics_formation_442() {
        let choice = TacticalChoice {
            formation: MatchTacticType::T442,
            starting_xi: valid_starting_xi(),
            approach: TacticalStyle::Balanced,
            captain_id: None,
            penalty_taker_id: None,
            free_kick_taker_id: None,
        };
        let tactics = choice.to_tactics();
        assert_eq!(tactics.tactic_type, MatchTacticType::T442);
        assert_eq!(tactics.selected_reason, TacticSelectionReason::CoachPreference);
        assert!((tactics.formation_strength - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn to_tactics_formation_433() {
        let choice = TacticalChoice {
            formation: MatchTacticType::T433,
            starting_xi: valid_starting_xi(),
            approach: TacticalStyle::Attacking,
            captain_id: None,
            penalty_taker_id: None,
            free_kick_taker_id: None,
        };
        let tactics = choice.to_tactics();
        assert_eq!(tactics.tactic_type, MatchTacticType::T433);
    }

    #[test]
    fn to_tactics_positions_match_formation() {
        let choice = TacticalChoice {
            formation: MatchTacticType::T433,
            starting_xi: valid_starting_xi(),
            approach: TacticalStyle::Balanced,
            captain_id: None,
            penalty_taker_id: None,
            free_kick_taker_id: None,
        };
        let tactics = choice.to_tactics();
        let positions = tactics.positions();
        assert_eq!(positions.len(), 11);
        assert!(matches!(positions[0], crate::club::PlayerPositionType::Goalkeeper));
    }

    #[test]
    fn to_tactics_strength_clamped() {
        let choice = TacticalChoice {
            formation: MatchTacticType::T352,
            starting_xi: valid_starting_xi(),
            approach: TacticalStyle::Defensive,
            captain_id: None,
            penalty_taker_id: None,
            free_kick_taker_id: None,
        };
        let tactics = choice.to_tactics();
        assert!(tactics.formation_strength >= 0.0 && tactics.formation_strength <= 1.0);
    }

    #[test]
    fn to_tactics_propagates_approach() {
        let choice = TacticalChoice {
            formation: MatchTacticType::T433,
            starting_xi: valid_starting_xi(),
            approach: TacticalStyle::Defensive,
            captain_id: None,
            penalty_taker_id: None,
            free_kick_taker_id: None,
        };
        let tactics = choice.to_tactics();
        assert_eq!(tactics.tactic_type, MatchTacticType::T433);
        assert_eq!(tactics.tactical_style(), TacticalStyle::Defensive);
    }
}
