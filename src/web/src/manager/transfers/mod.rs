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
#[template(path = "manager/transfers/index.html")]
pub struct ManagerTransfersTemplate {
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
    pub budget: Option<BudgetInfo>,
    pub players: Vec<PlayerInfo>,
}

pub struct BudgetInfo {
    pub total: String,
    pub spent: String,
    pub available: String,
}

pub struct PlayerInfo {
    pub id: u32,
    pub name: String,
    pub position: String,
    pub condition: u8,
}

fn get_conditions(condition: i16) -> u8 {
    (100f32 * (condition as f32 / 10000.0)) as u8
}

pub async fn manager_transfers(
    State(state): State<GameAppData>,
    Path(params): Path<ManagerRequest>,
) -> impl IntoResponse {
    let i18n = state.i18n.for_lang(&params.lang);

    let guard = state.data.read().await;
    let data = guard.as_ref();

    let (budget, players) = match data {
        Some(d) => {
            let gs = d.game_state();
            let budget_info = gs.transfer_budget().map(|b| BudgetInfo {
                total: format!("{}", b.total),
                spent: format!("{}", b.spent),
                available: format!("{}", b.available()),
            });
            let um = gs.user_manager.as_ref();
            let squad: Vec<PlayerInfo> = if let Some(mgr) = um {
                let club = d.club(mgr.club_id);
                club.map(|c| {
                    c.teams.main().map(|t| {
                        t.players().iter().map(|p| PlayerInfo {
                            id: p.id,
                            name: format!("{}", p.full_name),
                            position: format!("{}", p.position()),
                            condition: get_conditions(p.player_attributes.condition),
                        }).collect()
                    }).unwrap_or_default()
                }).unwrap_or_default()
            } else {
                Vec::new()
            };
            (budget_info, squad)
        }
        None => (None, Vec::new()),
    };

    let has_career = data.map(|d| d.game_state().user_manager.is_some()).unwrap_or(false);
    let current_path = format!("/{}/manager/transfers", &params.lang);
    let menu_sections = views::manager_menu(&i18n, &params.lang, &current_path, has_career);

    ManagerTransfersTemplate {
        css_version: CSS_VERSION,
        computer_name: &COMPUTER_NAME,
        cpu_brand: &CPU_BRAND,
        cores_count: *CPU_CORES,
        i18n,
        lang: params.lang,
        title: "Transfers".to_string(),
        sub_title: String::new(),
        sub_title_prefix: String::new(),
        sub_title_suffix: String::new(),
        sub_title_link: String::new(),
        sub_title_country_code: String::new(),
        header_color: "#333".to_string(),
        foreground_color: "#fff".to_string(),
        menu_sections,
        active_tab: "transfers",
        budget,
        players,
    }
}
