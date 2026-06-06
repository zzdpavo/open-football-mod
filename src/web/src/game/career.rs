use crate::GameAppData;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use core::Club;
use core::DecisionPoint;
use core::FootballSimulator;
use core::StaffPosition;
use core::career::tactics::{TacticalChoice, validate_tactical_choice};
use core::career::transfer::{
    SaleListingRequest, SaleListingType, TransferBidRequest, TransferBudget, TransferDecision,
    TransferProposalSummary, ListedPlayer, TransferTarget,
};
use core::career::interactive::{BoardVerdict, SeasonResolution};
use core::PlayerPositionType;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct StartCareerRequest {
    pub club_id: u32,
    pub manager_name: String,
}

#[derive(Serialize)]
pub struct StartCareerResponse {
    pub success: bool,
    pub staff_id: u32,
    pub club_id: u32,
}

#[derive(Serialize)]
pub struct CareerStatusResponse {
    pub active: bool,
    pub manager_name: Option<String>,
    pub club_id: Option<u32>,
    pub interactive_mode: bool,
    pub pending_decision: Option<DecisionPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reputation_score: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reputation_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board_confidence: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_season: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub career_history_count: Option<usize>,
}

pub async fn career_start(
    State(state): State<GameAppData>,
    Json(req): Json<StartCareerRequest>,
) -> impl IntoResponse {
    let data_arc = {
        let guard = state.data.read().await;
        match guard.as_ref() {
            Some(d) => Arc::clone(d),
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({"error": "No game data loaded"})),
                )
                    .into_response()
            }
        }
    };

    let handle = tokio::runtime::Handle::current();
    let data = state.data.clone();

    match tokio::task::spawn_blocking(move || {
        let mut simulator_data = Arc::unwrap_or_clone(data_arc);

        let gs = simulator_data.game_state();
        if gs.user_manager.is_some() {
            return Err("Career already started");
        }

        let club = simulator_data
            .club(req.club_id)
            .ok_or("Club not found")?;

        let (staff_id, club_id) = find_club_manager(club).ok_or("No manager found for club")?;

        simulator_data
            .game_state_mut()
            .start_career(staff_id, club_id, req.manager_name.clone());
        simulator_data.game_state_mut().interactive_mode = true;

        handle.block_on(async {
            let mut guard = data.write().await;
            *guard = Some(Arc::new(simulator_data));
        });

        Ok((staff_id, club_id))
    })
    .await
    {
        Ok(Ok((staff_id, club_id))) => Json(StartCareerResponse {
            success: true,
            staff_id,
            club_id,
        })
        .into_response(),
        Ok(Err(msg)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Task join error"})),
        )
            .into_response(),
    }
}

pub async fn career_status(
    State(state): State<GameAppData>,
) -> Json<CareerStatusResponse> {
    let guard = state.data.read().await;
    let Some(data) = guard.as_ref() else {
        return Json(CareerStatusResponse {
            active: false,
            manager_name: None,
            club_id: None,
            interactive_mode: false,
            pending_decision: None,
            reputation_score: None,
            reputation_tier: None,
            board_confidence: None,
            current_season: None,
            career_history_count: None,
        });
    };

    let gs = data.game_state();
    let um = gs.user_manager.as_ref();

    Json(CareerStatusResponse {
        active: um.is_some(),
        manager_name: um.map(|m| m.manager_name.clone()),
        club_id: um.map(|m| m.club_id),
        interactive_mode: gs.interactive_mode,
        pending_decision: gs.pending_decision.clone(),
        reputation_score: gs.career_state().map(|cs| cs.reputation.score()),
        reputation_tier: gs.career_state().map(|cs| format!("{:?}", cs.reputation.tier())),
        board_confidence: if um.is_some() { Some(gs.board_confidence) } else { None },
        current_season: if um.is_some() { Some(gs.current_season) } else { None },
        career_history_count: gs.career_state().map(|cs| cs.history.len()),
    })
}

const MAX_ADVANCE_DAYS: u32 = 365;

pub async fn career_advance(
    State(state): State<GameAppData>,
) -> impl IntoResponse {
    let data_arc = {
        let guard = state.data.read().await;
        match guard.as_ref() {
            Some(d) => Arc::clone(d),
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({"error": "No game data loaded"})),
                )
                    .into_response()
            }
        }
    };

    let data = state.data.clone();
    let handle = tokio::runtime::Handle::current();

    let result = tokio::task::spawn_blocking(move || {
        let mut simulator_data = Arc::unwrap_or_clone(data_arc);

        {
            let gs = simulator_data.game_state();
            if !gs.interactive_mode || gs.user_manager.is_none() {
                return Err("No active career");
            }
            if gs.pending_decision.is_some() {
                return Ok(build_status(&simulator_data));
            }
        }

        for _ in 0..MAX_ADVANCE_DAYS {
            let result = handle.block_on(FootballSimulator::simulate(&mut simulator_data));

            if let Some(decision) = result.pending_decision {
                simulator_data.game_state_mut().set_decision(decision);
                break;
            }
        }

        let status = build_status(&simulator_data);

        handle.block_on(async {
            let mut guard = data.write().await;
            *guard = Some(Arc::new(simulator_data));
        });

        Ok(status)
    })
    .await;

    match result {
        Ok(Ok(status)) => Json(status).into_response(),
        Ok(Err(msg)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Task join error"})),
        )
            .into_response(),
    }
}

fn find_club_manager(club: &Club) -> Option<(u32, u32)> {
    let priorities: &[StaffPosition] = &[
        StaffPosition::Manager,
        StaffPosition::CaretakerManager,
        StaffPosition::FirstTeamCoach,
        StaffPosition::AssistantManager,
        StaffPosition::Coach,
    ];

    for target_pos in priorities {
        for team in &club.teams.teams {
            for staff in &team.staffs.staffs {
                if let Some(contract) = &staff.contract {
                    if contract.position == *target_pos {
                        return Some((staff.id, club.id));
                    }
                }
            }
        }
    }

    Some((club.id, club.id))
}

fn build_status(simulator_data: &core::SimulatorData) -> CareerStatusResponse {
    let gs = simulator_data.game_state();
    let um = gs.user_manager.as_ref();

    let (reputation_score, reputation_tier, career_history_count) =
        if let Some(cs) = gs.career_state() {
            (
                Some(cs.reputation.score()),
                Some(format!("{:?}", cs.reputation.tier())),
                Some(cs.history.len()),
            )
        } else {
            (None, None, None)
        };

    CareerStatusResponse {
        active: um.is_some(),
        manager_name: um.map(|m| m.manager_name.clone()),
        club_id: um.map(|m| m.club_id),
        interactive_mode: gs.interactive_mode,
        pending_decision: gs.pending_decision.clone(),
        reputation_score,
        reputation_tier,
        board_confidence: if um.is_some() { Some(gs.board_confidence) } else { None },
        current_season: if um.is_some() { Some(gs.current_season) } else { None },
        career_history_count,
    }
}

#[derive(Deserialize)]
pub struct SubmitTacticsRequest {
    pub formation: core::MatchTacticType,
    pub starting_xi: Vec<u32>,
    pub approach: core::TacticalStyle,
    pub captain_id: Option<u32>,
    pub penalty_taker_id: Option<u32>,
    pub free_kick_taker_id: Option<u32>,
}

#[derive(Serialize)]
pub struct SubmitTacticsResponse {
    pub success: bool,
}

pub async fn career_submit_tactics(
    State(state): State<GameAppData>,
    Json(req): Json<SubmitTacticsRequest>,
) -> impl IntoResponse {
    let data_arc = {
        let guard = state.data.read().await;
        match guard.as_ref() {
            Some(d) => Arc::clone(d),
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({"error": "No game data loaded"})),
                )
                    .into_response()
            }
        }
    };

    let data = state.data.clone();
    let handle = tokio::runtime::Handle::current();

    let result = tokio::task::spawn_blocking(move || {
        let mut simulator_data = Arc::unwrap_or_clone(data_arc);

        let club_id = {
            let gs = simulator_data.game_state();
            let um = gs.user_manager.as_ref().ok_or("No active career".to_string())?;
            if !gs.interactive_mode {
                return Err("Not in interactive mode".to_string());
            }
            match &gs.pending_decision {
                Some(DecisionPoint::PreMatch { .. }) => {}
                _ => return Err("No pending PreMatch decision".to_string()),
            }
            um.club_id
        };

        let club = simulator_data.club(club_id).ok_or("Club not found".to_string())?;
        let main_team = club.teams.main().ok_or("Main team not found".to_string())?;
        let squad_player_ids: Vec<u32> = main_team.players().iter().map(|p| p.id).collect();

        let choice = TacticalChoice {
            formation: req.formation,
            starting_xi: req.starting_xi,
            approach: req.approach,
            captain_id: req.captain_id,
            penalty_taker_id: req.penalty_taker_id,
            free_kick_taker_id: req.free_kick_taker_id,
        };

        validate_tactical_choice(&choice, &squad_player_ids)
            .map_err(|errors| format!("Validation failed: {:?}", errors))?;

        simulator_data.game_state_mut().resolve_pre_match(choice);

        handle.block_on(async {
            let mut guard = data.write().await;
            *guard = Some(Arc::new(simulator_data));
        });

        Ok(())
    })
    .await;

    match result {
        Ok(Ok(())) => Json(SubmitTacticsResponse { success: true }).into_response(),
        Ok(Err(msg)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Task join error"})),
        )
            .into_response(),
    }
}

pub async fn career_resolve_transfer_window(
    State(state): State<GameAppData>,
) -> impl IntoResponse {
    let data_arc = {
        let guard = state.data.read().await;
        match guard.as_ref() {
            Some(d) => Arc::clone(d),
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({"error": "No game data loaded"})),
                )
                    .into_response()
            }
        }
    };

    let data = state.data.clone();
    let handle = tokio::runtime::Handle::current();

    let result = tokio::task::spawn_blocking(move || {
        let mut simulator_data = Arc::unwrap_or_clone(data_arc);

        {
            let gs = simulator_data.game_state();
            if !gs.interactive_mode {
                return Err("Not in interactive mode".to_string());
            }
            match &gs.pending_decision {
                Some(DecisionPoint::TransferWindow { .. }) => {}
                _ => return Err("No pending TransferWindow decision".to_string()),
            }
        }

        simulator_data.game_state_mut().resolve_transfer_window();

        handle.block_on(async {
            let mut guard = data.write().await;
            *guard = Some(Arc::new(simulator_data));
        });

        Ok(())
    })
    .await;

    match result {
        Ok(Ok(())) => Json(serde_json::json!({"success": true})).into_response(),
        Ok(Err(msg)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Task join error"})),
        )
            .into_response(),
    }
}

#[derive(Serialize)]
pub struct SquadPlayerResponse {
    pub id: u32,
    pub name: String,
    pub position: PlayerPositionType,
    pub overall_rating: u8,
}

pub async fn career_squad(State(state): State<GameAppData>) -> impl IntoResponse {
    let guard = state.data.read().await;
    let Some(data) = guard.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "No game data loaded"})),
        )
            .into_response();
    };

    let gs = data.game_state();
    let Some(um) = gs.user_manager.as_ref() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No active career"})),
        )
            .into_response();
    };

    let Some(club) = data.club(um.club_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Club not found"})),
        )
            .into_response();
    };

    let Some(main_team) = club.teams.main() else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Main team not found"})),
        )
            .into_response();
    };

    let players: Vec<SquadPlayerResponse> = main_team
        .players()
        .iter()
        .map(|p| SquadPlayerResponse {
            id: p.id,
            name: format!("{}", p.full_name),
            position: p.position(),
            overall_rating: p.skills.calculate_ability(),
        })
        .collect();

    Json(players).into_response()
}

#[derive(Serialize)]
pub struct TransferBudgetResponse {
    pub total: u64,
    pub spent: u64,
    pub reserved: u64,
    pub available: u64,
}

impl From<&TransferBudget> for TransferBudgetResponse {
    fn from(b: &TransferBudget) -> Self {
        Self {
            total: b.total,
            spent: b.spent,
            reserved: b.reserved,
            available: b.available(),
        }
    }
}

pub async fn career_transfer_budget(State(state): State<GameAppData>) -> impl IntoResponse {
    let guard = state.data.read().await;
    let Some(data) = guard.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "No game data loaded"})),
        )
            .into_response();
    };

    let gs = data.game_state();
    let Some(_um) = gs.user_manager.as_ref() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No active career"})),
        )
            .into_response();
    };

    if !gs.interactive_mode {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Not in interactive mode"})),
        )
            .into_response();
    }

    let budget = gs.transfer_budget();
    let Some(b) = budget else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Transfer budget not initialized"})),
        )
            .into_response();
    };

    let resp = TransferBudgetResponse::from(b);
    Json(resp).into_response()
}

pub async fn career_transfer_proposals(State(state): State<GameAppData>) -> impl IntoResponse {
    let guard = state.data.read().await;
    let Some(data) = guard.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "No game data loaded"})),
        )
            .into_response();
    };

    let gs = data.game_state();
    let Some(_um) = gs.user_manager.as_ref() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No active career"})),
        )
            .into_response();
    };

    if !gs.interactive_mode {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Not in interactive mode"})),
        )
            .into_response();
    }

    // TODO: Access TransferMarket from SimulatorData requires navigating through
    // countries → leagues → transfer_market. For now return empty until market
    // access is wired up in the simulation flow.
    let proposals: Vec<TransferProposalSummary> = Vec::new();
    Json(proposals).into_response()
}

#[derive(Deserialize)]
pub struct TransferDecideRequest {
    pub negotiation_id: u32,
    pub decision: TransferDecision,
}

#[derive(Serialize)]
pub struct TransferDecideResponse {
    pub success: bool,
}

pub async fn career_transfer_decide(
    State(state): State<GameAppData>,
    Json(req): Json<TransferDecideRequest>,
) -> impl IntoResponse {
    let data_arc = {
        let guard = state.data.read().await;
        match guard.as_ref() {
            Some(d) => Arc::clone(d),
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({"error": "No game data loaded"})),
                )
                    .into_response()
            }
        }
    };

    let data = state.data.clone();
    let handle = tokio::runtime::Handle::current();

    let result = tokio::task::spawn_blocking(move || {
        let mut simulator_data = Arc::unwrap_or_clone(data_arc);

        {
            let gs = simulator_data.game_state();
            let _um = gs.user_manager.as_ref().ok_or("No active career".to_string())?;
            if !gs.interactive_mode {
                return Err("Not in interactive mode".to_string());
            }
        }

        simulator_data
            .game_state_mut()
            .set_transfer_decision(req.negotiation_id, req.decision);

        handle.block_on(async {
            let mut guard = data.write().await;
            *guard = Some(Arc::new(simulator_data));
        });

        Ok(())
    })
    .await;

    match result {
        Ok(Ok(())) => Json(TransferDecideResponse { success: true }).into_response(),
        Ok(Err(msg)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Task join error"})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct ListPlayerRequest {
    pub player_id: u32,
    pub asking_price: f64,
    pub listing_type: SaleListingType,
}

#[derive(Serialize)]
pub struct ListPlayerResponse {
    pub success: bool,
}

pub async fn career_transfer_list_player(
    State(state): State<GameAppData>,
    Json(req): Json<ListPlayerRequest>,
) -> impl IntoResponse {
    let data_arc = {
        let guard = state.data.read().await;
        match guard.as_ref() {
            Some(d) => Arc::clone(d),
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({"error": "No game data loaded"})),
                )
                    .into_response()
            }
        }
    };

    let data = state.data.clone();
    let handle = tokio::runtime::Handle::current();

    let result = tokio::task::spawn_blocking(move || {
        let mut simulator_data = Arc::unwrap_or_clone(data_arc);

        {
            let gs = simulator_data.game_state();
            let _um = gs.user_manager.as_ref().ok_or("No active career".to_string())?;
            if !gs.interactive_mode {
                return Err("Not in interactive mode".to_string());
            }
        }

        let listing = SaleListingRequest {
            player_id: req.player_id,
            asking_price: req.asking_price,
            listing_type: req.listing_type,
        };

        simulator_data.game_state_mut().add_sale_listing(listing);

        handle.block_on(async {
            let mut guard = data.write().await;
            *guard = Some(Arc::new(simulator_data));
        });

        Ok(())
    })
    .await;

    match result {
        Ok(Ok(())) => Json(ListPlayerResponse { success: true }).into_response(),
        Ok(Err(msg)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Task join error"})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct TransferBidBody {
    pub target_player_id: u32,
    pub offering_amount: f64,
    pub is_loan: bool,
}

#[derive(Serialize)]
pub struct TransferBidResponse {
    pub success: bool,
}

pub async fn career_transfer_bid(
    State(state): State<GameAppData>,
    Json(req): Json<TransferBidBody>,
) -> impl IntoResponse {
    let data_arc = {
        let guard = state.data.read().await;
        match guard.as_ref() {
            Some(d) => Arc::clone(d),
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({"error": "No game data loaded"})),
                )
                    .into_response()
            }
        }
    };

    let data = state.data.clone();
    let handle = tokio::runtime::Handle::current();

    let result = tokio::task::spawn_blocking(move || {
        let mut simulator_data = Arc::unwrap_or_clone(data_arc);

        {
            let gs = simulator_data.game_state();
            let _um = gs.user_manager.as_ref().ok_or("No active career".to_string())?;
            if !gs.interactive_mode {
                return Err("Not in interactive mode".to_string());
            }
        }

        let bid = TransferBidRequest {
            target_player_id: req.target_player_id,
            offering_amount: req.offering_amount,
            is_loan: req.is_loan,
        };

        simulator_data
            .game_state_mut()
            .add_bid_request(bid)
            .map_err(|e| e)?;

        handle.block_on(async {
            let mut guard = data.write().await;
            *guard = Some(Arc::new(simulator_data));
        });

        Ok(())
    })
    .await;

    match result {
        Ok(Ok(())) => Json(TransferBidResponse { success: true }).into_response(),
        Ok(Err(msg)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Task join error"})),
        )
            .into_response(),
    }
}

#[derive(Serialize)]
pub struct LastMatchResponse {
    pub date: Option<String>,
    pub opponent: Option<String>,
    pub opponent_team_id: u32,
    pub home_goals: u8,
    pub away_goals: u8,
    pub was_home: bool,
    pub result: String,
    pub starting_tactic: Option<core::MatchTacticType>,
    pub final_tactic: Option<core::MatchTacticType>,
    pub tactic_change_minute: Option<u8>,
}

pub async fn career_last_match(State(state): State<GameAppData>) -> impl IntoResponse {
    let guard = state.data.read().await;
    let Some(data) = guard.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "No game data loaded"})),
        )
            .into_response();
    };

    let gs = data.game_state();
    let Some(um) = gs.user_manager.as_ref() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No active career"})),
        )
            .into_response();
    };

    let Some(club) = data.club(um.club_id) else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Club not found"})),
        )
            .into_response();
    };

    let Some(main_team) = club.teams.main() else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Main team not found"})),
        )
            .into_response();
    };

    let Some(last) = main_team.match_history.items().last() else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "No match history yet"})),
        )
            .into_response();
    };

    let rival_team = data.team(last.rival_team_id);
    let opponent_name = rival_team.map(|t| t.name.clone());

    let our_goals = last.score.0.get();
    let their_goals = last.score.1.get();
    let was_home = last.score.0.team_id == main_team.id;

    let result = match our_goals.cmp(&their_goals) {
        std::cmp::Ordering::Greater => "W",
        std::cmp::Ordering::Less => "L",
        std::cmp::Ordering::Equal => "D",
    };

    let resp = LastMatchResponse {
        date: Some(last.date.format("%Y-%m-%d %H:%M").to_string()),
        opponent: opponent_name,
        opponent_team_id: last.rival_team_id,
        home_goals: if was_home { our_goals } else { their_goals },
        away_goals: if was_home { their_goals } else { our_goals },
        was_home,
        result: result.to_string(),
        starting_tactic: last.tactic_started,
        final_tactic: last.tactic_used,
        tactic_change_minute: last.tactic_change_minute,
    };

    Json(resp).into_response()
}

pub async fn career_transfer_listed(State(state): State<GameAppData>) -> impl IntoResponse {
    let guard = state.data.read().await;
    let Some(data) = guard.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "No game data loaded"})),
        )
            .into_response();
    };

    let gs = data.game_state();
    let Some(_um) = gs.user_manager.as_ref() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No active career"})),
        )
            .into_response();
    };

    if !gs.interactive_mode {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Not in interactive mode"})),
        )
            .into_response();
    }

    // TODO: Access TransferMarket from SimulatorData requires navigating through
    // countries → leagues → transfer_market. For now return empty until market
    // access is wired up in the simulation flow.
    let listed: Vec<ListedPlayer> = Vec::new();
    Json(listed).into_response()
}

pub async fn career_transfer_targets(State(state): State<GameAppData>) -> impl IntoResponse {
    let guard = state.data.read().await;
    let Some(data) = guard.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "No game data loaded"})),
        )
            .into_response();
    };

    let gs = data.game_state();
    let Some(_um) = gs.user_manager.as_ref() else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No active career"})),
        )
            .into_response();
    };

    if !gs.interactive_mode {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Not in interactive mode"})),
        )
            .into_response();
    }

    // TODO: Wire to actual AI scouting shortlists. Requires navigating
    // SimulatorData → countries → leagues → transfer_market and integrating
    // with the tactical manager's target selection logic.
    let targets: Vec<TransferTarget> = Vec::new();
    Json(targets).into_response()
}

#[derive(Serialize)]
pub struct SeasonSummaryResponse {
    pub season: u16,
    pub league_position: u8,
    pub expected_position: u8,
    pub reputation_before: u16,
    pub reputation_after: u16,
    pub board_verdict: String,
    pub match_record_wins: u16,
    pub match_record_draws: u16,
    pub match_record_losses: u16,
}

pub async fn career_season_summary(State(state): State<GameAppData>) -> impl IntoResponse {
    let guard = state.data.read().await;
    let Some(data) = guard.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "No game data loaded"})),
        )
            .into_response();
    };

    let gs = data.game_state();
    if !gs.interactive_mode || gs.user_manager.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No active career"})),
        )
            .into_response();
    }

    match &gs.last_season_summary {
        Some(summary) => {
            let verdict_str = match &summary.board_verdict {
                BoardVerdict::Satisfied { confidence, .. } => format!("satisfied (confidence: {})", confidence),
                BoardVerdict::Neutral { confidence } => format!("neutral (confidence: {})", confidence),
                BoardVerdict::Warning { confidence } => format!("warning (confidence: {})", confidence),
                BoardVerdict::Sacked { reason } => format!("sacked ({})", reason),
            };
            Json(SeasonSummaryResponse {
                season: summary.season,
                league_position: summary.league_position,
                expected_position: summary.expected_position,
                reputation_before: summary.reputation_before,
                reputation_after: summary.reputation_after,
                board_verdict: verdict_str,
                match_record_wins: summary.match_record.wins,
                match_record_draws: summary.match_record.draws,
                match_record_losses: summary.match_record.losses,
            })
            .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "No season summary available"})),
        )
            .into_response(),
    }
}

#[derive(Deserialize)]
pub struct ResolveSeasonEndRequest {
    pub league_position: u8,
    pub expected_position: u8,
    pub trophies_won: u8,
}

#[derive(Serialize)]
pub struct ResolveSeasonEndResponse {
    pub resolution: String,
    pub summary: Option<SeasonSummaryResponse>,
}

pub async fn career_resolve_season_end(
    State(state): State<GameAppData>,
    Json(req): Json<ResolveSeasonEndRequest>,
) -> impl IntoResponse {
    let data_arc = {
        let guard = state.data.read().await;
        match guard.as_ref() {
            Some(d) => Arc::clone(d),
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({"error": "No game data loaded"})),
                )
                    .into_response()
            }
        }
    };

    let data = state.data.clone();

    let result = tokio::task::spawn_blocking(move || {
        let mut simulator_data = Arc::unwrap_or_clone(data_arc);
        let gs = simulator_data.game_state();
        if !gs.interactive_mode || gs.user_manager.is_none() {
            return Err("No active career");
        }

        let club_id = gs.user_manager.as_ref().unwrap().club_id;
        let club_name = simulator_data
            .club(club_id)
            .map(|c| c.name.clone())
            .unwrap_or_default();

        let resolution = simulator_data.game_state_mut().resolve_season_end(
            req.league_position,
            req.expected_position,
            req.trophies_won,
            club_name,
        );

        let (resolution_str, summary_resp) = match &resolution {
            SeasonResolution::Continuing { summary } => ("continuing", Some(summary.clone())),
            SeasonResolution::Sacked { summary } => ("sacked", Some(summary.clone())),
            SeasonResolution::ContractExpired { summary } => ("contract_expired", Some(summary.clone())),
        };

        let summary_out = summary_resp.map(|s| {
            let verdict_str = match &s.board_verdict {
                BoardVerdict::Satisfied { confidence, .. } => format!("satisfied (confidence: {})", confidence),
                BoardVerdict::Neutral { confidence } => format!("neutral (confidence: {})", confidence),
                BoardVerdict::Warning { confidence } => format!("warning (confidence: {})", confidence),
                BoardVerdict::Sacked { reason } => format!("sacked ({})", reason),
            };
            SeasonSummaryResponse {
                season: s.season,
                league_position: s.league_position,
                expected_position: s.expected_position,
                reputation_before: s.reputation_before,
                reputation_after: s.reputation_after,
                board_verdict: verdict_str,
                match_record_wins: s.match_record.wins,
                match_record_draws: s.match_record.draws,
                match_record_losses: s.match_record.losses,
            }
        });

        if matches!(resolution, SeasonResolution::Sacked { .. }) {
            let reason = match &resolution {
                SeasonResolution::Sacked { summary } => match &summary.board_verdict {
                    BoardVerdict::Sacked { reason } => reason.clone(),
                    _ => "unknown".to_string(),
                },
                _ => unreachable!(),
            };
            simulator_data.game_state_mut().sack_user(reason);
        }

        let handle = tokio::runtime::Handle::current();
        handle.block_on(async {
            let mut guard = data.write().await;
            *guard = Some(Arc::new(simulator_data));
        });

        Ok((resolution_str.to_string(), summary_out))
    })
    .await;

    match result {
        Ok(Ok((resolution, summary))) => Json(ResolveSeasonEndResponse {
            resolution,
            summary,
        })
        .into_response(),
        Ok(Err(msg)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Task join error"})),
        )
            .into_response(),
    }
}

#[derive(Serialize)]
pub struct JobOffersResponse {
    pub offers: Vec<JobOfferInfo>,
    pub is_free_agent: bool,
}

#[derive(Serialize)]
pub struct JobOfferInfo {
    pub club_id: u32,
    pub club_name: String,
    pub proposed_salary: u32,
    pub duration_years: u8,
}

pub async fn career_job_offers(State(state): State<GameAppData>) -> impl IntoResponse {
    let guard = state.data.read().await;
    let Some(data) = guard.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "No game data loaded"})),
        )
            .into_response();
    };

    let gs = data.game_state();
    if !gs.interactive_mode {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Not in interactive mode"})),
        )
            .into_response();
    }

    let is_free_agent = gs.user_manager.is_none();

    let offers: Vec<JobOfferInfo> = gs
        .pending_job_offers
        .iter()
        .map(|o| JobOfferInfo {
            club_id: o.club_id,
            club_name: o.club_name.clone(),
            proposed_salary: o.terms.proposed_salary,
            duration_years: o.terms.duration_years,
        })
        .collect();

    Json(JobOffersResponse {
        offers,
        is_free_agent,
    })
    .into_response()
}

#[derive(Deserialize)]
pub struct AcceptJobRequest {
    pub club_id: u32,
    pub staff_id: u32,
    pub club_name: String,
}

#[derive(Serialize)]
pub struct AcceptJobResponse {
    pub success: bool,
}

pub async fn career_accept_job(
    State(state): State<GameAppData>,
    Json(req): Json<AcceptJobRequest>,
) -> impl IntoResponse {
    let data_arc = {
        let guard = state.data.read().await;
        match guard.as_ref() {
            Some(d) => Arc::clone(d),
            None => {
                return (
                    StatusCode::SERVICE_UNAVAILABLE,
                    Json(serde_json::json!({"error": "No game data loaded"})),
                )
                    .into_response()
            }
        }
    };

    let data = state.data.clone();

    let result = tokio::task::spawn_blocking(move || {
        let mut simulator_data = Arc::unwrap_or_clone(data_arc);
        {
            let gs = simulator_data.game_state();
            if !gs.interactive_mode {
                return Err("Not in interactive mode".to_string());
            }
        }

        simulator_data
            .game_state_mut()
            .accept_job_offer(req.club_id, req.staff_id, req.club_name.clone())
            .map_err(|e| e)?;

        let handle = tokio::runtime::Handle::current();
        handle.block_on(async {
            let mut guard = data.write().await;
            *guard = Some(Arc::new(simulator_data));
        });

        Ok(())
    })
    .await;

    match result {
        Ok(Ok(())) => Json(AcceptJobResponse { success: true }).into_response(),
        Ok(Err(msg)) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": msg})),
        )
            .into_response(),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Task join error"})),
        )
            .into_response(),
    }
}

#[derive(Serialize)]
pub struct CareerHistoryResponse {
    pub entries: Vec<CareerHistoryEntry>,
}

#[derive(Serialize)]
pub struct CareerHistoryEntry {
    pub club_name: String,
    pub start_season: u16,
    pub end_season: Option<u16>,
    pub is_active: bool,
    pub exit_reason: Option<String>,
    pub wins: u16,
    pub draws: u16,
    pub losses: u16,
    pub expected_position: u8,
    pub actual_position: u8,
    pub reputation_start: u16,
    pub reputation_end: u16,
}

pub async fn career_history(State(state): State<GameAppData>) -> impl IntoResponse {
    let guard = state.data.read().await;
    let Some(data) = guard.as_ref() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "No game data loaded"})),
        )
            .into_response();
    };

    let gs = data.game_state();
    if !gs.interactive_mode || gs.user_manager.is_none() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No active career"})),
        )
            .into_response();
    }

    let entries = match gs.career_state() {
        Some(cs) => cs
            .history
            .iter()
            .map(|e| CareerHistoryEntry {
                club_name: e.club_name.clone(),
                start_season: e.start_season,
                end_season: e.end_season,
                is_active: e.is_active(),
                exit_reason: e.exit_reason.as_ref().map(|r| format!("{:?}", r)),
                wins: e.match_record.wins,
                draws: e.match_record.draws,
                losses: e.match_record.losses,
                expected_position: e.expected_position,
                actual_position: e.actual_position,
                reputation_start: e.reputation_start,
                reputation_end: e.reputation_end,
            })
            .collect(),
        None => Vec::new(),
    };

    Json(CareerHistoryResponse { entries }).into_response()
}

#[derive(Deserialize)]
pub struct ClubSearchQuery {
    pub q: Option<String>,
}

#[derive(Serialize)]
pub struct ClubSearchResult {
    pub id: u32,
    pub name: String,
    pub country: String,
    pub reputation: f32,
    pub reputation_tier: String,
    pub player_count: usize,
    pub balance: i64,
    pub color_primary: String,
    pub color_secondary: String,
    pub status: String,
}

pub async fn career_search_clubs(
    State(state): State<GameAppData>,
    axum::extract::Query(query): axum::extract::Query<ClubSearchQuery>,
) -> impl IntoResponse {
    let guard = state.data.read().await;
    let data = match guard.as_ref() {
        Some(d) => d,
        None => return Json(Vec::<ClubSearchResult>::new()).into_response(),
    };

    let q = query.q.unwrap_or_default().to_lowercase();
    if q.len() < 2 {
        return Json(Vec::<ClubSearchResult>::new()).into_response();
    }

    let results: Vec<ClubSearchResult> = data
        .continents
        .iter()
        .flat_map(|c| c.countries.iter())
        .flat_map(|country| {
            let country_name = country.name.clone();
            let q_ref = &q;
            country.clubs.iter().filter_map(move |club| {
                if !club.name.to_lowercase().contains(q_ref) {
                    return None;
                }

                let main_team = club.teams.main();
                let (reputation, reputation_tier, player_count) = match main_team {
                    Some(team) => (
                        team.reputation.overall_score(),
                        format!("{:?}", team.reputation.level()),
                        team.players().len(),
                    ),
                    None => (0.0, "Amateur".to_string(), 0),
                };

                Some(ClubSearchResult {
                    id: club.id,
                    name: club.name.clone(),
                    country: country_name.clone(),
                    reputation,
                    reputation_tier,
                    player_count,
                    balance: club.finance.balance.balance,
                    color_primary: club.colors.background.clone(),
                    color_secondary: club.colors.foreground.clone(),
                    status: format!("{:?}", club.status),
                })
            })
        })
        .take(20)
        .collect();

    Json(results).into_response()
}
