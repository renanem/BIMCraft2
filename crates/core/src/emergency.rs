//! Emergency lighting as IT 18/2025 of the CBPMESP (Iluminação de emergência) asks, with the parts of ABNT NBR 10898 it adopts
//! (autonomy, the 20:1 ratio of illuminances and the distances of Tabela A.1). Pure functions of their input: the `emergency_light`
//! command is their caller, so none of this needs a luminaire in the model.
//!
//! - IT 18: at least 3 lux on level ground and 5 lux on stairs and where the floor has obstacles (5.2); clearing points at most 15 m
//!   apart and 7.5 m from a wall, other spacings where NBR 10898's parameters are met (4.4.1.1); a room of 50 m² or less and under 50
//!   people, its exits straight to the corridor, is exempt (4.4.1.2); at most 30 V in areas with fire load, or for an existing
//!   installation a 30 mA residual-current switch with a 10 A breaker (5.3 and 5.4); beacons over the exit doors of a closed room of
//!   more than 100 people in F-3, F-5, F-6, F-7, F-10 and F-11, lit at all times (4.4.2.2).
//! - NBR 10898 (as read from the pages sent): autonomy of 2 h at least; the ratio of the greatest to the least illuminance on an escape
//!   route under 20:1; Tabela A.1, the farthest a luminaire of a given flux may stand from the wall at the end of a 2 m corridor and
//!   from the next, by its mounting height (a corridor of 2 m, light walls, nothing in the way).
//! - Not covered: the switchover time of a generator (a limit of NBR 10898 not read here), the voltage limits of NBR 10898 and the
//!   photometric study itself.

use crate::checks::{Check, Kind, Status, verdict};

const NBR: &str = "ABNT NBR 10898, as read from its text";

/// The least autonomy, hours (NBR 10898).
pub const MIN_AUTONOMY_H: f64 = 2.0;
/// The greatest ratio of the highest to the lowest illuminance on an escape route (NBR 10898).
pub const MAX_RATIO: f64 = 20.0;
/// The farthest clearing points may stand apart, m (IT 18 4.4.1.1).
pub const MAX_SPACING: f64 = 15.0;
/// The farthest a point may stand from a wall, m.
pub const MAX_WALL: f64 = 7.5;
/// The most the voltage of a luminaire may be in an area with fire load, V (IT 18 5.3).
pub const MAX_VOLTS: f64 = 30.0;
/// The occupations whose exit luminaires are beacons, lit at all times, in a closed room of more than 100 people.
pub const BEACON_OCCUPATIONS: &[&str] = &["F-3", "F-5", "F-6", "F-7", "F-10", "F-11"];

/// NBR 10898's Tabela A.1: (flux lm, mounting height m (at most), ceiling mounting allowed, farthest from the wall at the end of the
/// corridor m, farthest between points m).
pub const TABELA_A1: &[(f64, f64, bool, f64, f64)] = &[
    (300.0, 1.8, false, 3.6, 7.2),
    (300.0, 2.2, true, 4.4, 8.8),
    (400.0, 1.8, false, 3.6, 7.2),
    (400.0, 2.2, true, 4.4, 8.8),
    (400.0, 2.5, true, 5.0, 10.0),
    (500.0, 1.8, false, 3.6, 7.2),
    (500.0, 2.2, true, 4.4, 8.8),
    (500.0, 2.5, true, 5.0, 10.0),
    (500.0, 2.8, true, 5.6, 11.2),
    (500.0, 3.0, true, 6.0, 12.0),
    (750.0, 3.5, true, 7.0, 14.0),
    (750.0, 3.75, true, 7.5, 15.0),
    (750.0, 5.0, true, 7.5, 15.0),
    (1000.0, 3.75, true, 7.5, 15.0),
    (1000.0, 5.0, true, 7.5, 15.0),
    (1000.0, 7.0, true, 7.5, 15.0),
    (1500.0, 8.0, true, 7.5, 15.0),
    (1500.0, 9.0, true, 7.5, 15.0),
    (1500.0, 10.0, true, 7.5, 15.0),
    (2000.0, 11.0, true, 7.5, 15.0),
    (2000.0, 12.0, true, 7.5, 15.0),
];

/// The least illuminance on the floor, lux: 3 on level ground, 5 where it changes level or has obstacles.
pub fn min_lux(level_change: bool) -> f64 {
    if level_change { 5.0 } else { 3.0 }
}

/// The (end distance, spacing) Tabela A.1 gives for a luminaire of `lumens` mounted `height` m up (on the ceiling or on a wall), or
/// `None` where the table has no row: the flux is not one it lists or the height is above its highest.
pub fn table_a1(lumens: f64, height: f64, ceiling: bool) -> Option<(f64, f64)> {
    TABELA_A1
        .iter()
        .filter(|r| (r.0 - lumens).abs() < 1e-9 && r.1 + 1e-9 >= height && (!ceiling || r.2))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|r| (r.3, r.4))
}

/// Points along a corridor of `length` m, 15 m apart and 7.5 m from each end at most.
pub fn points_along(length: f64) -> u32 {
    if length <= 0.0 { 0 } else { (length / MAX_SPACING - 1e-9).ceil().max(1.0) as u32 }
}

#[derive(Clone, Debug)]
pub struct Input {
    pub level_change: bool,
    pub lux: Option<f64>,
    /// The highest illuminance measured or calculated on the route, lux.
    pub lux_max: Option<f64>,
    pub spacing: Option<f64>,
    pub wall: Option<f64>,
    pub area: Option<f64>,
    pub population: Option<f64>,
    pub direct_exit: bool,
    pub fire_load: bool,
    pub voltage: Option<f64>,
    pub existing: bool,
    pub rcd: bool,
    pub occupancy: Option<String>,
    pub capacity: Option<f64>,
    pub enclosed: bool,
    pub generator: bool,
    pub switch_time: Option<f64>,
    pub autonomy_h: Option<f64>,
    pub lumens: Option<f64>,
    pub height: Option<f64>,
    pub ceiling: bool,
    pub length: Option<f64>,
    pub room: Option<(f64, f64)>,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub min_lux: f64,
    pub exempt: bool,
    /// Points the 15 m rule takes for the corridor or the room given.
    pub points: Option<u32>,
    /// What Tabela A.1 gives for the flux and height given.
    pub table: Option<(f64, f64)>,
    pub checks: Vec<Check>,
}

fn check(id: &'static str, status: Status, text: impl Into<String>, source: &'static str) -> Check {
    Check { id, kind: Kind::Requirement, status, text: text.into(), source }
}

/// Checks the emergency lighting of `i`.
pub fn check_all(i: &Input) -> Outcome {
    let min = min_lux(i.level_change);
    let exempt = matches!((i.area, i.population), (Some(a), Some(p)) if a <= 50.0 && p < 50.0) && i.direct_exit;
    let mut checks = Vec::new();
    if exempt {
        checks.push(check(
            "exemption",
            Status::Ok,
            "a room of 50 m² or less and under 50 people, its exits straight to the corridor, needs no emergency lighting",
            "IT 18/2025 (CBPMESP), item 4.4.1.2",
        ));
    }
    checks.push(match i.lux {
        Some(l) => check(
            "lux",
            verdict(l >= min - 1e-9),
            format!(
                "{l} lux on the floor; {min} at least ({})",
                if i.level_change { "stairs or where the floor has obstacles" } else { "level ground" }
            ),
            "IT 18/2025 (CBPMESP), item 5.2",
        ),
        None => {
            check("lux", Status::Pending, format!("`lux` not given: {min} lux at least on the floor"), "IT 18/2025 (CBPMESP), item 5.2")
        }
    });
    checks.push(match i.lux_max {
        Some(hi) => match i.lux {
            Some(lo) if lo > 0.0 => check(
                "ratio",
                verdict(hi / lo < MAX_RATIO),
                format!("the highest illuminance over the least is {:.1}:1; under {MAX_RATIO}:1 on an escape route", hi / lo),
                NBR,
            ),
            _ => check("ratio", Status::Pending, "`lux` (the least) is needed with `lux_max` for the ratio", NBR),
        },
        None => check(
            "ratio",
            Status::Pending,
            format!("`lux_max` not given: the ratio of the highest to the least illuminance is under {MAX_RATIO}:1"),
            NBR,
        ),
    });
    checks.push(match i.autonomy_h {
        Some(h) => check("autonomy", verdict(h >= MIN_AUTONOMY_H - 1e-9), format!("{h} h of autonomy; {MIN_AUTONOMY_H} h at least"), NBR),
        None => check("autonomy", Status::Pending, format!("`autonomy` (h) not given: {MIN_AUTONOMY_H} h at least"), NBR),
    });
    checks.push(match (i.spacing, i.wall) {
        (None, None) => check(
            "spacing",
            Status::Pending,
            format!("`spacing` and `wall` not given: clearing points {MAX_SPACING} m apart at most and {MAX_WALL} m from a wall"),
            "IT 18/2025 (CBPMESP), item 4.4.1.1",
        ),
        (s, w) => {
            let over_s = s.filter(|s| *s > MAX_SPACING + 1e-9);
            let over_w = w.filter(|w| *w > MAX_WALL + 1e-9);
            check(
                "spacing",
                verdict(over_s.is_none() && over_w.is_none()),
                format!(
                    "points {} m apart and {} m from a wall; {MAX_SPACING} and {MAX_WALL} m at most (another spacing may be adopted where the parameters of NBR 10898 are met)",
                    s.map_or("?".into(), |v| v.to_string()),
                    w.map_or("?".into(), |v| v.to_string())
                ),
                "IT 18/2025 (CBPMESP), item 4.4.1.1",
            )
        }
    });
    let table = match (i.lumens, i.height) {
        (Some(l), Some(h)) => table_a1(l, h, i.ceiling),
        _ => None,
    };
    if let (Some(l), Some(h)) = (i.lumens, i.height) {
        checks.push(match table {
            Some((end, gap)) => {
                let ok = i.spacing.is_none_or(|s| s <= gap + 1e-9) && i.wall.is_none_or(|w| w <= end + 1e-9);
                check(
                    "table_a1",
                    verdict(ok),
                    format!("a {l} lm luminaire {h} m up: {end} m from the wall at the end of a corridor and {gap} m between points at most (a corridor of 2 m, light walls, nothing in the way)"),
                    "ABNT NBR 10898, Tabela A.1, as read from its text",
                )
            }
            None => check(
                "table_a1",
                Status::Pending,
                format!("Tabela A.1 has no row for {l} lm at {h} m{}: a luminotechnical study decides", if i.ceiling { " on the ceiling" } else { "" }),
                "ABNT NBR 10898, Tabela A.1, as read from its text",
            ),
        });
    }
    if i.fire_load {
        checks.push(match i.voltage {
            Some(v) if v <= MAX_VOLTS + 1e-9 => check("voltage", Status::Ok, format!("{v} V; {MAX_VOLTS} V at most in an area with fire load"), "IT 18/2025 (CBPMESP), item 5.3"),
            Some(v) if i.existing && i.rcd => check(
                "voltage",
                Status::Ok,
                format!("{v} V in an existing installation that cannot lower it, with a 30 mA residual-current switch and a 10 A breaker as NBR 5410 asks"),
                "IT 18/2025 (CBPMESP), item 5.4",
            ),
            Some(v) => check(
                "voltage",
                Status::Fail,
                format!("{v} V; {MAX_VOLTS} V at most in an area with fire load (an existing installation that cannot lower it may use a 30 mA residual-current switch with a 10 A breaker)"),
                "IT 18/2025 (CBPMESP), items 5.3 and 5.4",
            ),
            None => check("voltage", Status::Pending, format!("`voltage` (V) not given: {MAX_VOLTS} V at most in an area with fire load"), "IT 18/2025 (CBPMESP), item 5.3"),
        });
    }
    if let (Some(o), Some(n)) = (&i.occupancy, i.capacity)
        && n > 100.0
        && i.enclosed
        && BEACON_OCCUPATIONS.contains(&o.as_str())
    {
        checks.push(check(
            "exit_beacons",
            Status::Pending,
            format!("the luminaires over the exit doors of this closed room of {n} people ({o}) are beacons, lit permanently while the room is in use, normal and emergency"),
            "IT 18/2025 (CBPMESP), item 4.4.2.2",
        ));
    }
    if i.generator {
        checks.push(check(
            "switchover",
            Status::Pending,
            match i.switch_time {
                Some(t) => format!("a generator switching in {t} s: where it is longer than NBR 10898 allows (a limit not read here) a central battery or autonomous blocks are needed"),
                None => "a generator with `switch_time` (s) not given: where it is longer than NBR 10898 allows (a limit not read here) a central battery or autonomous blocks are needed".into(),
            },
            "IT 18/2025 (CBPMESP), item 4.1.6",
        ));
    }
    let points = match (i.length, i.room) {
        (Some(l), _) => Some(points_along(l)),
        (None, Some((w, d))) => Some(points_along(w) * points_along(d)),
        _ => None,
    };
    Outcome { min_lux: min, exempt, points, table, checks }
}

#[cfg(test)]
mod tests;
