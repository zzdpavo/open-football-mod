use serde::{Deserialize, Serialize};

use super::reputation::ManagerReputation;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BonusClause {
    Promotion { amount: u32 },
    AvoidRelegation { amount: u32 },
    WinTrophy { amount: u32 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManagerOfferTerms {
    pub proposed_salary: u32,
    pub duration_years: u8,
    pub bonuses: Vec<BonusClause>,
}

impl ManagerOfferTerms {
    pub fn new(proposed_salary: u32, duration_years: u8, bonuses: Vec<BonusClause>) -> Self {
        Self {
            proposed_salary,
            duration_years: duration_years.clamp(1, 3),
            bonuses,
        }
    }

    pub fn annual_value(&self) -> u32 {
        self.proposed_salary * self.duration_years as u32
    }
}

pub fn generate_offer(
    club_reputation: u16,
    manager_reputation: u16,
) -> ManagerOfferTerms {
    let proposed_salary = 30_000 + club_reputation as u32 * 50;
    let club_tier = ManagerReputation::new(club_reputation).tier();
    let manager_tier = ManagerReputation::new(manager_reputation).tier();
    let duration_years = if manager_tier.ordinal() <= club_tier.ordinal() {
        2
    } else {
        1
    };
    ManagerOfferTerms::new(proposed_salary, duration_years, vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offer_duration_clamped_min() {
        let offer = ManagerOfferTerms::new(100_000, 0, vec![]);
        assert_eq!(offer.duration_years, 1);
    }

    #[test]
    fn offer_duration_clamped_max() {
        let offer = ManagerOfferTerms::new(100_000, 5, vec![]);
        assert_eq!(offer.duration_years, 3);
    }

    #[test]
    fn offer_duration_valid_unchanged() {
        let offer = ManagerOfferTerms::new(100_000, 2, vec![]);
        assert_eq!(offer.duration_years, 2);
    }

    #[test]
    fn annual_value() {
        let offer = ManagerOfferTerms::new(100_000, 3, vec![]);
        assert_eq!(offer.annual_value(), 300_000);
    }

    #[test]
    fn generate_offer_salary_formula() {
        let offer = generate_offer(500, 300);
        assert_eq!(offer.proposed_salary, 30_000 + 500 * 50);
    }

    #[test]
    fn generate_offer_duration_lower_tier_manager() {
        let offer = generate_offer(500, 100);
        assert_eq!(offer.duration_years, 2);
    }

    #[test]
    fn generate_offer_duration_higher_tier_manager() {
        let offer = generate_offer(100, 500);
        assert_eq!(offer.duration_years, 1);
    }

    #[test]
    fn generate_offer_no_bonuses() {
        let offer = generate_offer(300, 300);
        assert!(offer.bonuses.is_empty());
    }

    #[test]
    fn generate_offer_same_tier_duration_2() {
        let offer = generate_offer(500, 500);
        assert_eq!(offer.duration_years, 2);
    }

    #[test]
    fn bonus_clause_promotion_construction() {
        let clause = BonusClause::Promotion { amount: 50_000 };
        assert_eq!(clause, BonusClause::Promotion { amount: 50_000 });
    }

    #[test]
    fn bonus_clause_avoid_relegation_construction() {
        let clause = BonusClause::AvoidRelegation { amount: 25_000 };
        assert_eq!(clause, BonusClause::AvoidRelegation { amount: 25_000 });
    }

    #[test]
    fn bonus_clause_win_trophy_construction() {
        let clause = BonusClause::WinTrophy { amount: 100_000 };
        assert_eq!(clause, BonusClause::WinTrophy { amount: 100_000 });
    }

    #[test]
    fn bonus_clause_equality() {
        assert_ne!(
            BonusClause::Promotion { amount: 50_000 },
            BonusClause::WinTrophy { amount: 50_000 }
        );
    }

    #[test]
    fn salary_formula_with_zero_reputation() {
        let offer = generate_offer(0, 0);
        assert_eq!(offer.proposed_salary, 30_000);
    }

    #[test]
    fn salary_formula_with_max_reputation() {
        let offer = generate_offer(1000, 1000);
        assert_eq!(offer.proposed_salary, 30_000 + 1000 * 50);
    }

    #[test]
    fn offer_with_bonuses() {
        let bonuses = vec![
            BonusClause::Promotion { amount: 100_000 },
            BonusClause::AvoidRelegation { amount: 50_000 },
        ];
        let offer = ManagerOfferTerms::new(200_000, 2, bonuses.clone());
        assert_eq!(offer.bonuses.len(), 2);
        assert_eq!(offer.annual_value(), 400_000);
    }

    #[test]
    fn offer_serde_roundtrip() {
        let offer = ManagerOfferTerms::new(
            150_000,
            3,
            vec![BonusClause::WinTrophy { amount: 500_000 }],
        );
        let json = serde_json::to_string(&offer).unwrap();
        let deserialized: ManagerOfferTerms = serde_json::from_str(&json).unwrap();
        assert_eq!(offer, deserialized);
    }

    #[test]
    fn bonus_clause_serde_roundtrip() {
        let clause = BonusClause::AvoidRelegation { amount: 75_000 };
        let json = serde_json::to_string(&clause).unwrap();
        let deserialized: BonusClause = serde_json::from_str(&json).unwrap();
        assert_eq!(clause, deserialized);
    }
}
