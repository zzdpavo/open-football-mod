use crate::club::player::builder::PlayerBuilder;
use crate::r#match::engine::flow::rng::MatchRng;
use crate::r#match::engine::instant::InstantEngine;
use crate::r#match::engine::player::MatchPlayer;
use crate::r#match::MatchSquad;
use crate::shared::fullname::FullName;
use crate::{
    MatchTacticType, PersonAttributes, PlayerAttributes, PlayerPosition, PlayerPositionType,
    PlayerPositions, PlayerSkills, Tactics,
};
use chrono::NaiveDate;
use std::sync::atomic::{AtomicU32, Ordering};

static VALIDATION_ID: AtomicU32 = AtomicU32::new(50_000);

fn make_player_with_skill(pos: PlayerPositionType, team_id: u32, skill_level: f32) -> MatchPlayer {
    let id = VALIDATION_ID.fetch_add(1, Ordering::Relaxed);
    let mut skills = PlayerSkills::default();
    skills.technical.passing = skill_level;
    skills.technical.finishing = skill_level;
    skills.technical.tackling = skill_level;
    skills.technical.dribbling = skill_level;
    skills.technical.technique = skill_level;
    skills.technical.first_touch = skill_level;
    skills.technical.marking = skill_level;
    skills.technical.penalty_taking = skill_level;
    skills.mental.composure = skill_level;
    skills.mental.decisions = skill_level;
    skills.mental.positioning = skill_level;
    skills.mental.anticipation = skill_level;
    skills.mental.concentration = skill_level;
    skills.mental.vision = skill_level;
    skills.mental.flair = skill_level;
    skills.mental.bravery = skill_level;
    skills.physical.pace = skill_level;
    skills.physical.stamina = skill_level;
    skills.physical.agility = skill_level;
    skills.physical.acceleration = skill_level;
    skills.physical.strength = skill_level;
    skills.physical.balance = skill_level;
    skills.physical.jumping = skill_level;
    skills.goalkeeping.handling = skill_level;
    skills.goalkeeping.reflexes = skill_level;
    skills.goalkeeping.one_on_ones = skill_level;

    let player = PlayerBuilder::new()
        .id(id)
        .full_name(FullName::new("Val".into(), "Player".into()))
        .birth_date(NaiveDate::from_ymd_opt(1995, 6, 15).unwrap())
        .country_id(1)
        .attributes(PersonAttributes::default())
        .skills(skills)
        .positions(PlayerPositions {
            positions: vec![PlayerPosition {
                position: pos,
                level: 18,
            }],
        })
        .player_attributes(PlayerAttributes {
            condition: 9000,
            ..Default::default()
        })
        .build()
        .unwrap();

    MatchPlayer::from_player(team_id, &player, pos, false)
}

fn make_balanced_squad(team_id: u32) -> MatchSquad {
    make_squad_with_skill(team_id, 12.0)
}

fn make_squad_with_skill(team_id: u32, skill: f32) -> MatchSquad {
    let positions = vec![
        PlayerPositionType::Goalkeeper,
        PlayerPositionType::DefenderCenter,
        PlayerPositionType::DefenderCenter,
        PlayerPositionType::DefenderLeft,
        PlayerPositionType::DefenderRight,
        PlayerPositionType::MidfielderCenter,
        PlayerPositionType::MidfielderCenter,
        PlayerPositionType::MidfielderCenter,
        PlayerPositionType::ForwardLeft,
        PlayerPositionType::ForwardCenter,
        PlayerPositionType::ForwardRight,
    ];

    let main: Vec<MatchPlayer> = positions
        .into_iter()
        .map(|pos| make_player_with_skill(pos, team_id, skill))
        .collect();

    let subs: Vec<MatchPlayer> = (0..7)
        .map(|_| make_player_with_skill(PlayerPositionType::MidfielderCenter, team_id, skill))
        .collect();

    MatchSquad {
        team_id,
        team_name: format!("Team {}", team_id),
        tactics: Tactics::new(MatchTacticType::T442),
        main_squad: main,
        substitutes: subs,
        captain_id: None,
        vice_captain_id: None,
        penalty_taker_id: None,
        free_kick_taker_id: None,
        selection_omissions: Vec::new(),
    }
}

fn run_matches(count: u64) -> Vec<(u8, u8)> {
    let home = make_balanced_squad(1);
    let away = make_balanced_squad(2);
    let mut results = Vec::with_capacity(count as usize);
    for seed in 0..count {
        let result =
            InstantEngine::play_seeded(&home, &away, false, Some(seed), &MatchRng::from_seed(0));
        let score = result.score.as_ref().unwrap();
        results.push((score.home_team.get(), score.away_team.get()));
    }
    results
}

fn mean(data: &[f64]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    data.iter().sum::<f64>() / data.len() as f64
}

fn std_dev(data: &[f64]) -> f64 {
    if data.len() < 2 {
        return 0.0;
    }
    let m = mean(data);
    let variance = data.iter().map(|x| (x - m).powi(2)).sum::<f64>() / (data.len() - 1) as f64;
    variance.sqrt()
}

fn pearson_r(xs: &[f64], ys: &[f64]) -> f64 {
    if xs.len() != ys.len() || xs.len() < 2 {
        return 0.0;
    }
    let mx = mean(xs);
    let my = mean(ys);
    let mut num = 0.0;
    let mut dx2 = 0.0;
    let mut dy2 = 0.0;
    for i in 0..xs.len() {
        let dx = xs[i] - mx;
        let dy = ys[i] - my;
        num += dx * dy;
        dx2 += dx * dx;
        dy2 += dy * dy;
    }
    let denom = (dx2 * dy2).sqrt();
    if denom == 0.0 {
        0.0
    } else {
        num / denom
    }
}

#[test]
fn test_goal_distribution() {
    let results = run_matches(1500);
    let goals: Vec<f64> = results.iter().map(|(h, a)| (*h + *a) as f64).collect();
    let m = mean(&goals);
    let sd = std_dev(&goals);

    assert!(
        m >= 2.5 && m <= 5.0,
        "goal mean {:.3} outside plausible range [2.5, 5.0]",
        m
    );
    assert!(
        sd >= 1.0 && sd <= 2.5,
        "goal std dev {:.3} outside plausible range [1.0, 2.5]",
        sd
    );

    let max_goals = goals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    assert!(
        max_goals <= 12.0,
        "max goals {} implausibly high",
        max_goals
    );
}

#[test]
fn test_home_win_rate() {
    let results = run_matches(1500);
    let home_wins = results
        .iter()
        .filter(|(h, a)| h > a)
        .count() as f64;
    let draws = results
        .iter()
        .filter(|(h, a)| h == a)
        .count() as f64;
    let rate = home_wins / results.len() as f64;
    let draw_rate = draws / results.len() as f64;

    assert!(
        rate >= 0.35 && rate <= 0.50,
        "home win rate {:.3} outside plausible range [35%, 50%]",
        rate
    );
    assert!(
        draw_rate >= 0.15 && draw_rate <= 0.35,
        "draw rate {:.3} outside plausible range [15%, 35%]",
        draw_rate
    );
}

#[test]
fn test_quality_result_correlation() {
    let num_matches: u64 = 1500;
    let mut skill_diffs: Vec<f64> = Vec::with_capacity(num_matches as usize);
    let mut goal_diffs: Vec<f64> = Vec::with_capacity(num_matches as usize);

    let strong = make_squad_with_skill(1, 15.0);
    let weak = make_squad_with_skill(2, 8.0);
    let even = make_squad_with_skill(3, 12.0);

    let matchups: [(f64, &MatchSquad, &MatchSquad); 3] = [
        (7.0, &strong, &weak),
        (0.0, &even, &even),
        (-7.0, &weak, &strong),
    ];

    for seed in 0..num_matches {
        let mi = (seed % 3) as usize;
        let (sdiff, home, away) = &matchups[mi];
        let result =
            InstantEngine::play_seeded(home, away, false, Some(seed), &MatchRng::from_seed(0));
        let score = result.score.as_ref().unwrap();
        let gdiff = score.home_team.get() as f64 - score.away_team.get() as f64;
        skill_diffs.push(*sdiff);
        goal_diffs.push(gdiff);
    }

    let r = pearson_r(&skill_diffs, &goal_diffs);
    assert!(
        r > 0.6,
        "Pearson r {:.4} not > 0.6 for skill-goal differential correlation",
        r
    );
}

#[test]
fn test_player_stat_distributions() {
    let home = make_balanced_squad(1);
    let away = make_balanced_squad(2);
    let num_matches: u64 = 1000;

    let mut total_pass_attempts: u64 = 0;
    let mut total_pass_completed: u64 = 0;
    let mut total_shot_attempts: u64 = 0;
    let mut total_shot_on_target: u64 = 0;
    let mut total_tackles: u64 = 0;
    let mut total_dribble_attempts: u64 = 0;
    let mut total_dribble_success: u64 = 0;

    for seed in 0..num_matches {
        let result = InstantEngine::play_seeded(
            &home,
            &away,
            false,
            Some(seed),
            &MatchRng::from_seed(0),
        );
        for (_, s) in &result.player_stats {
            total_pass_attempts += s.passes_attempted as u64;
            total_pass_completed += s.passes_completed as u64;
            total_shot_attempts += s.shots_total as u64;
            total_shot_on_target += s.shots_on_target as u64;
            total_tackles += s.tackles as u64;
            total_dribble_attempts += s.attempted_dribbles as u64;
            total_dribble_success += s.successful_dribbles as u64;
        }
    }

    let pass_pct = if total_pass_attempts > 0 {
        total_pass_completed as f64 / total_pass_attempts as f64
    } else {
        0.0
    };
    let shot_acc = if total_shot_attempts > 0 {
        total_shot_on_target as f64 / total_shot_attempts as f64
    } else {
        0.0
    };
    let dribble_pct = if total_dribble_attempts > 0 {
        total_dribble_success as f64 / total_dribble_attempts as f64
    } else {
        0.0
    };

    assert!(
        pass_pct >= 0.55 && pass_pct <= 1.0,
        "pass completion {:.3} outside plausible range [55%, 100%]",
        pass_pct
    );
    assert!(
        shot_acc >= 0.15 && shot_acc <= 0.65,
        "shot accuracy {:.3} outside plausible range [15%, 65%]",
        shot_acc
    );
    assert!(
        dribble_pct >= 0.25 && dribble_pct <= 0.75,
        "dribble success {:.3} outside plausible range [25%, 75%]",
        dribble_pct
    );
    assert!(
        total_tackles > 0,
        "expected some tackles across {} matches",
        num_matches
    );
}

#[test]
fn test_determinism_across_seeds() {
    let home = make_balanced_squad(1);
    let away = make_balanced_squad(2);

    for seed in [0u64, 1, 42, 999, 12345, 0xFFFF_FFFF].iter() {
        let r1 = InstantEngine::play_seeded(
            &home,
            &away,
            false,
            Some(*seed),
            &MatchRng::from_seed(0),
        );
        let r2 = InstantEngine::play_seeded(
            &home,
            &away,
            false,
            Some(*seed),
            &MatchRng::from_seed(0),
        );
        let s1 = r1.score.as_ref().unwrap();
        let s2 = r2.score.as_ref().unwrap();
        assert_eq!(
            s1.home_team.get(),
            s2.home_team.get(),
            "home goals differ for seed {}",
            seed
        );
        assert_eq!(
            s1.away_team.get(),
            s2.away_team.get(),
            "away goals differ for seed {}",
            seed
        );
    }
}
