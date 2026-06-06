pub const DRIVES_PER_MATCH_MIN: u32 = 150;
pub const DRIVES_PER_MATCH_MAX: u32 = 250;

pub const DRIVES_PER_ET_MIN: u32 = 50;
pub const DRIVES_PER_ET_MAX: u32 = 80;

pub const ACTIONS_PER_DRIVE_MIN: u32 = 4;
pub const ACTIONS_PER_DRIVE_MAX: u32 = 8;

pub const DRIVE_TIME_MIN_MS: u64 = 30_000;
pub const DRIVE_TIME_MAX_MS: u64 = 180_000;
pub const MATCH_DURATION_MS: u64 = 90 * 60 * 1000;
pub const EXTRA_TIME_DURATION_MS: u64 = 30 * 60 * 1000;

pub const BASE_SHOT_XG: f32 = 0.08;
pub const CLOSE_SHOT_XG: f32 = 0.25;

pub const SHOT_FREQUENCY: f32 = 0.18;

pub const PASS_SUCCESS_BASE: f32 = 0.82;
pub const DRIBBLE_SUCCESS_BASE: f32 = 0.55;
pub const TACKLE_SUCCESS_BASE: f32 = 0.50;

pub const CARD_RATE: f32 = 0.12;
pub const RED_CARD_RATE: f32 = 0.02;

pub const FOUL_RATE: f32 = 0.15;

pub const HOME_ADVANTAGE: f32 = 0.03;

pub const CONDITION_DRAIN_PER_MINUTE: f32 = 50.0;
pub const CONDITION_FLOOR: i16 = 1500;

pub const HIGH_INTENSITY_DEFAULT: f32 = 0.15;

pub const SUBS_PER_TEAM_MIN: u32 = 0;
pub const SUBS_PER_TEAM_MAX: u32 = 3;
pub const SUB_MIN_TIME_MIN_MS: u64 = 55 * 60 * 1000;
pub const SUB_MIN_TIME_MAX_MS: u64 = 80 * 60 * 1000;

pub const INJURY_CHANCE_PER_DRIVE: f32 = 0.003;
pub const INJURY_CONDITION_DRAIN: i16 = 5000;
pub const INJURY_CRITICAL_CONDITION: i16 = 2000;
