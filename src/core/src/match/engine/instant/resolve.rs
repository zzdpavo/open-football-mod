use crate::r#match::engine::flow::rng::MatchRng;
use crate::r#match::engine::player::MatchPlayer;

use super::calibration;
use super::stats::PlayerStatsMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionType {
    Pass,
    Dribble,
    Tackle,
    Shot,
}

#[inline]
fn norm(skill: f32) -> f32 {
    (skill / 20.0).clamp(0.0, 1.0)
}

fn passing_composite(player: &MatchPlayer, _minute: u32) -> f32 {
    let s = &player.skills;
    norm(s.technical.passing) * 0.38
        + norm(s.technical.technique) * 0.20
        + norm(s.mental.vision) * 0.16
        + norm(s.mental.decisions) * 0.10
        + norm(s.mental.composure) * 0.08
        + norm(s.mental.concentration) * 0.08
}

fn defensive_composite(player: &MatchPlayer, _minute: u32) -> f32 {
    let s = &player.skills;
    norm(s.technical.tackling) * 0.24
        + norm(s.mental.positioning) * 0.17
        + norm(s.mental.anticipation) * 0.15
        + norm(s.technical.marking) * 0.13
        + norm(s.physical.strength) * 0.10
        + norm(s.physical.balance) * 0.07
        + norm(s.physical.agility) * 0.06
        + norm(s.mental.concentration) * 0.05
        + norm(s.mental.bravery) * 0.03
}

fn dribble_composite(player: &MatchPlayer, _minute: u32) -> f32 {
    let s = &player.skills;
    norm(s.technical.dribbling) * 0.25
        + norm(s.technical.technique) * 0.17
        + norm(s.mental.flair) * 0.10
        + norm(s.physical.agility) * 0.14
        + norm(s.physical.acceleration) * 0.10
        + norm(s.physical.balance) * 0.09
        + norm(s.mental.composure) * 0.07
        + norm(s.mental.decisions) * 0.05
        + norm(s.physical.strength) * 0.03
}

fn shooting_composite(player: &MatchPlayer, _minute: u32) -> f32 {
    let s = &player.skills;
    norm(s.technical.finishing) * 0.42
        + norm(s.mental.composure) * 0.22
        + norm(s.technical.first_touch) * 0.13
        + norm(s.technical.technique) * 0.10
        + norm(s.mental.decisions) * 0.08
        + norm(s.physical.balance) * 0.05
}

fn gk_shot_stopping(player: &MatchPlayer, _minute: u32) -> f32 {
    let s = &player.skills;
    norm(s.goalkeeping.reflexes) * 0.30
        + norm(s.goalkeeping.handling) * 0.20
        + norm(s.goalkeeping.one_on_ones) * 0.15
        + norm(s.physical.agility) * 0.10
        + norm(s.mental.positioning) * 0.10
        + norm(s.mental.concentration) * 0.08
        + norm(s.physical.jumping) * 0.07
}

pub fn resolve_pass(
    attacker: &MatchPlayer,
    defender: &MatchPlayer,
    minute: u32,
    rng: &MatchRng,
) -> bool {
    let atk = passing_composite(attacker, minute);
    let def = defensive_composite(defender, minute) * 0.4;
    let threshold = calibration::PASS_SUCCESS_BASE + (atk - def) * 0.5;
    rng.unit_f32() < threshold.clamp(0.1, 0.98)
}

pub fn resolve_dribble(
    attacker: &MatchPlayer,
    defender: &MatchPlayer,
    minute: u32,
    rng: &MatchRng,
) -> bool {
    let atk = dribble_composite(attacker, minute);
    let def = defensive_composite(defender, minute);
    let threshold = calibration::DRIBBLE_SUCCESS_BASE + (atk - def) * 0.5;
    rng.unit_f32() < threshold.clamp(0.05, 0.95)
}

pub fn resolve_tackle(
    defender: &MatchPlayer,
    attacker: &MatchPlayer,
    minute: u32,
    rng: &MatchRng,
) -> (bool, bool, bool) {
    let def = defensive_composite(defender, minute);
    let atk = dribble_composite(attacker, minute) * 0.5;
    let threshold = calibration::TACKLE_SUCCESS_BASE + (def - atk) * 0.5;
    let success = rng.unit_f32() < threshold.clamp(0.1, 0.95);

    let is_foul = !success && rng.unit_f32() < calibration::FOUL_RATE;
    let is_card = is_foul && rng.unit_f32() < calibration::CARD_RATE;
    (success, is_foul, is_card)
}

pub struct ShotResult {
    pub is_goal: bool,
    pub is_on_target: bool,
    pub xg: f32,
}

pub fn resolve_shot(
    shooter: &MatchPlayer,
    gk: &MatchPlayer,
    minute: u32,
    zone: f32,
    rng: &MatchRng,
) -> ShotResult {
    let shoot = shooting_composite(shooter, minute);
    let gk_skill = gk_shot_stopping(gk, minute);

    let base_xg = calibration::BASE_SHOT_XG
        + zone * (calibration::CLOSE_SHOT_XG - calibration::BASE_SHOT_XG);
    let xg = (base_xg * (0.5 + shoot * 0.8) * (1.0 - gk_skill * 0.5))
        .clamp(0.01, 0.55);

    let roll = rng.unit_f32();
    let is_goal = roll < xg;

    let on_target_threshold = 0.3 + shoot * 0.4;
    let is_on_target = is_goal || rng.unit_f32() < on_target_threshold;

    ShotResult {
        is_goal,
        is_on_target,
        xg,
    }
}

pub fn pick_action(
    zone: f32,
    rng: &MatchRng,
) -> ActionType {
    let shot_chance = calibration::SHOT_FREQUENCY * (0.5 + zone * 1.5);
    let roll = rng.unit_f32();

    if roll < shot_chance {
        return ActionType::Shot;
    }

    let remaining = 1.0 - shot_chance;
    let pass_share = 0.60;
    let dribble_share = 0.25;

    let norm_roll = (roll - shot_chance) / remaining;

    if norm_roll < pass_share {
        ActionType::Pass
    } else if norm_roll < pass_share + dribble_share {
        ActionType::Dribble
    } else {
        ActionType::Tackle
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardResult {
    None,
    Yellow,
    Red,
}

pub fn issue_card(
    player_id: u32,
    _team_id: u32,
    rng: &MatchRng,
    stats: &mut PlayerStatsMap,
) -> CardResult {
    let roll = rng.unit_f32();
    if roll < calibration::RED_CARD_RATE {
        if let Some(s) = stats.get_mut(&player_id) {
            s.red_cards += 1;
        }
        CardResult::Red
    } else {
        if let Some(s) = stats.get_mut(&player_id) {
            s.yellow_cards += 1;
            if s.yellow_cards >= 2 {
                s.red_cards = 1;
                return CardResult::Red;
            }
        }
        CardResult::Yellow
    }
}
