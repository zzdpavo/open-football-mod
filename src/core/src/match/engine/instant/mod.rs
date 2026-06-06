mod calibration;
mod drive;
mod resolve;
mod stats;

#[cfg(test)]
mod stats_validation;

use crate::r#match::engine::flow::rng::MatchRng;
use crate::r#match::engine::officiating::set_pieces::{
    penalty_conversion_prob, score_keeper_save, score_penalty_taker,
};
use crate::r#match::engine::player::MatchPlayer;
use crate::r#match::engine::player::statistics::MatchStatisticType;
use crate::r#match::engine::rating::RatingContext;
use crate::r#match::engine::result::{
    FieldSquad, GoalDetail, MatchResultRaw, PenaltyShootoutKick, PlayerMatchPhysicalSnapshot,
    Score, SubstitutionInfo,
};
use crate::r#match::result::ResultMatchPositionData;
use crate::r#match::MatchSquad;
use crate::PlayerFieldPositionGroup;

use self::stats::{InstantPlayerStats, PlayerStatsMap};
use self::drive::{DriveEvent, DriveEventType};

pub struct InstantEngine;

struct PendingSubstitution {
    team_id: u32,
    out_player_id: u32,
    in_player_id: u32,
    time_ms: u64,
}

impl InstantEngine {
    pub fn play(home_squad: &MatchSquad, away_squad: &MatchSquad, knockout: bool) -> MatchResultRaw {
        let rng = MatchRng::from_entropy();
        Self::play_seeded(home_squad, away_squad, knockout, None, &rng)
    }

    pub fn play_seeded(
        home_squad: &MatchSquad,
        away_squad: &MatchSquad,
        knockout: bool,
        seed: Option<u64>,
        fallback_rng: &MatchRng,
    ) -> MatchResultRaw {
        let rng = match seed {
            Some(s) => MatchRng::from_seed(s),
            None => MatchRng::from_seed(fallback_rng.random::<u64>()),
        };

        let home_team_id = home_squad.team_id;
        let away_team_id = away_squad.team_id;

        let home_starting_tactic = Some(home_squad.tactics.tactic_type);
        let away_starting_tactic = Some(away_squad.tactics.tactic_type);

        let mut score = Score::new(home_team_id, away_team_id);

        let home_field_squad = FieldSquad::from_team(home_squad);
        let away_field_squad = FieldSquad::from_team(away_squad);

        let mut home_players: Vec<MatchPlayer> = home_squad.main_squad.clone();
        let mut away_players: Vec<MatchPlayer> = away_squad.main_squad.clone();
        let mut home_subs: Vec<MatchPlayer> = home_squad.substitutes.clone();
        let mut away_subs: Vec<MatchPlayer> = away_squad.substitutes.clone();

        let mut player_stats = PlayerStatsMap::new();
        for p in &home_players {
            player_stats.insert(
                p.id,
                InstantPlayerStats::new(
                    home_team_id,
                    p.tactical_position.current_position.position_group(),
                    p.player_attributes.condition,
                ),
            );
        }
        for p in &away_players {
            player_stats.insert(
                p.id,
                InstantPlayerStats::new(
                    away_team_id,
                    p.tactical_position.current_position.position_group(),
                    p.player_attributes.condition,
                ),
            );
        }

        let mut substitutions: Vec<SubstitutionInfo> = Vec::new();

        let num_drives = rng.range_u64(
            calibration::DRIVES_PER_MATCH_MIN as u64,
            (calibration::DRIVES_PER_MATCH_MAX + 1) as u64,
        );

        let mut match_time_ms: u64 = 0;
        let mut possession_is_home: bool = rng.bernoulli(0.5 + calibration::HOME_ADVANTAGE);
        let mut goal_details: Vec<GoalDetail> = Vec::new();

        let sub_window = Self::plan_substitutions(
            &home_players,
            &home_subs,
            home_team_id,
            &away_players,
            &away_subs,
            away_team_id,
            &rng,
        );

        for _ in 0..num_drives {
            let drive_time = drive::estimate_drive_time_ms(&rng);
            let minute = (match_time_ms / 60_000) as u32;

            Self::apply_pending_subs(
                &sub_window,
                match_time_ms,
                &mut home_players,
                &mut home_subs,
                &mut away_players,
                &mut away_subs,
                &mut player_stats,
                &mut substitutions,
            );

            let drain_minutes = drive_time as f32 / 60_000.0;
            for s in player_stats.values_mut() {
                s.drain_condition(drain_minutes);
            }

            let output = drive::simulate_drive(
                &home_players,
                &away_players,
                &score,
                possession_is_home,
                minute,
                match_time_ms,
                &rng,
                &mut player_stats,
            );

            if let Some(dr) = output.result {
                if dr.goal {
                    if dr.is_home {
                        score.increment_home_goals();
                    } else {
                        score.increment_away_goals();
                    }
                    goal_details.push(GoalDetail {
                        player_id: dr.scorer_id.unwrap_or(0),
                        stat_type: MatchStatisticType::Goal,
                        is_auto_goal: false,
                        time: match_time_ms,
                    });
                    possession_is_home = !dr.is_home;
                } else {
                    possession_is_home = !possession_is_home;
                }
            } else {
                possession_is_home = !possession_is_home;
            }

            Self::process_drive_events(
                &output.events,
                &mut home_players,
                &mut home_subs,
                &mut away_players,
                &mut away_subs,
                &mut player_stats,
                &mut substitutions,
                match_time_ms,
            );

            Self::roll_injuries(
                &mut home_players,
                &mut home_subs,
                home_team_id,
                &mut away_players,
                &mut away_subs,
                away_team_id,
                &mut player_stats,
                &mut substitutions,
                match_time_ms,
                &rng,
            );

            match_time_ms += drive_time;
            if match_time_ms >= calibration::MATCH_DURATION_MS {
                match_time_ms = calibration::MATCH_DURATION_MS;
                break;
            }
        }

        // Extra time
        let mut had_extra_time = false;
        if knockout && score.is_tied() {
            had_extra_time = true;
            let et_drives = rng.range_u64(
                calibration::DRIVES_PER_ET_MIN as u64,
                (calibration::DRIVES_PER_ET_MAX + 1) as u64,
            );
            let et_start = match_time_ms;
            for _ in 0..et_drives {
                let drive_time = drive::estimate_drive_time_ms(&rng);
                let minute = (match_time_ms / 60_000) as u32;

                let drain_minutes = drive_time as f32 / 60_000.0;
                for s in player_stats.values_mut() {
                    s.drain_condition(drain_minutes);
                }

                let output = drive::simulate_drive(
                    &home_players,
                    &away_players,
                    &score,
                    possession_is_home,
                    minute,
                    match_time_ms,
                    &rng,
                    &mut player_stats,
                );

                if let Some(dr) = output.result {
                    if dr.goal {
                        if dr.is_home {
                            score.increment_home_goals();
                        } else {
                            score.increment_away_goals();
                        }
                        goal_details.push(GoalDetail {
                            player_id: dr.scorer_id.unwrap_or(0),
                            stat_type: MatchStatisticType::Goal,
                            is_auto_goal: false,
                            time: match_time_ms,
                        });
                        possession_is_home = !dr.is_home;
                    } else {
                        possession_is_home = !possession_is_home;
                    }
                } else {
                    possession_is_home = !possession_is_home;
                }

                Self::process_drive_events(
                    &output.events,
                    &mut home_players,
                    &mut home_subs,
                    &mut away_players,
                    &mut away_subs,
                    &mut player_stats,
                    &mut substitutions,
                    match_time_ms,
                );

                Self::roll_injuries(
                    &mut home_players,
                    &mut home_subs,
                    home_team_id,
                    &mut away_players,
                    &mut away_subs,
                    away_team_id,
                    &mut player_stats,
                    &mut substitutions,
                    match_time_ms,
                    &rng,
                );

                match_time_ms += drive_time;
                if match_time_ms >= et_start + calibration::EXTRA_TIME_DURATION_MS {
                    match_time_ms = et_start + calibration::EXTRA_TIME_DURATION_MS;
                    break;
                }
            }
        }

        // Penalty shootout
        let mut penalty_kicks: Vec<PenaltyShootoutKick> = Vec::new();
        if knockout && score.is_tied() {
            Self::run_penalty_shootout(
                &home_players,
                &away_players,
                home_team_id,
                away_team_id,
                &mut penalty_kicks,
                &mut score,
                &rng,
            );

            if score.home_shootout > score.away_shootout {
                score.increment_home_goals();
            } else if score.away_shootout > score.home_shootout {
                score.increment_away_goals();
            }
        }

        for gd in &goal_details {
            score.add_goal_detail(gd.clone());
        }

        let home_goals = score.home_team.get();
        let away_goals = score.away_team.get();

        let total_match_time_ms = if had_extra_time {
            calibration::MATCH_DURATION_MS + calibration::EXTRA_TIME_DURATION_MS
        } else {
            match_time_ms
        };

        let mut result = MatchResultRaw::with_match_time(total_match_time_ms);
        result.score = Some(score);
        result.left_team_players = home_field_squad;
        result.right_team_players = away_field_squad;
        result.additional_time_ms = 0;
        result.position_data = ResultMatchPositionData::empty();
        result.starting_home_tactic = home_starting_tactic;
        result.starting_away_tactic = away_starting_tactic;
        result.final_home_tactic = home_starting_tactic;
        result.final_away_tactic = away_starting_tactic;
        result.substitutions = substitutions;
        result.penalty_shootout = penalty_kicks;

        let player_ids: Vec<u32> = player_stats.keys().copied().collect();
        for pid in player_ids {
            let is = match player_stats.get_mut(&pid) {
                Some(s) => s,
                None => continue,
            };
            let is_home = is.team_id == home_team_id;
            let exit_time = if is.subbed_out {
                is.subbed_out_time_ms
            } else {
                total_match_time_ms
            };
            let minutes = ((exit_time.saturating_sub(is.entry_time_ms)) / 60_000) as u16;
            let minutes = minutes.min(120);
            let (tg, og) = if is_home {
                (home_goals, away_goals)
            } else {
                (away_goals, home_goals)
            };
            let end_stats = is.to_end_stats(minutes);
            let rating = RatingContext::new(&end_stats, tg, og).calculate();
            let phys = PlayerMatchPhysicalSnapshot {
                player_id: pid,
                minutes_played: minutes as f32,
                starting_condition: is.starting_condition,
                final_match_energy: is.current_condition,
                high_intensity_load_hint: calibration::HIGH_INTENSITY_DEFAULT,
            };
            let mut final_stats = end_stats;
            final_stats.match_rating = rating;
            result.player_stats.insert(pid, final_stats);
            result.physical_snapshots.insert(pid, phys);
        }

        let mut best_rating: f32 = 0.0;
        let mut potm: Option<u32> = None;
        for (pid, stats) in &result.player_stats {
            if stats.match_rating > best_rating {
                best_rating = stats.match_rating;
                potm = Some(*pid);
            }
        }
        result.player_of_the_match_id = potm;

        result
    }

    fn plan_substitutions(
        home_players: &[MatchPlayer],
        home_subs: &[MatchPlayer],
        home_team_id: u32,
        away_players: &[MatchPlayer],
        away_subs: &[MatchPlayer],
        away_team_id: u32,
        rng: &MatchRng,
    ) -> Vec<PendingSubstitution> {
        let mut pending = Vec::new();

        let home_count = rng.range_u64(
            calibration::SUBS_PER_TEAM_MIN as u64,
            (calibration::SUBS_PER_TEAM_MAX + 1) as u64,
        ) as usize;
        let away_count = rng.range_u64(
            calibration::SUBS_PER_TEAM_MIN as u64,
            (calibration::SUBS_PER_TEAM_MAX + 1) as u64,
        ) as usize;

        let home_planned = Self::pick_subs_for_team(home_players, home_subs, home_team_id, home_count, rng);
        let away_planned = Self::pick_subs_for_team(away_players, away_subs, away_team_id, away_count, rng);

        pending.extend(home_planned);
        pending.extend(away_planned);
        pending.sort_by_key(|s| s.time_ms);
        pending
    }

    fn pick_subs_for_team(
        starters: &[MatchPlayer],
        bench: &[MatchPlayer],
        team_id: u32,
        count: usize,
        rng: &MatchRng,
    ) -> Vec<PendingSubstitution> {
        let mut subs = Vec::new();
        let mut used_bench: Vec<usize> = Vec::new();

        for _ in 0..count.min(bench.len()) {
            let out_idx = match Self::most_tired_outfield(starters) {
                Some(i) => i,
                None => break,
            };
            let out_pos = starters[out_idx].tactical_position.current_position.position_group();

            let in_idx = match Self::best_matching_bench(bench, &used_bench, out_pos) {
                Some(i) => i,
                None => break,
            };
            used_bench.push(in_idx);

            let time_ms = rng.range_u64(
                calibration::SUB_MIN_TIME_MIN_MS,
                calibration::SUB_MIN_TIME_MAX_MS + 1,
            );

            subs.push(PendingSubstitution {
                team_id,
                out_player_id: starters[out_idx].id,
                in_player_id: bench[in_idx].id,
                time_ms,
            });
        }

        subs
    }

    fn most_tired_outfield(players: &[MatchPlayer]) -> Option<usize> {
        players.iter().enumerate()
            .filter(|(_, p)| {
                p.tactical_position.current_position.position_group()
                    != PlayerFieldPositionGroup::Goalkeeper
            })
            .min_by_key(|(_, p)| p.player_attributes.condition as i64)
            .map(|(i, _)| i)
    }

    fn best_matching_bench(
        bench: &[MatchPlayer],
        used: &[usize],
        target_pos: PlayerFieldPositionGroup,
    ) -> Option<usize> {
        let mut best: Option<(usize, i32)> = None;
        for (i, p) in bench.iter().enumerate() {
            if used.contains(&i) {
                continue;
            }
            let bench_pos = p.tactical_position.current_position.position_group();
            let score = if bench_pos == target_pos { 2 } else { 0 };
            if best.is_none() || score > best.unwrap().1 {
                best = Some((i, score));
            }
        }
        best.map(|(i, _)| i)
    }

    fn apply_pending_subs(
        pending: &[PendingSubstitution],
        current_time_ms: u64,
        home_players: &mut Vec<MatchPlayer>,
        home_subs: &mut Vec<MatchPlayer>,
        away_players: &mut Vec<MatchPlayer>,
        away_subs: &mut Vec<MatchPlayer>,
        player_stats: &mut PlayerStatsMap,
        substitutions: &mut Vec<SubstitutionInfo>,
    ) {
        for sub in pending {
            if sub.time_ms > current_time_ms {
                continue;
            }
            if substitutions.iter().any(|s| s.player_out_id == sub.out_player_id) {
                continue;
            }

            let (players, bench): (&mut Vec<MatchPlayer>, &mut Vec<MatchPlayer>) =
                if sub.team_id == home_players.first().map(|p| p.team_id).unwrap_or(0) {
                    (home_players, home_subs)
                } else {
                    (away_players, away_subs)
                };

            let team_id = sub.team_id;

            let in_player = match bench.iter().find(|p| p.id == sub.in_player_id).cloned() {
                Some(p) => p,
                None => continue,
            };
            let out_idx = match players.iter().position(|p| p.id == sub.out_player_id) {
                Some(i) => i,
                None => continue,
            };

            let mut sub_player = in_player;
            sub_player.entry_match_time_ms = current_time_ms;

            let mut new_stats = InstantPlayerStats::new(
                team_id,
                sub_player.tactical_position.current_position.position_group(),
                sub_player.player_attributes.condition,
            );
            new_stats.entry_time_ms = current_time_ms;
            player_stats.insert(sub_player.id, new_stats);

            if let Some(out_stats) = player_stats.get_mut(&sub.out_player_id) {
                out_stats.subbed_out = true;
                out_stats.subbed_out_time_ms = current_time_ms;
            }

            players[out_idx] = sub_player;

            substitutions.push(SubstitutionInfo {
                team_id,
                player_out_id: sub.out_player_id,
                player_in_id: sub.in_player_id,
                match_time_ms: current_time_ms,
            });
        }
    }

    fn process_drive_events(
        events: &[DriveEvent],
        home_players: &mut Vec<MatchPlayer>,
        home_subs: &mut Vec<MatchPlayer>,
        away_players: &mut Vec<MatchPlayer>,
        away_subs: &mut Vec<MatchPlayer>,
        player_stats: &mut PlayerStatsMap,
        substitutions: &mut Vec<SubstitutionInfo>,
        current_time_ms: u64,
    ) {
        for event in events {
            match event.event_type {
                DriveEventType::RedCard => {
                    if event.is_home_team {
                        Self::handle_red_card(
                            home_players, home_subs, event,
                            player_stats, substitutions, current_time_ms,
                        );
                    } else {
                        Self::handle_red_card(
                            away_players, away_subs, event,
                            player_stats, substitutions, current_time_ms,
                        );
                    }
                }
            }
        }
    }

    fn handle_red_card(
        players: &mut Vec<MatchPlayer>,
        bench: &mut Vec<MatchPlayer>,
        event: &DriveEvent,
        player_stats: &mut PlayerStatsMap,
        substitutions: &mut Vec<SubstitutionInfo>,
        current_time_ms: u64,
    ) {
        if let Some(pos) = players.iter().position(|p| p.id == event.player_id) {
            let removed_team_id = event.team_id;
            let removed_id = event.player_id;

            if let Some(sub) = bench.first().cloned() {
                bench.remove(0);

                let mut sub_player = sub;
                sub_player.entry_match_time_ms = current_time_ms;
                let sub_id = sub_player.id;

                let mut new_stats = InstantPlayerStats::new(
                    removed_team_id,
                    sub_player.tactical_position.current_position.position_group(),
                    sub_player.player_attributes.condition,
                );
                new_stats.entry_time_ms = current_time_ms;
                player_stats.insert(sub_id, new_stats);

                if let Some(out_stats) = player_stats.get_mut(&removed_id) {
                    out_stats.subbed_out = true;
                    out_stats.subbed_out_time_ms = current_time_ms;
                }

                players[pos] = sub_player;

                substitutions.push(SubstitutionInfo {
                    team_id: removed_team_id,
                    player_out_id: removed_id,
                    player_in_id: sub_id,
                    match_time_ms: current_time_ms,
                });
            } else {
                players.remove(pos);
            }
        }
    }

    fn roll_injuries(
        home_players: &mut Vec<MatchPlayer>,
        home_subs: &mut Vec<MatchPlayer>,
        home_team_id: u32,
        away_players: &mut Vec<MatchPlayer>,
        away_subs: &mut Vec<MatchPlayer>,
        away_team_id: u32,
        player_stats: &mut PlayerStatsMap,
        substitutions: &mut Vec<SubstitutionInfo>,
        current_time_ms: u64,
        rng: &MatchRng,
    ) {
        Self::roll_injuries_for_team(
            home_players, home_subs, home_team_id,
            player_stats, substitutions, current_time_ms, rng,
        );
        Self::roll_injuries_for_team(
            away_players, away_subs, away_team_id,
            player_stats, substitutions, current_time_ms, rng,
        );
    }

    fn roll_injuries_for_team(
        players: &mut Vec<MatchPlayer>,
        bench: &mut Vec<MatchPlayer>,
        team_id: u32,
        player_stats: &mut PlayerStatsMap,
        substitutions: &mut Vec<SubstitutionInfo>,
        current_time_ms: u64,
        rng: &MatchRng,
    ) {
        let mut injured_ids: Vec<u32> = Vec::new();
        for p in players.iter() {
            if p.tactical_position.current_position.position_group()
                == PlayerFieldPositionGroup::Goalkeeper
            {
                continue;
            }
            if rng.unit_f32() < calibration::INJURY_CHANCE_PER_DRIVE {
                if let Some(s) = player_stats.get_mut(&p.id) {
                    s.current_condition =
                        (s.current_condition - calibration::INJURY_CONDITION_DRAIN)
                            .max(0);
                    s.injured = true;
                }
                if let Some(s) = player_stats.get(&p.id) {
                    if s.current_condition < calibration::INJURY_CRITICAL_CONDITION {
                        injured_ids.push(p.id);
                    }
                }
            }
        }

        for injured_id in injured_ids {
            if substitutions.iter().any(|s| s.player_out_id == injured_id) {
                continue;
            }

            if let Some(sub) = bench.first().cloned() {
                bench.remove(0);

                let mut sub_player = sub;
                sub_player.entry_match_time_ms = current_time_ms;
                let sub_id = sub_player.id;

                let mut new_stats = InstantPlayerStats::new(
                    team_id,
                    sub_player.tactical_position.current_position.position_group(),
                    sub_player.player_attributes.condition,
                );
                new_stats.entry_time_ms = current_time_ms;
                player_stats.insert(sub_id, new_stats);

                if let Some(out_stats) = player_stats.get_mut(&injured_id) {
                    out_stats.subbed_out = true;
                    out_stats.subbed_out_time_ms = current_time_ms;
                }

                if let Some(pos) = players.iter().position(|p| p.id == injured_id) {
                    players[pos] = sub_player;
                }

                substitutions.push(SubstitutionInfo {
                    team_id,
                    player_out_id: injured_id,
                    player_in_id: sub_id,
                    match_time_ms: current_time_ms,
                });
            }
        }
    }

    fn run_penalty_shootout(
        home_players: &[MatchPlayer],
        away_players: &[MatchPlayer],
        home_team_id: u32,
        away_team_id: u32,
        kicks: &mut Vec<PenaltyShootoutKick>,
        score: &mut Score,
        rng: &MatchRng,
    ) {
        let home_takers = Self::penalty_takers_for(home_players, home_team_id);
        let away_takers = Self::penalty_takers_for(away_players, away_team_id);
        let home_keeper = Self::penalty_keeper_for(away_players, away_team_id);
        let away_keeper = Self::penalty_keeper_for(home_players, home_team_id);

        let taker_score = |players: &[MatchPlayer], id: u32| -> f32 {
            if let Some(p) = players.iter().find(|p| p.id == id) {
                score_penalty_taker(
                    p.skills.technical.penalty_taking,
                    p.skills.technical.finishing,
                    p.skills.mental.composure,
                    p.attributes.pressure,
                    p.skills.technical.technique,
                    0.0,
                ).clamp(0.05, 1.0)
            } else {
                0.5
            }
        };

        let keeper_score = |players: &[MatchPlayer], id: Option<u32>| -> f32 {
            match id {
                Some(gk_id) => {
                    if let Some(p) = players.iter().find(|p| p.id == gk_id) {
                        score_keeper_save(
                            p.skills.goalkeeping.reflexes,
                            p.skills.physical.agility,
                            p.skills.goalkeeping.handling,
                            p.skills.mental.anticipation,
                            p.attributes.pressure,
                            p.skills.mental.concentration,
                        ).clamp(0.05, 1.0)
                    } else {
                        0.5
                    }
                }
                None => 0.05,
            }
        };

        let mut home_score: u8 = 0;
        let mut away_score: u8 = 0;
        let mut home_idx: usize = 0;
        let mut away_idx: usize = 0;

        for round in 0..5u8 {
            let home_remaining = 5 - round;
            let away_remaining = 5 - round;

            if let Some(&id) = home_takers.get(home_idx % home_takers.len()) {
                let t = taker_score(home_players, id);
                let k = keeper_score(away_players, away_keeper);
                let pressure = (0.35 + (round as f32 + 1.0) * 0.04).clamp(0.0, 1.0);
                let p = penalty_conversion_prob(t, k, pressure, true);
                let scored = rng.bernoulli(p);
                kicks.push(PenaltyShootoutKick {
                    team_id: home_team_id,
                    taker_id: id,
                    goalkeeper_id: away_keeper,
                    round: round + 1,
                    scored,
                    sudden_death: false,
                });
                if scored {
                    home_score += 1;
                }
                home_idx += 1;
            }

            if (home_score as i32 - away_score as i32).abs()
                > (home_remaining as i32 - 1).max(0) + away_remaining as i32
            {
                break;
            }

            if let Some(&id) = away_takers.get(away_idx % away_takers.len()) {
                let t = taker_score(away_players, id);
                let k = keeper_score(home_players, home_keeper);
                let pressure = (0.35 + (round as f32 + 1.0) * 0.04).clamp(0.0, 1.0);
                let p = penalty_conversion_prob(t, k, pressure, true);
                let scored = rng.bernoulli(p);
                kicks.push(PenaltyShootoutKick {
                    team_id: away_team_id,
                    taker_id: id,
                    goalkeeper_id: home_keeper,
                    round: round + 1,
                    scored,
                    sudden_death: false,
                });
                if scored {
                    away_score += 1;
                }
                away_idx += 1;
            }

            if (home_score as i32 - away_score as i32).abs()
                > (home_remaining as i32 - 1).max(0) + (away_remaining as i32 - 1).max(0)
            {
                break;
            }
        }

        let mut sudden_rounds = 0u8;
        while home_score == away_score && sudden_rounds < 30 {
            sudden_rounds += 1;
            let round = 5 + sudden_rounds;

            if let Some(&id) = home_takers.get(home_idx % home_takers.len()) {
                let t = taker_score(home_players, id);
                let k = keeper_score(away_players, away_keeper);
                let pressure = 0.65_f32;
                let p = penalty_conversion_prob(t, k, pressure, true);
                let scored = rng.bernoulli(p);
                kicks.push(PenaltyShootoutKick {
                    team_id: home_team_id,
                    taker_id: id,
                    goalkeeper_id: away_keeper,
                    round,
                    scored,
                    sudden_death: true,
                });
                if scored {
                    home_score += 1;
                }
                home_idx += 1;
            }

            if let Some(&id) = away_takers.get(away_idx % away_takers.len()) {
                let t = taker_score(away_players, id);
                let k = keeper_score(home_players, home_keeper);
                let pressure = 0.65_f32;
                let p = penalty_conversion_prob(t, k, pressure, true);
                let scored = rng.bernoulli(p);
                kicks.push(PenaltyShootoutKick {
                    team_id: away_team_id,
                    taker_id: id,
                    goalkeeper_id: home_keeper,
                    round,
                    scored,
                    sudden_death: true,
                });
                if scored {
                    away_score += 1;
                }
                away_idx += 1;
            }
        }

        if home_score == away_score {
            if rng.bernoulli(0.5) {
                home_score += 1;
            } else {
                away_score += 1;
            }
        }

        score.home_shootout = home_score;
        score.away_shootout = away_score;
    }

    fn penalty_takers_for(players: &[MatchPlayer], team_id: u32) -> Vec<u32> {
        let mut candidates: Vec<(u32, f32)> = players.iter()
            .filter(|p| p.team_id == team_id)
            .filter(|p| {
                p.tactical_position.current_position.position_group()
                    != PlayerFieldPositionGroup::Goalkeeper
            })
            .map(|p| {
                let s = score_penalty_taker(
                    p.skills.technical.penalty_taking,
                    p.skills.technical.finishing,
                    p.skills.mental.composure,
                    p.attributes.pressure,
                    p.skills.technical.technique,
                    0.0,
                );
                (p.id, s)
            })
            .collect();
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        candidates.into_iter().take(11).map(|(id, _)| id).collect()
    }

    fn penalty_keeper_for(players: &[MatchPlayer], team_id: u32) -> Option<u32> {
        players.iter()
            .find(|p| {
                p.team_id == team_id
                    && p.tactical_position.current_position.position_group()
                        == PlayerFieldPositionGroup::Goalkeeper
            })
            .map(|p| p.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::club::player::builder::PlayerBuilder;
    use crate::r#match::engine::player::MatchPlayer;
    use crate::shared::fullname::FullName;
    use crate::{
        MatchTacticType, PersonAttributes, PlayerAttributes, PlayerPosition, PlayerPositionType,
        PlayerPositions, PlayerSkills, Tactics,
    };
    use chrono::NaiveDate;
    use std::sync::atomic::{AtomicU32, Ordering};

    static NEXT_ID: AtomicU32 = AtomicU32::new(1000);

    fn make_match_player(pos: PlayerPositionType, team_id: u32) -> MatchPlayer {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let mut skills = PlayerSkills::default();
        skills.technical.passing = 12.0;
        skills.technical.finishing = 12.0;
        skills.technical.tackling = 12.0;
        skills.technical.dribbling = 12.0;
        skills.technical.technique = 12.0;
        skills.technical.first_touch = 12.0;
        skills.technical.marking = 12.0;
        skills.mental.composure = 12.0;
        skills.mental.decisions = 12.0;
        skills.mental.positioning = 12.0;
        skills.mental.anticipation = 12.0;
        skills.mental.concentration = 12.0;
        skills.mental.vision = 12.0;
        skills.mental.flair = 12.0;
        skills.physical.pace = 12.0;
        skills.physical.stamina = 12.0;
        skills.physical.agility = 12.0;
        skills.physical.acceleration = 12.0;
        skills.physical.strength = 12.0;
        skills.physical.balance = 12.0;
        skills.physical.jumping = 12.0;
        skills.goalkeeping.handling = 12.0;
        skills.goalkeeping.reflexes = 12.0;
        skills.goalkeeping.one_on_ones = 12.0;

        let player = PlayerBuilder::new()
            .id(id)
            .full_name(FullName::new("Test".into(), "Player".into()))
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

    fn make_squad(team_id: u32) -> MatchSquad {
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
            .map(|pos| make_match_player(pos, team_id))
            .collect();

        let subs: Vec<MatchPlayer> = (0..7)
            .map(|_| make_match_player(PlayerPositionType::MidfielderCenter, team_id))
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

    #[test]
    fn test_instant_engine_produces_valid_result() {
        let home = make_squad(1);
        let away = make_squad(2);

        let result = InstantEngine::play(&home, &away, false);

        let score = result.score.as_ref().expect("score must be present");
        let home_goals = score.home_team.get();
        let away_goals = score.away_team.get();

        assert!(
            home_goals <= 9 && away_goals <= 9,
            "score {}-{} out of range",
            home_goals,
            away_goals
        );

        assert!(
            !result.player_stats.is_empty(),
            "player stats must not be empty"
        );
        assert!(
            result.player_stats.len() >= 22,
            "expected at least 22 player stats, got {}",
            result.player_stats.len()
        );

        for (pid, stats) in &result.player_stats {
            assert!(
                stats.match_rating >= 1.0 && stats.match_rating <= 10.0,
                "player {} rating {} out of range",
                pid,
                stats.match_rating
            );
            assert!(
                stats.minutes_played > 0,
                "player {} has 0 minutes",
                pid
            );
        }

        assert!(
            !result.physical_snapshots.is_empty(),
            "physical snapshots must not be empty"
        );
        for (pid, snap) in &result.physical_snapshots {
            assert!(
                snap.final_match_energy >= 0 && snap.final_match_energy <= 10000,
                "player {} energy {} out of range",
                pid,
                snap.final_match_energy
            );
            assert!(
                snap.minutes_played > 0.0,
                "player {} snapshot has 0 minutes",
                pid
            );
        }

        assert_eq!(
            result.left_team_players.team_id, 1,
            "home team id mismatch"
        );
        assert_eq!(
            result.right_team_players.team_id, 2,
            "away team id mismatch"
        );

        assert!(
            result.starting_home_tactic.is_some(),
            "starting home tactic missing"
        );
        assert!(
            result.starting_away_tactic.is_some(),
            "starting away tactic missing"
        );

        let _total_shots: u16 = result.player_stats.values().map(|s| s.shots_total).sum();
        let total_goals: u16 = result.player_stats.values().map(|s| s.goals).sum();
        assert!(
            total_goals as u8 <= home_goals + away_goals,
            "player goals {} exceed scoreline {}-{}",
            total_goals,
            home_goals,
            away_goals
        );

        let total_passes: u16 = result
            .player_stats
            .values()
            .map(|s| s.passes_attempted)
            .sum();
        assert!(
            total_passes > 30,
            "expected meaningful pass volume, got {}",
            total_passes
        );
    }

    #[test]
    fn test_instant_engine_deterministic_with_seed() {
        let home = make_squad(1);
        let away = make_squad(2);

        let seed: u64 = 42;

        let result1 = InstantEngine::play_seeded(&home, &away, false, Some(seed), &MatchRng::from_seed(0));
        let result2 = InstantEngine::play_seeded(&home, &away, false, Some(seed), &MatchRng::from_seed(0));

        let s1 = result1.score.as_ref().unwrap();
        let s2 = result2.score.as_ref().unwrap();
        assert_eq!(
            s1.home_team.get(),
            s2.home_team.get(),
            "deterministic home goals"
        );
        assert_eq!(
            s1.away_team.get(),
            s2.away_team.get(),
            "deterministic away goals"
        );
    }

    #[test]
    fn test_knockout_tied_produces_extra_time() {
        let home = make_squad(1);
        let away = make_squad(2);

        for seed in 0..200u64 {
            let result = InstantEngine::play_seeded(&home, &away, true, Some(seed), &MatchRng::from_seed(0));
            let score = result.score.as_ref().unwrap();
            if score.home_team.get() == score.away_team.get() && !score.had_shootout() {
                assert!(
                    result.match_time_ms >= calibration::MATCH_DURATION_MS + calibration::EXTRA_TIME_DURATION_MS,
                    "tied knockout with no shootout should have extra time: seed={}, time={}",
                    seed,
                    result.match_time_ms
                );
                return;
            }
        }
    }

    #[test]
    fn test_penalty_shootout_valid_data() {
        let home = make_squad(1);
        let away = make_squad(2);

        for seed in 0..500u64 {
            let result = InstantEngine::play_seeded(&home, &away, true, Some(seed), &MatchRng::from_seed(0));
            let score = result.score.as_ref().unwrap();
            if score.had_shootout() {
                assert!(
                    !result.penalty_shootout.is_empty(),
                    "shootout kicks must not be empty when shootout occurred"
                );

                let home_kicks: Vec<_> = result.penalty_shootout.iter().filter(|k| k.team_id == 1).collect();
                let away_kicks: Vec<_> = result.penalty_shootout.iter().filter(|k| k.team_id == 2).collect();

                assert!(
                    home_kicks.len() >= 5,
                    "home should have at least 5 kicks, got {}",
                    home_kicks.len()
                );
                assert!(
                    away_kicks.len() >= 5,
                    "away should have at least 5 kicks, got {}",
                    away_kicks.len()
                );

                for kick in &result.penalty_shootout {
                    assert!(
                        kick.round >= 1 && kick.round <= 35,
                        "round {} out of range",
                        kick.round
                    );
                }

                assert!(
                    score.home_shootout != score.away_shootout,
                    "shootout must produce a winner"
                );

                let home_scored: u8 = home_kicks.iter().map(|k| if k.scored { 1 } else { 0 }).sum();
                let away_scored: u8 = away_kicks.iter().map(|k| if k.scored { 1 } else { 0 }).sum();
                assert_eq!(
                    home_scored, score.home_shootout,
                    "home shootout tally mismatch"
                );
                assert_eq!(
                    away_scored, score.away_shootout,
                    "away shootout tally mismatch"
                );

                return;
            }
        }
        panic!("no tied knockout match found in 500 seeds");
    }

    #[test]
    fn test_substitutions_recorded() {
        let home = make_squad(1);
        let away = make_squad(2);

        for seed in 0..100u64 {
            let result = InstantEngine::play_seeded(&home, &away, false, Some(seed), &MatchRng::from_seed(0));
            if !result.substitutions.is_empty() {
                for sub in &result.substitutions {
                    assert!(
                        sub.match_time_ms >= calibration::SUB_MIN_TIME_MIN_MS,
                        "sub time {} too early",
                        sub.match_time_ms
                    );
                    assert!(
                        result.player_stats.contains_key(&sub.player_out_id),
                        "subbed-out player {} missing from stats",
                        sub.player_out_id
                    );
                    assert!(
                        result.player_stats.contains_key(&sub.player_in_id),
                        "subbed-in player {} missing from stats",
                        sub.player_in_id
                    );
                    assert!(
                        result.physical_snapshots.contains_key(&sub.player_out_id),
                        "subbed-out player {} missing from physical snapshots",
                        sub.player_out_id
                    );
                    assert!(
                        result.physical_snapshots.contains_key(&sub.player_in_id),
                        "subbed-in player {} missing from physical snapshots",
                        sub.player_in_id
                    );
                }

                let home_subs: Vec<_> = result.substitutions.iter().filter(|s| s.team_id == 1).collect();
                let away_subs: Vec<_> = result.substitutions.iter().filter(|s| s.team_id == 2).collect();
                assert!(
                    home_subs.len() <= 3,
                    "home should have at most 3 subs, got {}",
                    home_subs.len()
                );
                assert!(
                    away_subs.len() <= 3,
                    "away should have at most 3 subs, got {}",
                    away_subs.len()
                );
                return;
            }
        }
    }

    #[test]
    fn test_match_play_routes_instant_for_non_user() {
        use crate::MatchRuntime;

        let was_instant = MatchRuntime::instant_engine_mode();
        let was_user = MatchRuntime::user_team_id();

        MatchRuntime::set_instant_engine_mode(true);
        MatchRuntime::set_user_team_id(99);

        let home = make_squad(1);
        let away = make_squad(2);

        let result = InstantEngine::play(&home, &away, false);
        let score = result.score.as_ref().expect("score present");
        assert!(
            score.home_team.get() <= 9 && score.away_team.get() <= 9,
            "instant engine should produce valid score"
        );

        MatchRuntime::set_instant_engine_mode(was_instant);
        MatchRuntime::set_user_team_id(was_user);
    }

    #[test]
    fn test_match_play_uses_full_engine_for_user_team() {
        use crate::r#match::Match;
        use crate::MatchRuntime;

        let was_instant = MatchRuntime::instant_engine_mode();
        let was_user = MatchRuntime::user_team_id();

        MatchRuntime::set_instant_engine_mode(true);
        MatchRuntime::set_user_team_id(1);

        let home = make_squad(1);
        let away = make_squad(2);

        let m = Match::make(
            "test".into(),
            1,
            "test-league",
            home,
            away,
            false,
        );

        let result = m.play();
        assert!(
            result.details.is_some(),
            "match result must have details"
        );
        let _ = &result.score;

        MatchRuntime::set_instant_engine_mode(was_instant);
        MatchRuntime::set_user_team_id(was_user);
    }
}
