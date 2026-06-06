use askama::Template;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use serde::Deserialize;

use crate::common::default_handler::{COMPUTER_NAME, CPU_BRAND, CPU_CORES, CSS_VERSION};
use crate::views::{self, MenuSection};
use crate::{GameAppData, I18n};

#[derive(Deserialize)]
pub struct ManagerRequest {
    pub lang: String,
}

#[derive(Template, askama_web::WebTemplate)]
#[template(path = "manager/get/index.html")]
pub struct ManagerOverviewTemplate {
    pub css_version: &'static str,
    pub computer_name: &'static str,
    pub cpu_brand: &'static str,
    pub cores_count: usize,
    pub i18n: I18n,
    pub lang: String,
    pub title: String,
    pub sub_title: String,
    pub sub_title_prefix: String,
    pub sub_title_suffix: String,
    pub sub_title_link: String,
    pub sub_title_country_code: String,
    pub header_color: String,
    pub foreground_color: String,
    pub menu_sections: Vec<MenuSection>,
    pub active_tab: &'static str,
    pub manager_name: String,
    pub club_name: String,
    pub club_id_display: String,
    pub current_season: u16,
    pub reputation_score: u16,
    pub reputation_tier: String,
    pub board_confidence: u8,
    pub pending_decision: bool,
    pub decision_label: String,
    pub has_career: bool,
    pub has_history: bool,
    pub history: Vec<HistoryEntry>,
}

pub struct HistoryEntry {
    pub club_name: String,
    pub start_season: u16,
    pub end_season: Option<u16>,
    pub is_active: bool,
    pub exit_reason: String,
    pub wins: u16,
    pub draws: u16,
    pub losses: u16,
    pub expected_position: u8,
    pub actual_position: u8,
    pub reputation_start: u16,
    pub reputation_end: u16,
}

pub async fn manager_overview(
    State(state): State<GameAppData>,
    Path(params): Path<ManagerRequest>,
) -> impl IntoResponse {
    let i18n = state.i18n.for_lang(&params.lang);

    let guard = state.data.read().await;
    let data = guard.as_ref();

    let (manager_name, club_name, club_id_display, current_season, reputation_score, reputation_tier, board_confidence, pending_decision, decision_label, has_career, has_history, history) = match data {
        Some(d) => {
            let gs = d.game_state();
            let um = gs.user_manager.as_ref();
            let cs = gs.career_state();
            let (rep_score, rep_tier) = cs
                .map(|c| (c.reputation.score(), format!("{:?}", c.reputation.tier())))
                .unwrap_or((0, "N/A".to_string()));
            let (has_decision, dec_label) = match &gs.pending_decision {
                Some(dp) => (true, format!("{:?}", dp)),
                None => (false, String::new()),
            };
            let hist: Vec<HistoryEntry> = cs
                .map(|c| {
                    c.history.iter().map(|e| HistoryEntry {
                        club_name: e.club_name.clone(),
                        start_season: e.start_season,
                        end_season: e.end_season,
                        is_active: e.is_active(),
                        exit_reason: e.exit_reason.as_ref().map(|r| format!("{:?}", r)).unwrap_or_default(),
                        wins: e.match_record.wins,
                        draws: e.match_record.draws,
                        losses: e.match_record.losses,
                        expected_position: e.expected_position,
                        actual_position: e.actual_position,
                        reputation_start: e.reputation_start,
                        reputation_end: e.reputation_end,
                    }).collect()
                })
                .unwrap_or_default();
            let cid = um.map(|m| m.club_id);
            let cname = cid.and_then(|id| d.club(id)).map(|c| c.name.clone()).unwrap_or_default();
            let mgr_name = um.map(|m| m.manager_name.clone()).unwrap_or_default();
            (
                mgr_name,
                cname,
                cid.map(|id| id.to_string()).unwrap_or_default(),
                gs.current_season,
                rep_score,
                rep_tier,
                gs.board_confidence,
                has_decision,
                dec_label,
                um.is_some(),
                !hist.is_empty(),
                hist,
            )
        }
        None => (String::new(), String::new(), String::new(), 0, 0, "N/A".to_string(), 0, false, String::new(), false, false, Vec::new()),
    };

    let current_path = format!("/{}/manager", &params.lang);
    let menu_sections = views::manager_menu(&i18n, &params.lang, &current_path, has_career);

    ManagerOverviewTemplate {
        css_version: CSS_VERSION,
        computer_name: &COMPUTER_NAME,
        cpu_brand: &CPU_BRAND,
        cores_count: *CPU_CORES,
        i18n,
        lang: params.lang,
        title: "Manager Dashboard".to_string(),
        sub_title: String::new(),
        sub_title_prefix: String::new(),
        sub_title_suffix: String::new(),
        sub_title_link: String::new(),
        sub_title_country_code: String::new(),
        header_color: "#333".to_string(),
        foreground_color: "#fff".to_string(),
        menu_sections,
        active_tab: "overview",
        manager_name,
        club_name,
        club_id_display,
        current_season,
        reputation_score,
        reputation_tier,
        board_confidence,
        pending_decision,
        decision_label,
        has_career,
        has_history,
        history,
    }
}
