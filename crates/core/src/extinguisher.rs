//! Portable and wheeled fire extinguishers as IT 21/2025 of the CBPMESP (Sistema de proteção por extintores de incêndio) asks. Pure
//! functions of their input: the `extinguisher_size` command is their caller, so none of this needs an extinguisher in the model.
//!
//! - The standard fixes no number of units for an area: it fixes how far one may be from any point (Tabela 1, by the risk: 25, 20
//!   and 15 m, half as far again for a wheeled unit), how many a floor has at least (two: one for class A and one for B and C, or
//!   two ABC powder; a single ABC powder under 50 m²; two in each subdivision with no passage), the least capacity of a unit of each
//!   agent (items 4.1.1 and 4.1.2), and the risks that are protected whatever the general protection (4.2.1.12). That is what it
//!   checks, and the risk class itself (low, medium, high) is given, since the standard names it and the occupation's fire load
//!   (IT 14) decides it.
//! - Which unit stands where, the mounting (a support 1.6 m high at most, its foot 0.10 m up at least, never on a stair) and the
//!   signs (IT 20) are the project's and are not checked here.

use crate::checks::{Check, Kind, Status, verdict};

/// The risk the walking distance of Tabela 1 is read by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Risk {
    Low,
    Medium,
    High,
}

impl Risk {
    pub fn parse(s: &str) -> Option<Risk> {
        match s {
            "low" => Some(Risk::Low),
            "medium" => Some(Risk::Medium),
            "high" => Some(Risk::High),
            _ => None,
        }
    }
}

/// The agents the standard gives a least capacity for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agent {
    Water,
    Foam,
    Co2,
    PowderBc,
    PowderAbc,
    Halogenated,
}

/// Every agent, with the name the command replies by.
pub const AGENTS: &[(&str, Agent)] = &[
    ("water", Agent::Water),
    ("foam", Agent::Foam),
    ("co2", Agent::Co2),
    ("powder_bc", Agent::PowderBc),
    ("powder_abc", Agent::PowderAbc),
    ("halogenated", Agent::Halogenated),
];

/// The risks of item 4.2.1.12 that are protected whatever the general protection: (key, what it is).
pub const SPECIFIC: &[(&str, &str)] = &[
    ("boiler", "boiler room"),
    ("pump", "pump room"),
    ("power", "electrical power house"),
    ("machine", "machine room"),
    ("transmission", "transmission gallery"),
    ("incinerator", "incinerator"),
    ("lift", "elevator machine room"),
    ("escalator", "escalator machine room"),
    ("panel", "electrical panel"),
    ("transformer", "transformer"),
    ("telephony", "telephony container"),
    ("flammables", "area storing or handling flammable or combustible gases or liquids"),
    ("pyrophoric", "place with pyrophoric metals"),
    ("kitchen", "professional kitchen"),
];

/// The key of the specific risk named `s`, or `None`.
pub fn parse_specific(s: &str) -> Option<&'static str> {
    SPECIFIC.iter().find(|r| r.0 == s).map(|r| r.0)
}

/// The most an operator may walk to a portable extinguisher (Tabela 1), m; half as far again to a wheeled one (item 4.1.5).
pub fn max_walk(risk: Risk, wheeled: bool) -> f64 {
    let portable = match risk {
        Risk::Low => 25.0,
        Risk::Medium => 20.0,
        Risk::High => 15.0,
    };
    if wheeled { portable * 1.5 } else { portable }
}

/// The least extinguishing capacity of a unit of `agent` (items 4.1.1 and 4.1.2), or `None` where the standard lists no wheeled
/// unit of it (halogenated).
pub fn capacity(agent: Agent, wheeled: bool) -> Option<&'static str> {
    Some(match (agent, wheeled) {
        (Agent::Water, false) => "2-A",
        (Agent::Foam, false) => "2-A : 10-B",
        (Agent::Co2, false) => "5-B:C",
        (Agent::PowderBc, false) => "20-B:C",
        (Agent::PowderAbc, false) => "2-A : 20-B:C",
        (Agent::Halogenated, false) => "5-B:C",
        (Agent::Water, true) => "10-A",
        (Agent::Foam, true) => "6-A : 40-B",
        (Agent::Co2, true) => "10-B:C",
        (Agent::PowderBc, true) => "80-B:C",
        (Agent::PowderAbc, true) => "6-A : 80-B:C",
        (Agent::Halogenated, true) => return None,
    })
}

/// The units a floor, or one of its subdivisions, has, by class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Units {
    /// Water or foam: class A.
    pub a: u32,
    /// Class B and C.
    pub bc: u32,
    /// ABC powder, which stands for any class.
    pub abc: u32,
}

#[derive(Clone, Debug)]
pub struct Input {
    pub risk: Risk,
    pub wheeled: bool,
    /// The floor's built area, m².
    pub area: Option<f64>,
    /// The parts of the floor with no passage between them.
    pub subdivisions: u32,
    pub units: Option<Units>,
    /// The farthest an operator walks to the nearest unit, m.
    pub walk: Option<f64>,
    /// The distance of the nearest unit to the main entrance, m.
    pub entrance: Option<f64>,
    /// The keys of the `SPECIFIC` risks the building has.
    pub specific: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct Outcome {
    pub max_walk_m: f64,
    /// The fewest units the floor has (in all its subdivisions).
    pub min_units: u32,
    pub checks: Vec<Check>,
}

fn check(id: &'static str, status: Status, text: impl Into<String>, source: &'static str) -> Check {
    Check { id, kind: Kind::Requirement, status, text: text.into(), source }
}

/// Checks the protection of `i`: the walking distance, the units of a floor and the risks that stand on their own.
pub fn size(i: &Input) -> Outcome {
    let max_walk_m = max_walk(i.risk, i.wheeled);
    let single = i.subdivisions == 1 && i.area.is_some_and(|a| a < 50.0);
    let min_units = if single { 1 } else { 2 * i.subdivisions };
    let mut checks = vec![match i.walk {
        Some(w) => check(
            "walk",
            verdict(w <= max_walk_m + 1e-9),
            format!(
                "the farthest walk is {w} m; {max_walk_m} m at most for a {} unit of this risk",
                if i.wheeled { "wheeled" } else { "portable" }
            ),
            "IT 21/2025 (CBPMESP), items 4.1.4 and 4.1.5, Tabela 1",
        ),
        None => check(
            "walk",
            Status::Pending,
            format!("`walk` (m) not given: no point may be farther than {max_walk_m} m from a unit"),
            "IT 21/2025 (CBPMESP), items 4.1.4 and 4.1.5, Tabela 1",
        ),
    }];
    checks.push(match i.units {
        Some(u) => {
            let total = u.a + u.bc + u.abc;
            let ok = if single {
                u.abc >= 1
            } else {
                total >= min_units && u.a + u.abc >= 1 && u.bc + u.abc >= 1
            };
            check(
                "units",
                verdict(ok),
                if single {
                    format!("a floor under 50 m² may have a single unit, of ABC powder: it has {} of ABC powder", u.abc)
                } else {
                    format!(
                        "{total} unit(s) for {} space(s), {min_units} at least: one for class A and one for B and C, or two of ABC powder (a: {}, b and c: {}, abc: {})",
                        i.subdivisions, u.a, u.bc, u.abc
                    )
                },
                "IT 21/2025 (CBPMESP), items 4.2.1.5, 4.2.1.5.1, 4.2.1.6 and 4.2.1.7",
            )
        }
        None => check(
            "units",
            Status::Pending,
            format!("`units` not given: {min_units} unit(s) at least, {}", if single { "of ABC powder" } else { "one for class A and one for B and C, or two of ABC powder" }),
            "IT 21/2025 (CBPMESP), items 4.2.1.5, 4.2.1.5.1, 4.2.1.6 and 4.2.1.7",
        ),
    });
    if let Some(d) = i.entrance {
        checks.push(check(
            "entrance",
            verdict(d <= 5.0 + 1e-9),
            format!("the nearest unit is {d} m from the main entrance; 5 m at most (and from the stairs on the other floors)"),
            "IT 21/2025 (CBPMESP), item 4.2.1.12.3",
        ));
    }
    if i.wheeled {
        checks.push(check(
            "wheeled",
            Status::Pending,
            "wheeled units are never the only protection: they take half the risk's area at most, the rest by portable units, alternating; they must stand where access is free, protect only their own floor, and are obligatory in a high-risk building handling explosives or flammable liquids, unless its tanks are buried",
            "IT 21/2025 (CBPMESP), items 4.2.2.1 to 4.2.2.4",
        ));
    }
    let named: Vec<&str> = SPECIFIC.iter().filter(|r| i.specific.iter().any(|s| s == r.0)).map(|r| r.1).collect();
    if !named.is_empty() {
        checks.push(check(
            "specific",
            Status::Pending,
            format!("protected by extinguishers whatever the general protection: {}", named.join(", ")),
            "IT 21/2025 (CBPMESP), item 4.2.1.12",
        ));
    }
    if i.specific.iter().any(|s| s == "kitchen") {
        checks.push(check(
            "kitchen",
            Status::Pending,
            "a professional kitchen has extinguishers whose agent reacts by saponification, to cool the fat or the edible vegetable oil",
            "IT 21/2025 (CBPMESP), item 4.1.6",
        ));
    }
    Outcome { max_walk_m, min_units, checks }
}

#[cfg(test)]
mod tests;
