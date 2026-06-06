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
#[template(path = "manager/tactics/index.html")]
pub struct ManagerTacticsTemplate {
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
    pub formations: Vec<String>,
    pub default_formation: &'static str,
    pub approaches: Vec<String>,
    pub players: Vec<PlayerInfo>,
}

pub struct PlayerInfo {
    pub id: u32,
    pub name: String,
    pub position: String,
    pub ability: u8,
    pub condition: u8,
}

fn get_conditions(condition: i16) -> u8 {
    (100f32 * (condition as f32 / 10000.0)) as u8
}

pub async fn manager_tactics(
    State(state): State<GameAppData>,
    Path(params): Path<ManagerRequest>,
) -> impl IntoResponse {
    let i18n = state.i18n.for_lang(&params.lang);

    let guard = state.data.read().await;
    let data = guard.as_ref();

    let (has_pending_match, opponent, competition, players) = match data {
        Some(d) => {
            let gs = d.game_state();
            let um = gs.user_manager.as_ref();
            let (has, opp, comp) = match &gs.pending_decision {
                Some(core::career::interactive::DecisionPoint::PreMatch { opponent, competition, .. }) => {
                    (true, opponent.clone(), competition.clone())
                }
                _ => (false, String::new(), String::new()),
            };
            let squad: Vec<PlayerInfo> = if let Some(mgr) = um {
                let club = d.club(mgr.club_id);
                club.map(|c| {
                    c.teams.main().map(|t| {
                        t.players().iter().map(|p| PlayerInfo {
                            id: p.id,
                            name: format!("{}", p.full_name),
                            position: format!("{}", p.position()),
                            ability: p.skills.calculate_ability(),
                            condition: get_conditions(p.player_attributes.condition),
                        }).collect()
                    }).unwrap_or_default()
                }).unwrap_or_default()
            } else {
                Vec::new()
            };
            (has, opp, comp, squad)
        }
        None => (false, String::new(), String::new(), Vec::new()),
    };

    let formations = vec![
        "T442".to_string(), "T433".to_string(), "T4231".to_string(), "T4321".to_string(),
        "T352".to_string(), "T343".to_string(), "T451".to_string(), "T532".to_string(),
        "T541".to_string(), "T4411".to_string(), "T4141".to_string(), "T41212".to_string(),
    ];
    let approaches = vec![
        "VeryDefensive".to_string(), "Defensive".to_string(), "SlightlyDefensive".to_string(),
        "Balanced".to_string(), "SlightlyAttacking".to_string(), "Attacking".to_string(),
        "VeryAttacking".to_string(),
    ];

    let has_career = data.map(|d| d.game_state().user_manager.is_some()).unwrap_or(false);
    let current_path = format!("/{}/manager/tactics", &params.lang);
    let menu_sections = views::manager_menu(&i18n, &params.lang, &current_path, has_career);

    ManagerTacticsTemplate {
        css_version: CSS_VERSION,
        computer_name: &COMPUTER_NAME,
        cpu_brand: &CPU_BRAND,
        cores_count: *CPU_CORES,
        i18n,
        lang: params.lang,
        title: "Tactics".to_string(),
        sub_title: String::new(),
        sub_title_prefix: String::new(),
        sub_title_suffix: String::new(),
        sub_title_link: String::new(),
        sub_title_country_code: String::new(),
        header_color: "#333".to_string(),
        foreground_color: "#fff".to_string(),
        menu_sections,
        active_tab: "tactics",
        has_pending_match,
        opponent,
        competition,
        formations,
        default_formation: "T433",
        approaches,
        players,
    }
}
