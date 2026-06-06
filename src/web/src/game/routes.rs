use crate::GameAppData;
use crate::game::{
    career_accept_job, career_advance, career_history, career_job_offers, career_last_match,
    career_resolve_season_end, career_resolve_transfer_window, career_search_clubs, career_season_summary, career_squad, career_start, career_status,
    career_submit_tactics, career_transfer_bid, career_transfer_budget, career_transfer_decide,
    career_transfer_list_player, career_transfer_listed, career_transfer_proposals,
    career_transfer_targets, game_cancel_action, game_create_action, game_process_action,
    game_processing_status_action,
};
use axum::Router;
use axum::routing::{get, post};

pub fn game_routes() -> Router<GameAppData> {
    Router::new()
        .route("/api/game/create", get(game_create_action))
        .route("/api/game/process", post(game_process_action))
        .route("/api/game/processing", get(game_processing_status_action))
        .route("/api/game/cancel", post(game_cancel_action))
        .route("/api/game/career/start", post(career_start))
        .route("/api/game/career/status", get(career_status))
        .route("/api/game/career/advance", post(career_advance))
        .route("/api/game/career/tactics", post(career_submit_tactics))
        .route("/api/game/career/squad", get(career_squad))
        .route("/api/game/career/transfer-budget", get(career_transfer_budget))
        .route("/api/game/career/transfer-proposals", get(career_transfer_proposals))
        .route("/api/game/career/transfer-decide", post(career_transfer_decide))
        .route("/api/game/career/transfer-list-player", post(career_transfer_list_player))
        .route("/api/game/career/transfer-bid", post(career_transfer_bid))
        .route("/api/game/career/transfer-listed", get(career_transfer_listed))
        .route("/api/game/career/transfer-targets", get(career_transfer_targets))
        .route("/api/game/career/last-match", get(career_last_match))
        .route("/api/game/career/season-summary", get(career_season_summary))
        .route("/api/game/career/resolve-season-end", post(career_resolve_season_end))
        .route("/api/game/career/resolve-transfer-window", post(career_resolve_transfer_window))
        .route("/api/game/career/job-offers", get(career_job_offers))
        .route("/api/game/career/accept-job", post(career_accept_job))
        .route("/api/game/career/history", get(career_history))
        .route("/api/game/career/clubs", get(career_search_clubs))
}
