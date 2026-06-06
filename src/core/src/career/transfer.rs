use serde::{Deserialize, Serialize};

use crate::club::Club;
use crate::transfers::market::{TransferListingOrigin, TransferMarket};
use crate::transfers::negotiation::NegotiationStatus;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferBudget {
    pub total: u64,
    pub spent: u64,
    pub reserved: u64,
    pub season: u16,
}

impl TransferBudget {
    pub fn available(&self) -> u64 {
        self.total
            .saturating_sub(self.spent)
            .saturating_sub(self.reserved)
    }

    pub fn can_afford(&self, amount: u64) -> bool {
        self.available() >= amount
    }

    pub fn reserve(&mut self, amount: u64) -> bool {
        if self.can_afford(amount) {
            self.reserved = self.reserved.saturating_add(amount);
            true
        } else {
            false
        }
    }

    pub fn commit_reserved(&mut self, amount: u64) {
        self.reserved = self.reserved.saturating_sub(amount);
        self.spent = self.spent.saturating_add(amount);
    }

    pub fn release_reserved(&mut self, amount: u64) {
        self.reserved = self.reserved.saturating_sub(amount);
    }
}

pub fn derive_budget_from_club(club: &Club) -> TransferBudget {
    let total = if let Some(ref budget) = club.finance.transfer_budget {
        budget.amount as u64
    } else {
        let available = club.transfer_plan.available_budget();
        if available > 0.0 {
            available as u64
        } else {
            (club.transfer_plan.total_budget - club.transfer_plan.spent).max(0.0) as u64
        }
    };

    TransferBudget {
        total,
        spent: club.transfer_plan.spent.max(0.0) as u64,
        reserved: club.transfer_plan.reserved.max(0.0) as u64,
        season: 0,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferProposalSummary {
    pub negotiation_id: u32,
    pub player_id: u32,
    pub player_name: String,
    pub buying_club_id: u32,
    pub buying_club_name: String,
    pub offer_amount: f64,
    pub is_loan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransferDecision {
    Approve,
    Reject,
}

pub fn collect_incoming_proposals(
    user_club_id: u32,
    market: &TransferMarket,
) -> Vec<TransferProposalSummary> {
    market
        .negotiations
        .iter()
        .filter(|(_, n)| n.selling_club_id == user_club_id && n.status == NegotiationStatus::Pending)
        .map(|(id, n)| TransferProposalSummary {
            negotiation_id: *id,
            player_id: n.player_id,
            player_name: n.player_name.clone(),
            buying_club_id: n.buying_club_id,
            buying_club_name: String::new(),
            offer_amount: n.current_offer.base_fee.amount,
            is_loan: n.is_loan,
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SaleListingType {
    Transfer,
    Loan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaleListingRequest {
    pub player_id: u32,
    pub asking_price: f64,
    pub listing_type: SaleListingType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListedPlayer {
    pub player_id: u32,
    pub player_name: String,
    pub asking_price: f64,
    pub listing_type: SaleListingType,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferBidRequest {
    pub target_player_id: u32,
    pub offering_amount: f64,
    pub is_loan: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferTarget {
    pub player_id: u32,
    pub player_name: String,
    pub club_name: String,
    pub estimated_value: f64,
    pub position: String,
}

pub fn validate_bid_request(
    bid: &TransferBidRequest,
    budget: Option<&TransferBudget>,
) -> Result<(), String> {
    if bid.offering_amount <= 0.0 {
        return Err("offering_amount must be positive".to_string());
    }
    if let Some(b) = budget {
        if !b.can_afford(bid.offering_amount as u64) {
            return Err("insufficient transfer budget".to_string());
        }
    }
    Ok(())
}

pub fn collect_listed_players(user_club_id: u32, market: &TransferMarket) -> Vec<ListedPlayer> {
    market
        .listings
        .iter()
        .filter(|l| {
            l.club_id == user_club_id
                && matches!(
                    l.origin,
                    TransferListingOrigin::SellerListed | TransferListingOrigin::LoanOutListed
                )
        })
        .map(|l| ListedPlayer {
            player_id: l.player_id,
            player_name: String::new(),
            asking_price: l.asking_price.amount,
            listing_type: match l.listing_type {
                crate::transfers::market::TransferListingType::Transfer => SaleListingType::Transfer,
                crate::transfers::market::TransferListingType::Loan => SaleListingType::Loan,
                crate::transfers::market::TransferListingType::EndOfContract => SaleListingType::Transfer,
            },
            status: format!("{:?}", l.status),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_budget(total: u64, spent: u64, reserved: u64) -> TransferBudget {
        TransferBudget {
            total,
            spent,
            reserved,
            season: 1,
        }
    }

    #[test]
    fn available_is_total_minus_spent_minus_reserved() {
        let b = make_budget(1000, 200, 150);
        assert_eq!(b.available(), 650);
    }

    #[test]
    fn available_saturates_on_overflow() {
        let b = make_budget(100, 80, 80);
        assert_eq!(b.available(), 0);

        let b2 = make_budget(100, 200, 0);
        assert_eq!(b2.available(), 0);

        let b3 = make_budget(100, 0, 200);
        assert_eq!(b3.available(), 0);
    }

    #[test]
    fn can_afford_true_when_enough() {
        let b = make_budget(1000, 200, 100);
        assert!(b.can_afford(700));
    }

    #[test]
    fn can_afford_false_when_insufficient() {
        let b = make_budget(1000, 200, 100);
        assert!(!b.can_afford(701));
    }

    #[test]
    fn can_afford_exact_amount() {
        let b = make_budget(1000, 200, 100);
        assert!(b.can_afford(700));
    }

    #[test]
    fn reserve_succeeds_when_affordable() {
        let mut b = make_budget(1000, 200, 100);
        assert!(b.reserve(300));
        assert_eq!(b.reserved, 400);
        assert_eq!(b.available(), 400);
    }

    #[test]
    fn reserve_fails_when_too_much() {
        let mut b = make_budget(1000, 200, 100);
        assert!(!b.reserve(701));
        assert_eq!(b.reserved, 100);
    }

    #[test]
    fn reserve_commit_flow() {
        let mut b = make_budget(1000, 0, 0);
        assert!(b.reserve(300));
        assert_eq!(b.available(), 700);
        assert_eq!(b.reserved, 300);

        b.commit_reserved(300);
        assert_eq!(b.spent, 300);
        assert_eq!(b.reserved, 0);
        assert_eq!(b.available(), 700);
    }

    #[test]
    fn reserve_release_flow() {
        let mut b = make_budget(1000, 0, 0);
        assert!(b.reserve(300));
        assert_eq!(b.available(), 700);

        b.release_reserved(300);
        assert_eq!(b.reserved, 0);
        assert_eq!(b.available(), 1000);
    }

    #[test]
    fn partial_release() {
        let mut b = make_budget(1000, 0, 0);
        assert!(b.reserve(300));
        b.release_reserved(100);
        assert_eq!(b.reserved, 200);
        assert_eq!(b.available(), 800);
    }

    #[test]
    fn release_and_commit_saturate() {
        let mut b = make_budget(1000, 0, 100);
        b.release_reserved(200);
        assert_eq!(b.reserved, 0);

        let mut b2 = make_budget(1000, 900, 50);
        b2.commit_reserved(100);
        assert_eq!(b2.reserved, 0);
        assert_eq!(b2.spent, 1000);
    }

    #[test]
    fn serialize_deserialize_roundtrip() {
        let b = make_budget(5000, 1000, 500);
        let json = serde_json::to_string(&b).unwrap();
        let b2: TransferBudget = serde_json::from_str(&json).unwrap();
        assert_eq!(b.total, b2.total);
        assert_eq!(b.spent, b2.spent);
        assert_eq!(b.reserved, b2.reserved);
        assert_eq!(b.season, b2.season);
    }

    #[test]
    fn zero_budget() {
        let b = make_budget(0, 0, 0);
        assert_eq!(b.available(), 0);
        assert!(!b.can_afford(1));
    }

    #[test]
    fn transfer_proposal_summary_serde_roundtrip() {
        let summary = TransferProposalSummary {
            negotiation_id: 42,
            player_id: 10,
            player_name: "Lionel Messi".to_string(),
            buying_club_id: 5,
            buying_club_name: "Barcelona".to_string(),
            offer_amount: 50_000_000.0,
            is_loan: false,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let restored: TransferProposalSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.negotiation_id, 42);
        assert_eq!(restored.player_id, 10);
        assert_eq!(restored.player_name, "Lionel Messi");
        assert_eq!(restored.buying_club_id, 5);
        assert_eq!(restored.buying_club_name, "Barcelona");
        assert_eq!(restored.offer_amount, 50_000_000.0);
        assert!(!restored.is_loan);
    }

    #[test]
    fn transfer_proposal_summary_loan_serde_roundtrip() {
        let summary = TransferProposalSummary {
            negotiation_id: 99,
            player_id: 7,
            player_name: "Erling Haaland".to_string(),
            buying_club_id: 3,
            buying_club_name: "Dortmund".to_string(),
            offer_amount: 5_000_000.0,
            is_loan: true,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let restored: TransferProposalSummary = serde_json::from_str(&json).unwrap();
        assert!(restored.is_loan);
    }

    #[test]
    fn transfer_decision_approve_serde_roundtrip() {
        let decision = TransferDecision::Approve;
        let json = serde_json::to_string(&decision).unwrap();
        let restored: TransferDecision = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, TransferDecision::Approve));
    }

    #[test]
    fn transfer_decision_reject_serde_roundtrip() {
        let decision = TransferDecision::Reject;
        let json = serde_json::to_string(&decision).unwrap();
        let restored: TransferDecision = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, TransferDecision::Reject));
    }

    #[test]
    fn collect_incoming_proposals_filters_by_selling_club_and_pending_status() {
        use crate::shared::{Currency, CurrencyValue};
        use crate::transfers::market::{TransferListing, TransferListingType, TransferMarket};
        use crate::transfers::negotiation::TransferNegotiation;
        use crate::transfers::offer::TransferOffer;
        use chrono::NaiveDate;

        let date = NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        let mut market = TransferMarket::new();

        market.add_listing(TransferListing::new(
            10,
            1,
            100,
            CurrencyValue::new(1_000_000.0, Currency::Usd),
            date,
            TransferListingType::Transfer,
        ));

        let offer1 = TransferOffer::new(
            CurrencyValue::new(800_000.0, Currency::Usd),
            2,
            date,
        );
        let mut neg1 = TransferNegotiation::new(
            1, 10, 0, 1, 2, offer1, date, 0.5, 0.6, 24, 0.5,
        );
        neg1.player_name = "Player A".to_string();

        let offer2 = TransferOffer::new(
            CurrencyValue::new(900_000.0, Currency::Usd),
            3,
            date,
        );
        let mut neg2 = TransferNegotiation::new(
            2, 20, 0, 1, 3, offer2, date, 0.5, 0.7, 28, 0.5,
        );
        neg2.player_name = "Player B".to_string();

        let offer3 = TransferOffer::new(
            CurrencyValue::new(500_000.0, Currency::Usd),
            4,
            date,
        );
        let mut neg3 = TransferNegotiation::new(
            3, 30, 0, 1, 4, offer3, date, 0.5, 0.8, 22, 0.5,
        );
        neg3.player_name = "Player C".to_string();
        neg3.status = NegotiationStatus::Accepted;

        let offer4 = TransferOffer::new(
            CurrencyValue::new(700_000.0, Currency::Usd),
            5,
            date,
        );
        let mut neg4 = TransferNegotiation::new(
            4, 40, 0, 99, 5, offer4, date, 0.5, 0.9, 26, 0.5,
        );
        neg4.player_name = "Player D".to_string();

        market.negotiations.insert(1, neg1);
        market.negotiations.insert(2, neg2);
        market.negotiations.insert(3, neg3);
        market.negotiations.insert(4, neg4);

        let proposals = collect_incoming_proposals(1, &market);

        assert_eq!(proposals.len(), 2);
        let ids: Vec<u32> = proposals.iter().map(|p| p.negotiation_id).collect();
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
        assert!(!ids.contains(&3));
        assert!(!ids.contains(&4));

        let p1 = proposals.iter().find(|p| p.negotiation_id == 1).unwrap();
        assert_eq!(p1.player_id, 10);
        assert_eq!(p1.player_name, "Player A");
        assert_eq!(p1.buying_club_id, 2);
        assert_eq!(p1.offer_amount, 800_000.0);
        assert!(!p1.is_loan);

        let p2 = proposals.iter().find(|p| p.negotiation_id == 2).unwrap();
        assert_eq!(p2.player_id, 20);
        assert_eq!(p2.player_name, "Player B");
        assert_eq!(p2.buying_club_id, 3);
        assert_eq!(p2.offer_amount, 900_000.0);
    }

    #[test]
    fn collect_incoming_proposals_returns_empty_when_no_matches() {
        let market = TransferMarket::new();
        let proposals = collect_incoming_proposals(999, &market);
        assert!(proposals.is_empty());
    }

    #[test]
    fn sale_listing_type_serde_roundtrip() {
        let t1 = SaleListingType::Transfer;
        let json = serde_json::to_string(&t1).unwrap();
        let restored: SaleListingType = serde_json::from_str(&json).unwrap();
        assert!(matches!(restored, SaleListingType::Transfer));

        let t2 = SaleListingType::Loan;
        let json2 = serde_json::to_string(&t2).unwrap();
        let restored2: SaleListingType = serde_json::from_str(&json2).unwrap();
        assert!(matches!(restored2, SaleListingType::Loan));
    }

    #[test]
    fn sale_listing_request_serde_roundtrip() {
        let req = SaleListingRequest {
            player_id: 7,
            asking_price: 25_000_000.0,
            listing_type: SaleListingType::Transfer,
        };
        let json = serde_json::to_string(&req).unwrap();
        let restored: SaleListingRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.player_id, 7);
        assert_eq!(restored.asking_price, 25_000_000.0);
        assert!(matches!(restored.listing_type, SaleListingType::Transfer));
    }

    #[test]
    fn listed_player_serde_roundtrip() {
        let lp = ListedPlayer {
            player_id: 10,
            player_name: "Test Player".to_string(),
            asking_price: 5_000_000.0,
            listing_type: SaleListingType::Loan,
            status: "Available".to_string(),
        };
        let json = serde_json::to_string(&lp).unwrap();
        let restored: ListedPlayer = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.player_id, 10);
        assert_eq!(restored.player_name, "Test Player");
        assert_eq!(restored.asking_price, 5_000_000.0);
        assert!(matches!(restored.listing_type, SaleListingType::Loan));
        assert_eq!(restored.status, "Available");
    }

    #[test]
    fn collect_listed_players_filters_by_club_and_origin() {
        use crate::shared::{Currency, CurrencyValue};
        use crate::transfers::market::{TransferListing, TransferListingType};

        let date = chrono::NaiveDate::from_ymd_opt(2026, 7, 1).unwrap();
        let mut market = TransferMarket::new();

        market.add_listing(TransferListing::new(
            10, 1, 100,
            CurrencyValue::new(1_000_000.0, Currency::Usd),
            date,
            TransferListingType::Transfer,
        ));
        market.add_listing(TransferListing::new(
            20, 1, 100,
            CurrencyValue::new(500_000.0, Currency::Usd),
            date,
            TransferListingType::Loan,
        ));
        market.add_listing(TransferListing::new(
            30, 2, 200,
            CurrencyValue::new(2_000_000.0, Currency::Usd),
            date,
            TransferListingType::Transfer,
        ));

        let listed = collect_listed_players(1, &market);
        assert_eq!(listed.len(), 2);

        let ids: Vec<u32> = listed.iter().map(|l| l.player_id).collect();
        assert!(ids.contains(&10));
        assert!(ids.contains(&20));
        assert!(!ids.contains(&30));

        let loan = listed.iter().find(|l| l.player_id == 20).unwrap();
        assert!(matches!(loan.listing_type, SaleListingType::Loan));
        assert_eq!(loan.asking_price, 500_000.0);
    }

    #[test]
    fn collect_listed_players_returns_empty_when_no_match() {
        let market = TransferMarket::new();
        let listed = collect_listed_players(999, &market);
        assert!(listed.is_empty());
    }

    #[test]
    fn transfer_bid_request_serde_roundtrip() {
        let req = TransferBidRequest {
            target_player_id: 42,
            offering_amount: 25_000_000.0,
            is_loan: false,
        };
        let json = serde_json::to_string(&req).unwrap();
        let restored: TransferBidRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.target_player_id, 42);
        assert_eq!(restored.offering_amount, 25_000_000.0);
        assert!(!restored.is_loan);
    }

    #[test]
    fn transfer_bid_request_loan_serde_roundtrip() {
        let req = TransferBidRequest {
            target_player_id: 7,
            offering_amount: 1_000_000.0,
            is_loan: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        let restored: TransferBidRequest = serde_json::from_str(&json).unwrap();
        assert!(restored.is_loan);
    }

    #[test]
    fn transfer_target_serde_roundtrip() {
        let target = TransferTarget {
            player_id: 10,
            player_name: "Kylian Mbappe".to_string(),
            club_name: "Real Madrid".to_string(),
            estimated_value: 180_000_000.0,
            position: "ST".to_string(),
        };
        let json = serde_json::to_string(&target).unwrap();
        let restored: TransferTarget = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.player_id, 10);
        assert_eq!(restored.player_name, "Kylian Mbappe");
        assert_eq!(restored.club_name, "Real Madrid");
        assert_eq!(restored.estimated_value, 180_000_000.0);
        assert_eq!(restored.position, "ST");
    }

    #[test]
    fn validate_bid_request_valid() {
        let bid = TransferBidRequest {
            target_player_id: 1,
            offering_amount: 500.0,
            is_loan: false,
        };
        let budget = make_budget(1000, 200, 100);
        assert!(validate_bid_request(&bid, Some(&budget)).is_ok());
    }

    #[test]
    fn validate_bid_request_over_budget() {
        let bid = TransferBidRequest {
            target_player_id: 1,
            offering_amount: 800.0,
            is_loan: false,
        };
        let budget = make_budget(1000, 200, 100);
        assert!(validate_bid_request(&bid, Some(&budget)).is_err());
    }

    #[test]
    fn validate_bid_request_zero_amount() {
        let bid = TransferBidRequest {
            target_player_id: 1,
            offering_amount: 0.0,
            is_loan: false,
        };
        assert!(validate_bid_request(&bid, None).is_err());
    }

    #[test]
    fn validate_bid_request_negative_amount() {
        let bid = TransferBidRequest {
            target_player_id: 1,
            offering_amount: -100.0,
            is_loan: false,
        };
        assert!(validate_bid_request(&bid, None).is_err());
    }

    #[test]
    fn validate_bid_request_no_budget() {
        let bid = TransferBidRequest {
            target_player_id: 1,
            offering_amount: 999_999.0,
            is_loan: false,
        };
        assert!(validate_bid_request(&bid, None).is_ok());
    }
}
