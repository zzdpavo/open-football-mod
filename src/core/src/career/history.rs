use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CareerExitReason {
    ContractExpired,
    Resigned,
    Sacked { stage: String },
    MutualConsent,
    MovedUp,
    MovedDown,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MatchRecord {
    pub wins: u16,
    pub draws: u16,
    pub losses: u16,
}

impl MatchRecord {
    pub fn total(&self) -> u16 {
        self.wins + self.draws + self.losses
    }

    pub fn win_percentage(&self) -> f32 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        (self.wins as f32 / total as f32) * 100.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManagerCareerEntry {
    pub club_name: String,
    pub start_season: u16,
    pub end_season: Option<u16>,
    pub exit_reason: Option<CareerExitReason>,
    pub match_record: MatchRecord,
    pub expected_position: u8,
    pub actual_position: u8,
    pub reputation_start: u16,
    pub reputation_end: u16,
}

impl ManagerCareerEntry {
    pub fn new(
        club_name: String,
        start_season: u16,
        match_record: MatchRecord,
        expected_position: u8,
        actual_position: u8,
        reputation_start: u16,
    ) -> Self {
        Self {
            club_name,
            start_season,
            end_season: None,
            exit_reason: None,
            match_record,
            expected_position,
            actual_position,
            reputation_start,
            reputation_end: 0,
        }
    }

    pub fn end_season(
        &mut self,
        end_season: u16,
        exit_reason: CareerExitReason,
        reputation_end: u16,
    ) {
        self.end_season = Some(end_season);
        self.exit_reason = Some(exit_reason);
        self.reputation_end = reputation_end;
    }

    pub fn is_active(&self) -> bool {
        self.end_season.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_record_total() {
        let record = MatchRecord { wins: 10, draws: 5, losses: 3 };
        assert_eq!(record.total(), 18);
    }

    #[test]
    fn match_record_win_percentage() {
        let record = MatchRecord { wins: 10, draws: 5, losses: 5 };
        assert!((record.win_percentage() - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn match_record_win_percentage_zero_total() {
        let record = MatchRecord { wins: 0, draws: 0, losses: 0 };
        assert_eq!(record.win_percentage(), 0.0);
    }

    #[test]
    fn career_entry_new_is_active() {
        let entry = ManagerCareerEntry::new(
            "Arsenal".to_string(),
            2025,
            MatchRecord { wins: 10, draws: 5, losses: 3 },
            4,
            2,
            500,
        );
        assert!(entry.is_active());
        assert_eq!(entry.club_name, "Arsenal");
        assert!(entry.end_season.is_none());
        assert!(entry.exit_reason.is_none());
        assert_eq!(entry.reputation_start, 500);
        assert_eq!(entry.reputation_end, 0);
    }

    #[test]
    fn career_entry_end_season() {
        let mut entry = ManagerCareerEntry::new(
            "Chelsea".to_string(),
            2025,
            MatchRecord { wins: 20, draws: 8, losses: 10 },
            6,
            8,
            400,
        );
        entry.end_season(2026, CareerExitReason::Sacked { stage: "mid_season".to_string() }, 350);
        assert!(!entry.is_active());
        assert_eq!(entry.end_season, Some(2026));
        assert_eq!(
            entry.exit_reason,
            Some(CareerExitReason::Sacked { stage: "mid_season".to_string() })
        );
        assert_eq!(entry.reputation_end, 350);
    }

    #[test]
    fn exit_reason_variants() {
        let reasons = vec![
            CareerExitReason::ContractExpired,
            CareerExitReason::Resigned,
            CareerExitReason::Sacked { stage: "early".to_string() },
            CareerExitReason::MutualConsent,
            CareerExitReason::MovedUp,
            CareerExitReason::MovedDown,
            CareerExitReason::Retired,
        ];
        assert_eq!(reasons.len(), 7);
    }

    #[test]
    fn exit_reason_equality() {
        assert_eq!(CareerExitReason::ContractExpired, CareerExitReason::ContractExpired);
        assert_ne!(CareerExitReason::Resigned, CareerExitReason::MutualConsent);
        assert_eq!(
            CareerExitReason::Sacked { stage: "mid".to_string() },
            CareerExitReason::Sacked { stage: "mid".to_string() }
        );
    }

    #[test]
    fn career_entry_serde_roundtrip() {
        let entry = ManagerCareerEntry::new(
            "Liverpool".to_string(),
            2024,
            MatchRecord { wins: 25, draws: 7, losses: 6 },
            3,
            1,
            600,
        );
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: ManagerCareerEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, deserialized);
    }

    #[test]
    fn career_entry_with_exit_serde_roundtrip() {
        let mut entry = ManagerCareerEntry::new(
            "Bayern".to_string(),
            2023,
            MatchRecord { wins: 20, draws: 5, losses: 9 },
            1,
            2,
            700,
        );
        entry.end_season(2024, CareerExitReason::MutualConsent, 680);
        let json = serde_json::to_string(&entry).unwrap();
        let deserialized: ManagerCareerEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, deserialized);
        assert!(!deserialized.is_active());
    }

    #[test]
    fn match_record_serde_roundtrip() {
        let record = MatchRecord { wins: 15, draws: 10, losses: 13 };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: MatchRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, deserialized);
    }

    #[test]
    fn match_record_win_percentage_all_wins() {
        let record = MatchRecord { wins: 10, draws: 0, losses: 0 };
        assert!((record.win_percentage() - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn match_record_win_percentage_all_losses() {
        let record = MatchRecord { wins: 0, draws: 0, losses: 10 };
        assert!((record.win_percentage() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn exit_reason_serde_roundtrip() {
        let reason = CareerExitReason::Sacked { stage: "late_season".to_string() };
        let json = serde_json::to_string(&reason).unwrap();
        let deserialized: CareerExitReason = serde_json::from_str(&json).unwrap();
        assert_eq!(reason, deserialized);
    }

    #[test]
    fn career_entry_reputation_tracking() {
        let mut entry = ManagerCareerEntry::new(
            "Juventus".to_string(),
            2025,
            MatchRecord { wins: 22, draws: 8, losses: 8 },
            2,
            4,
            550,
        );
        assert_eq!(entry.reputation_start, 550);
        assert_eq!(entry.reputation_end, 0);
        entry.end_season(2026, CareerExitReason::Resigned, 520);
        assert_eq!(entry.reputation_end, 520);
    }
}
