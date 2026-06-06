use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ManagerReputationTier {
    SundayLeague,
    SemiPro,
    LowerLeague,
    Professional,
    Continental,
    WorldClass,
}

impl ManagerReputationTier {
    pub fn from_score(score: u16) -> Self {
        match score {
            0..=99 => Self::SundayLeague,
            100..=249 => Self::SemiPro,
            250..=499 => Self::LowerLeague,
            500..=699 => Self::Professional,
            700..=849 => Self::Continental,
            850..=1000 => Self::WorldClass,
            _ => Self::WorldClass,
        }
    }

    pub fn ordinal(self) -> i8 {
        match self {
            Self::SundayLeague => 0,
            Self::SemiPro => 1,
            Self::LowerLeague => 2,
            Self::Professional => 3,
            Self::Continental => 4,
            Self::WorldClass => 5,
        }
    }

    pub fn allows_approach(self, other: &Self) -> bool {
        (self.ordinal() - other.ordinal()).abs() <= 1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManagerReputation {
    score: u16,
}

impl ManagerReputation {
    pub fn new(score: u16) -> Self {
        Self {
            score: score.min(1000),
        }
    }

    pub fn score(&self) -> u16 {
        self.score
    }

    pub fn add(&mut self, amount: u16) {
        self.score = self.score.saturating_add(amount).min(1000);
    }

    pub fn subtract(&mut self, amount: u16) {
        self.score = self.score.saturating_sub(amount);
    }

    pub fn tier(&self) -> ManagerReputationTier {
        ManagerReputationTier::from_score(self.score)
    }

    pub fn allows_approach(&self, other: &Self) -> bool {
        self.tier().allows_approach(&other.tier())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReputationConfig {
    pub position_delta_weight: i16,
    pub trophy_bonus: i16,
    pub board_confidence_high_threshold: u8,
    pub board_confidence_high_bonus: i16,
    pub board_confidence_low_threshold: u8,
    pub board_confidence_low_penalty: i16,
    pub sacking_penalty: i16,
    pub overachievement_threshold: i8,
    pub overachievement_bonus: i16,
    pub youth_development_credit: i16,
    pub youth_development_max_graduates: u8,
}

impl Default for ReputationConfig {
    fn default() -> Self {
        Self {
            position_delta_weight: 12,
            trophy_bonus: 50,
            board_confidence_high_threshold: 75,
            board_confidence_high_bonus: 5,
            board_confidence_low_threshold: 25,
            board_confidence_low_penalty: 5,
            sacking_penalty: 20,
            overachievement_threshold: 3,
            overachievement_bonus: 8,
            youth_development_credit: 3,
            youth_development_max_graduates: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeasonDeltaInput {
    pub expected_position: u8,
    pub actual_position: u8,
    pub trophies_won: u8,
    pub board_confidence: u8,
    pub was_sacked: bool,
    pub sacking_stage: Option<String>,
    pub youth_graduates: u8,
}

pub fn apply_season_delta(
    current_score: u16,
    input: &SeasonDeltaInput,
    config: &ReputationConfig,
) -> u16 {
    let position_delta =
        config.position_delta_weight * (input.expected_position as i16 - input.actual_position as i16);
    let trophy_total = config.trophy_bonus * input.trophies_won as i16;
    let board_bonus = if input.board_confidence >= config.board_confidence_high_threshold {
        config.board_confidence_high_bonus
    } else if input.board_confidence <= config.board_confidence_low_threshold {
        -config.board_confidence_low_penalty
    } else {
        0
    };
    let sacking_total = if input.was_sacked {
        -config.sacking_penalty
    } else {
        0
    };
    let overachievement =
        if (input.expected_position as i8 - input.actual_position as i8) >= config.overachievement_threshold {
            config.overachievement_bonus
        } else {
            0
        };
    let capped_graduates = input.youth_graduates.min(config.youth_development_max_graduates);
    let youth_total = config.youth_development_credit * capped_graduates as i16;

    let new_score = current_score as i16
        + position_delta
        + trophy_total
        + board_bonus
        + sacking_total
        + overachievement
        + youth_total;

    new_score.clamp(0, 1000) as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_boundaries() {
        assert_eq!(ManagerReputationTier::from_score(0), ManagerReputationTier::SundayLeague);
        assert_eq!(ManagerReputationTier::from_score(99), ManagerReputationTier::SundayLeague);
        assert_eq!(ManagerReputationTier::from_score(100), ManagerReputationTier::SemiPro);
        assert_eq!(ManagerReputationTier::from_score(249), ManagerReputationTier::SemiPro);
        assert_eq!(ManagerReputationTier::from_score(250), ManagerReputationTier::LowerLeague);
        assert_eq!(ManagerReputationTier::from_score(499), ManagerReputationTier::LowerLeague);
        assert_eq!(ManagerReputationTier::from_score(500), ManagerReputationTier::Professional);
        assert_eq!(ManagerReputationTier::from_score(699), ManagerReputationTier::Professional);
        assert_eq!(ManagerReputationTier::from_score(700), ManagerReputationTier::Continental);
        assert_eq!(ManagerReputationTier::from_score(849), ManagerReputationTier::Continental);
        assert_eq!(ManagerReputationTier::from_score(850), ManagerReputationTier::WorldClass);
        assert_eq!(ManagerReputationTier::from_score(1000), ManagerReputationTier::WorldClass);
    }

    #[test]
    fn reputation_clamps_score() {
        assert_eq!(ManagerReputation::new(0).score(), 0);
        assert_eq!(ManagerReputation::new(500).score(), 500);
        assert_eq!(ManagerReputation::new(1000).score(), 1000);
        assert_eq!(ManagerReputation::new(1500).score(), 1000);
    }

    #[test]
    fn allows_approach_same_tier() {
        let a = ManagerReputation::new(50);
        let b = ManagerReputation::new(80);
        assert!(a.allows_approach(&b));
    }

    #[test]
    fn allows_approach_adjacent_tier() {
        let a = ManagerReputation::new(99);
        let b = ManagerReputation::new(100);
        assert!(a.allows_approach(&b));
    }

    #[test]
    fn disallows_approach_two_tiers_apart() {
        let a = ManagerReputation::new(50);
        let b = ManagerReputation::new(250);
        assert!(!a.allows_approach(&b));
    }

    #[test]
    fn season_delta_overachievement() {
        let input = SeasonDeltaInput {
            expected_position: 10,
            actual_position: 5,
            trophies_won: 0,
            board_confidence: 50,
            was_sacked: false,
            sacking_stage: None,
            youth_graduates: 0,
        };
        let config = ReputationConfig::default();
        let result = apply_season_delta(500, &input, &config);
        let expected_delta = 12 * (10 - 5) + 8;
        assert_eq!(result, 500 + expected_delta as u16);
    }

    #[test]
    fn season_delta_sacking_penalty() {
        let input = SeasonDeltaInput {
            expected_position: 10,
            actual_position: 12,
            trophies_won: 0,
            board_confidence: 20,
            was_sacked: true,
            sacking_stage: Some("mid_season".to_string()),
            youth_graduates: 0,
        };
        let config = ReputationConfig::default();
        let result = apply_season_delta(300, &input, &config);
        let expected_delta: i16 = 12 * (10 - 12) + (-5) + (-20);
        assert_eq!(result, (300i16 + expected_delta).clamp(0, 1000) as u16);
    }

    #[test]
    fn season_delta_clamps_to_zero() {
        let input = SeasonDeltaInput {
            expected_position: 1,
            actual_position: 20,
            trophies_won: 0,
            board_confidence: 10,
            was_sacked: true,
            sacking_stage: Some("early_season".to_string()),
            youth_graduates: 0,
        };
        let config = ReputationConfig::default();
        let result = apply_season_delta(10, &input, &config);
        assert_eq!(result, 0);
    }

    #[test]
    fn season_delta_clamps_to_1000() {
        let input = SeasonDeltaInput {
            expected_position: 20,
            actual_position: 1,
            trophies_won: 5,
            board_confidence: 90,
            was_sacked: false,
            sacking_stage: None,
            youth_graduates: 10,
        };
        let config = ReputationConfig::default();
        let result = apply_season_delta(990, &input, &config);
        assert_eq!(result, 1000);
    }

    #[test]
    fn season_delta_youth_capped() {
        let input = SeasonDeltaInput {
            expected_position: 5,
            actual_position: 5,
            trophies_won: 0,
            board_confidence: 50,
            was_sacked: false,
            sacking_stage: None,
            youth_graduates: 10,
        };
        let config = ReputationConfig::default();
        let result = apply_season_delta(500, &input, &config);
        assert_eq!(result, 500 + 3 * 5);
    }

    #[test]
    fn config_default_values() {
        let c = ReputationConfig::default();
        assert_eq!(c.position_delta_weight, 12);
        assert_eq!(c.trophy_bonus, 50);
        assert_eq!(c.board_confidence_high_threshold, 75);
        assert_eq!(c.board_confidence_high_bonus, 5);
        assert_eq!(c.board_confidence_low_threshold, 25);
        assert_eq!(c.board_confidence_low_penalty, 5);
        assert_eq!(c.sacking_penalty, 20);
        assert_eq!(c.overachievement_threshold, 3);
        assert_eq!(c.overachievement_bonus, 8);
        assert_eq!(c.youth_development_credit, 3);
        assert_eq!(c.youth_development_max_graduates, 5);
    }

    #[test]
    fn season_delta_trophy_bonus() {
        let input = SeasonDeltaInput {
            expected_position: 5,
            actual_position: 5,
            trophies_won: 2,
            board_confidence: 50,
            was_sacked: false,
            sacking_stage: None,
            youth_graduates: 0,
        };
        let config = ReputationConfig::default();
        let result = apply_season_delta(500, &input, &config);
        assert_eq!(result, 500 + 50 * 2);
    }

    #[test]
    fn season_delta_board_confidence_high_bonus() {
        let input = SeasonDeltaInput {
            expected_position: 5,
            actual_position: 5,
            trophies_won: 0,
            board_confidence: 80,
            was_sacked: false,
            sacking_stage: None,
            youth_graduates: 0,
        };
        let config = ReputationConfig::default();
        let result = apply_season_delta(500, &input, &config);
        assert_eq!(result, 500 + 5);
    }

    #[test]
    fn season_delta_board_confidence_low_penalty() {
        let input = SeasonDeltaInput {
            expected_position: 5,
            actual_position: 5,
            trophies_won: 0,
            board_confidence: 20,
            was_sacked: false,
            sacking_stage: None,
            youth_graduates: 0,
        };
        let config = ReputationConfig::default();
        let result = apply_season_delta(500, &input, &config);
        assert_eq!(result, 500 - 5);
    }

    #[test]
    fn season_delta_board_confidence_midrange_no_effect() {
        let input = SeasonDeltaInput {
            expected_position: 5,
            actual_position: 5,
            trophies_won: 0,
            board_confidence: 50,
            was_sacked: false,
            sacking_stage: None,
            youth_graduates: 0,
        };
        let config = ReputationConfig::default();
        let result = apply_season_delta(500, &input, &config);
        assert_eq!(result, 500);
    }

    #[test]
    fn season_delta_youth_graduates_partial_cap() {
        let input = SeasonDeltaInput {
            expected_position: 5,
            actual_position: 5,
            trophies_won: 0,
            board_confidence: 50,
            was_sacked: false,
            sacking_stage: None,
            youth_graduates: 3,
        };
        let config = ReputationConfig::default();
        let result = apply_season_delta(500, &input, &config);
        assert_eq!(result, 500 + 3 * 3);
    }

    #[test]
    fn custom_config_override() {
        let custom = ReputationConfig {
            position_delta_weight: 20,
            trophy_bonus: 100,
            board_confidence_high_threshold: 80,
            board_confidence_high_bonus: 10,
            board_confidence_low_threshold: 30,
            board_confidence_low_penalty: 15,
            sacking_penalty: 50,
            overachievement_threshold: 2,
            overachievement_bonus: 15,
            youth_development_credit: 5,
            youth_development_max_graduates: 3,
        };
        let input = SeasonDeltaInput {
            expected_position: 10,
            actual_position: 1,
            trophies_won: 1,
            board_confidence: 85,
            was_sacked: false,
            sacking_stage: None,
            youth_graduates: 5,
        };
        let result = apply_season_delta(400, &input, &custom);
        let position_delta = 20 * (10 - 1);
        let trophy = 100 * 1;
        let board = 10;
        let overachievement = 15;
        let youth = 5 * 3;
        let expected = 400 + position_delta + trophy + board + overachievement + youth;
        assert_eq!(result, expected as u16);
    }

    #[test]
    fn allows_approach_minus_one_tier() {
        let higher = ManagerReputation::new(100);
        let lower = ManagerReputation::new(50);
        assert!(higher.allows_approach(&lower));
    }

    #[test]
    fn tier_exact_boundary_values() {
        assert_eq!(ManagerReputationTier::from_score(100), ManagerReputationTier::SemiPro);
        assert_eq!(ManagerReputationTier::from_score(250), ManagerReputationTier::LowerLeague);
        assert_eq!(ManagerReputationTier::from_score(500), ManagerReputationTier::Professional);
        assert_eq!(ManagerReputationTier::from_score(700), ManagerReputationTier::Continental);
        assert_eq!(ManagerReputationTier::from_score(850), ManagerReputationTier::WorldClass);
    }

    #[test]
    fn tier_ordinal_ordering() {
        assert!(ManagerReputationTier::SundayLeague.ordinal() < ManagerReputationTier::SemiPro.ordinal());
        assert!(ManagerReputationTier::SemiPro.ordinal() < ManagerReputationTier::LowerLeague.ordinal());
        assert!(ManagerReputationTier::LowerLeague.ordinal() < ManagerReputationTier::Professional.ordinal());
        assert!(ManagerReputationTier::Professional.ordinal() < ManagerReputationTier::Continental.ordinal());
        assert!(ManagerReputationTier::Continental.ordinal() < ManagerReputationTier::WorldClass.ordinal());
    }

    #[test]
    fn reputation_tier_method() {
        assert_eq!(ManagerReputation::new(50).tier(), ManagerReputationTier::SundayLeague);
        assert_eq!(ManagerReputation::new(150).tier(), ManagerReputationTier::SemiPro);
        assert_eq!(ManagerReputation::new(350).tier(), ManagerReputationTier::LowerLeague);
        assert_eq!(ManagerReputation::new(600).tier(), ManagerReputationTier::Professional);
        assert_eq!(ManagerReputation::new(775).tier(), ManagerReputationTier::Continental);
        assert_eq!(ManagerReputation::new(950).tier(), ManagerReputationTier::WorldClass);
    }

    #[test]
    fn reputation_score_above_1000_clamps() {
        let rep = ManagerReputation::new(5000);
        assert_eq!(rep.score(), 1000);
        assert_eq!(rep.tier(), ManagerReputationTier::WorldClass);
    }

    #[test]
    fn season_delta_underperformance_negative() {
        let input = SeasonDeltaInput {
            expected_position: 1,
            actual_position: 10,
            trophies_won: 0,
            board_confidence: 50,
            was_sacked: false,
            sacking_stage: None,
            youth_graduates: 0,
        };
        let config = ReputationConfig::default();
        let result = apply_season_delta(500, &input, &config);
        assert_eq!(result, 500 - 12 * 9);
    }

    #[test]
    fn season_delta_no_change_baseline() {
        let input = SeasonDeltaInput {
            expected_position: 5,
            actual_position: 5,
            trophies_won: 0,
            board_confidence: 50,
            was_sacked: false,
            sacking_stage: None,
            youth_graduates: 0,
        };
        let config = ReputationConfig::default();
        let result = apply_season_delta(500, &input, &config);
        assert_eq!(result, 500);
    }

    #[test]
    fn reputation_serde_roundtrip() {
        let rep = ManagerReputation::new(750);
        let json = serde_json::to_string(&rep).unwrap();
        let deserialized: ManagerReputation = serde_json::from_str(&json).unwrap();
        assert_eq!(rep, deserialized);
    }

    #[test]
    fn tier_serde_roundtrip() {
        let tier = ManagerReputationTier::Continental;
        let json = serde_json::to_string(&tier).unwrap();
        let deserialized: ManagerReputationTier = serde_json::from_str(&json).unwrap();
        assert_eq!(tier, deserialized);
    }

    #[test]
    fn reputation_add_clamps_to_1000() {
        let mut rep = ManagerReputation::new(990);
        rep.add(20);
        assert_eq!(rep.score(), 1000);
    }

    #[test]
    fn reputation_subtract_clamps_to_zero() {
        let mut rep = ManagerReputation::new(10);
        rep.subtract(50);
        assert_eq!(rep.score(), 0);
    }

    #[test]
    fn reputation_add_subtract_chain() {
        let mut rep = ManagerReputation::new(500);
        rep.add(100);
        assert_eq!(rep.score(), 600);
        rep.subtract(50);
        assert_eq!(rep.score(), 550);
    }
}
