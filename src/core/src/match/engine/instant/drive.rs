use crate::r#match::engine::flow::rng::MatchRng;
use crate::r#match::engine::player::MatchPlayer;
use crate::r#match::engine::result::Score;
use crate::PlayerFieldPositionGroup;

use super::calibration;
use super::resolve::{self, ActionType, CardResult};
use super::stats::PlayerStatsMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrivePhase {
    BuildUp,
    Advance,
    Chance,
    Shot,
    Finished,
}

#[derive(Debug, Clone, Copy)]
pub struct DriveResult {
    pub goal: bool,
    pub scorer_id: Option<u32>,
    pub assist_id: Option<u32>,
    pub time_ms: u64,
    pub is_home: bool,
    pub on_target: bool,
    pub xg: f32,
    pub gk_id: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
pub struct DriveEvent {
    pub player_id: u32,
    pub team_id: u32,
    pub is_home_team: bool,
    pub event_type: DriveEventType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveEventType {
    RedCard,
}

pub struct DriveOutput {
    pub result: Option<DriveResult>,
    pub events: Vec<DriveEvent>,
}

pub struct DriveState<'a> {
    pub phase: DrivePhase,
    pub zone: f32,
    pub possession_team_is_home: bool,
    pub ball_carrier_idx: usize,
    pub home_players: &'a [MatchPlayer],
    pub away_players: &'a [MatchPlayer],
    pub score: &'a Score,
    pub actions_taken: u32,
    pub max_actions: u32,
    pub last_passer_id: Option<u32>,
    pub result: Option<DriveResult>,
}

impl<'a> DriveState<'a> {
    fn attackers(&self) -> &'a [MatchPlayer] {
        if self.possession_team_is_home {
            self.home_players
        } else {
            self.away_players
        }
    }

    fn defenders(&self) -> &'a [MatchPlayer] {
        if self.possession_team_is_home {
            self.away_players
        } else {
            self.home_players
        }
    }

    fn gk_of(&self, is_home: bool) -> &'a MatchPlayer {
        let team = if is_home {
            self.home_players
        } else {
            self.away_players
        };
        team.iter()
            .find(|p| {
                p.tactical_position.current_position.position_group()
                    == PlayerFieldPositionGroup::Goalkeeper
            })
            .unwrap_or(&team[0])
    }

    fn pick_attacker(&self, rng: &MatchRng) -> usize {
        let team = self.attackers();
        let mut candidates: Vec<(usize, f32)> = Vec::new();
        for (i, p) in team.iter().enumerate() {
            if p.tactical_position.current_position.position_group()
                == PlayerFieldPositionGroup::Goalkeeper
            {
                continue;
            }
            let weight = match p.tactical_position.current_position.position_group() {
                PlayerFieldPositionGroup::Forward => 3.0 + self.zone * 2.0,
                PlayerFieldPositionGroup::Midfielder => 2.0 + self.zone * 1.0,
                PlayerFieldPositionGroup::Defender => 1.0 - self.zone * 0.5,
                _ => 0.5,
            };
            candidates.push((i, weight.max(0.1)));
        }
        if candidates.is_empty() {
            return 0;
        }
        let total: f32 = candidates.iter().map(|(_, w)| *w).sum();
        let mut roll = rng.unit_f32() * total;
        for (idx, w) in &candidates {
            roll -= w;
            if roll <= 0.0 {
                return *idx;
            }
        }
        candidates.last().map(|(i, _)| *i).unwrap_or(0)
    }

    fn pick_defender(&self, rng: &MatchRng) -> usize {
        let team = self.defenders();
        let mut candidates: Vec<(usize, f32)> = Vec::new();
        for (i, p) in team.iter().enumerate() {
            if p.tactical_position.current_position.position_group()
                == PlayerFieldPositionGroup::Goalkeeper
            {
                continue;
            }
            let weight = match p.tactical_position.current_position.position_group() {
                PlayerFieldPositionGroup::Defender => 3.0 + self.zone * 2.0,
                PlayerFieldPositionGroup::Midfielder => 2.0,
                _ => 1.0,
            };
            candidates.push((i, weight.max(0.1)));
        }
        if candidates.is_empty() {
            return 0;
        }
        let total: f32 = candidates.iter().map(|(_, w)| *w).sum();
        let mut roll = rng.unit_f32() * total;
        for (idx, w) in &candidates {
            roll -= w;
            if roll <= 0.0 {
                return *idx;
            }
        }
        candidates.last().map(|(i, _)| *i).unwrap_or(0)
    }
}

pub fn simulate_drive(
    home_players: &[MatchPlayer],
    away_players: &[MatchPlayer],
    score: &Score,
    possession_is_home: bool,
    minute: u32,
    time_ms: u64,
    rng: &MatchRng,
    stats: &mut PlayerStatsMap,
) -> DriveOutput {
    let max_actions = rng.range_u64(
        calibration::ACTIONS_PER_DRIVE_MIN as u64,
        (calibration::ACTIONS_PER_DRIVE_MAX + 1) as u64,
    ) as u32;

    let mut events: Vec<DriveEvent> = Vec::new();

    let mut state = DriveState {
        phase: DrivePhase::BuildUp,
        zone: rng.range_f32(0.1, 0.3),
        possession_team_is_home: possession_is_home,
        ball_carrier_idx: 0,
        home_players,
        away_players,
        score,
        actions_taken: 0,
        max_actions,
        last_passer_id: None,
        result: None,
    };

    state.ball_carrier_idx = state.pick_attacker(rng);

    loop {
        if state.actions_taken >= state.max_actions || state.phase == DrivePhase::Finished {
            break;
        }

        state.actions_taken += 1;
        let minute_now = minute;

        let action = resolve::pick_action(state.zone, rng);

        match action {
            ActionType::Pass => {
                let atk = &state.attackers()[state.ball_carrier_idx];
                let def_idx = state.pick_defender(rng);
                let def = &state.defenders()[def_idx];

                if let Some(s) = stats.get_mut(&atk.id) {
                    s.passes_attempted += 1;
                }

                let success = resolve::resolve_pass(atk, def, minute_now, rng);
                if success {
                    if let Some(s) = stats.get_mut(&atk.id) {
                        s.passes_completed += 1;
                    }
                    state.last_passer_id = Some(atk.id);
                    state.ball_carrier_idx = state.pick_attacker(rng);
                    state.zone = (state.zone + rng.range_f32(0.05, 0.20)).min(1.0);
                    state.phase = advance_phase(state.phase, state.zone);
                } else {
                    state.possession_team_is_home = !state.possession_team_is_home;
                    state.zone = (state.zone - rng.range_f32(0.05, 0.15)).max(0.0);
                    state.ball_carrier_idx = state.pick_attacker(rng);
                    state.phase = DrivePhase::BuildUp;
                }
            }
            ActionType::Dribble => {
                let atk = &state.attackers()[state.ball_carrier_idx];
                let def_idx = state.pick_defender(rng);
                let def = &state.defenders()[def_idx];

                if let Some(s) = stats.get_mut(&atk.id) {
                    s.attempted_dribbles += 1;
                }

                let success = resolve::resolve_dribble(atk, def, minute_now, rng);
                if success {
                    if let Some(s) = stats.get_mut(&atk.id) {
                        s.successful_dribbles += 1;
                        s.progressive_carries += 1;
                    }
                    state.zone = (state.zone + rng.range_f32(0.08, 0.22)).min(1.0);
                    state.phase = advance_phase(state.phase, state.zone);
                } else {
                    if let Some(s) = stats.get_mut(&atk.id) {
                        s.miscontrols += 1;
                    }
                    state.possession_team_is_home = !state.possession_team_is_home;
                    state.ball_carrier_idx = state.pick_attacker(rng);
                    state.phase = DrivePhase::BuildUp;
                }
            }
            ActionType::Tackle => {
                let atk = &state.attackers()[state.ball_carrier_idx];
                let def_idx = state.pick_defender(rng);
                let defender = &state.defenders()[def_idx];

                let (success, is_foul, is_card) =
                    resolve::resolve_tackle(defender, atk, minute_now, rng);

                if success {
                    if let Some(s) = stats.get_mut(&defender.id) {
                        s.tackles += 1;
                    }
                    state.possession_team_is_home = !state.possession_team_is_home;
                    state.ball_carrier_idx = state.pick_attacker(rng);
                    state.phase = DrivePhase::BuildUp;
                } else if is_foul {
                    if let Some(s) = stats.get_mut(&defender.id) {
                        s.fouls += 1;
                    }
                    if is_card {
                        let card_result =
                            resolve::issue_card(defender.id, defender.team_id, rng, stats);
                        if card_result == CardResult::Red {
                            let is_home_team = !state.possession_team_is_home;
                            events.push(DriveEvent {
                                player_id: defender.id,
                                team_id: defender.team_id,
                                is_home_team,
                                event_type: DriveEventType::RedCard,
                            });
                            state.zone = (state.zone + 0.1).min(1.0);
                        }
                    }
                    state.zone = (state.zone + rng.range_f32(0.03, 0.10)).min(1.0);
                    state.phase = advance_phase(state.phase, state.zone);
                } else {
                    if let Some(s) = stats.get_mut(&defender.id) {
                        s.pressures += 1;
                    }
                    state.zone = (state.zone + rng.range_f32(0.01, 0.05)).min(1.0);
                }
            }
            ActionType::Shot => {
                let shooter = &state.attackers()[state.ball_carrier_idx];
                let opp_gk = state.gk_of(!state.possession_team_is_home);

                if let Some(s) = stats.get_mut(&shooter.id) {
                    s.shots_total += 1;
                    s.xg_chain += 0.0;
                }

                let shot_result =
                    resolve::resolve_shot(shooter, opp_gk, minute_now, state.zone, rng);

                if let Some(s) = stats.get_mut(&shooter.id) {
                    s.xg += shot_result.xg;
                    if shot_result.is_on_target {
                        s.shots_on_target += 1;
                    }
                }

                if let Some(s) = stats.get_mut(&opp_gk.id) {
                    s.shots_faced += 1;
                    if shot_result.is_on_target {
                        s.saves += if !shot_result.is_goal { 1 } else { 0 };
                    }
                }

                if shot_result.is_goal {
                    if let Some(s) = stats.get_mut(&shooter.id) {
                        s.goals += 1;
                    }
                    if let Some(assist_id) = state.last_passer_id {
                        if assist_id != shooter.id {
                            if let Some(s) = stats.get_mut(&assist_id) {
                                s.assists += 1;
                                s.key_passes += 1;
                            }
                        }
                    }
                    state.result = Some(DriveResult {
                        goal: true,
                        scorer_id: Some(shooter.id),
                        assist_id: state.last_passer_id,
                        time_ms,
                        is_home: state.possession_team_is_home,
                        on_target: shot_result.is_on_target,
                        xg: shot_result.xg,
                        gk_id: Some(opp_gk.id),
                    });
                }

                state.phase = DrivePhase::Finished;
            }
        }
    }

    DriveOutput {
        result: state.result,
        events,
    }
}

fn advance_phase(current: DrivePhase, zone: f32) -> DrivePhase {
    match current {
        DrivePhase::BuildUp if zone > 0.45 => DrivePhase::Advance,
        DrivePhase::Advance if zone > 0.70 => DrivePhase::Chance,
        DrivePhase::Chance if zone > 0.85 => DrivePhase::Shot,
        _ => current,
    }
}

pub fn estimate_drive_time_ms(rng: &MatchRng) -> u64 {
    rng.range_u64(
        calibration::DRIVE_TIME_MIN_MS,
        calibration::DRIVE_TIME_MAX_MS + 1,
    )
}
