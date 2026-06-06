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

pub struct LastMatchInfo {
    pub home_team: String,
    pub away_team: String,
    pub home_score: u32,
    pub away_score: u32,
    pub competition: String,
}

#[derive(Template, askama_web::WebTemplate)]
#[template(path = "manager/match/index.html")]
pub struct ManagerMatchTemplate {
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
    pub has_pending_match: bool,
    pub opponent: String,
    pub competition: String,
    pub last_match: Option<LastMatchInfo>,
}

pub async fn manager_match(
    State(state): State<GameAppData>,
    Path(params): Path<ManagerRequest>,
) -> impl IntoResponse {
    let i18n = state.i18n.for_lang(&params.lang);

    let guard = state.data.read().await;
    let data = guard.as_ref();

    let (has_pending_match, opponent, competition) = match data {
        Some(d) => {
            let gs = d.game_state();
            match &gs.pending_decision {
                Some(core::career::interactive::DecisionPoint::PreMatch { opponent, competition, .. }) => {
                    (true, opponent.clone(), competition.clone())
                }
                _ => (false, String::new(), String::new()),
            }
        }
        None => (false, String::new(), String::new()),
    };

    let has_career = data.map(|d| d.game_state().user_manager.is_some()).unwrap_or(false);
    let current_path = format!("/{}/manager/match", &params.lang);
    let menu_sections = views::manager_menu(&i18n, &params.lang, &current_path, has_career);

    ManagerMatchTemplate {
        css_version: CSS_VERSION,
        computer_name: &COMPUTER_NAME,
        cpu_brand: &CPU_BRAND,
        cores_count: *CPU_CORES,
        i18n,
        lang: params.lang,
        title: "Match Center".to_string(),
        sub_title: String::new(),
        sub_title_prefix: String::new(),
        sub_title_suffix: String::new(),
        sub_title_link: String::new(),
        sub_title_country_code: String::new(),
        header_color: "#333".to_string(),
        foreground_color: "#fff".to_string(),
        menu_sections,
        active_tab: "match",
        has_pending_match,
        opponent,
        competition,
        last_match: None,
    }
}
