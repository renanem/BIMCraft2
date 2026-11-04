//! Portable fire extinguishers as IT 21/2025 of the CBPMESP asks: the table of walking distances, the least capacity of each unit,
//! the units a floor has, and the checks of each kind.

use super::*;
use crate::checks::Status;

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9
}

fn input(risk: Risk) -> Input {
    Input { risk, wheeled: false, area: None, subdivisions: 1, units: None, walk: None, entrance: None, specific: vec![] }
}

fn status(o: &Outcome, id: &str) -> Status {
    o.checks.iter().find(|c| c.id == id).map(|c| c.status).unwrap_or_else(|| panic!("no check {id}"))
}

#[test]
fn tabela_1_is_as_printed() {
    assert!(near(max_walk(Risk::Low, false), 25.0));
    assert!(near(max_walk(Risk::Medium, false), 20.0));
    assert!(near(max_walk(Risk::High, false), 15.0));
}

#[test]
fn a_wheeled_unit_walks_half_as_far_again() {
    assert!(near(max_walk(Risk::Low, true), 37.5));
    assert!(near(max_walk(Risk::Medium, true), 30.0));
    assert!(near(max_walk(Risk::High, true), 22.5));
}

#[test]
fn the_least_capacity_of_each_agent_is_as_printed() {
    let p = |a| capacity(a, false);
    assert_eq!(p(Agent::Water), Some("2-A"));
    assert_eq!(p(Agent::Foam), Some("2-A : 10-B"));
    assert_eq!(p(Agent::Co2), Some("5-B:C"));
    assert_eq!(p(Agent::PowderBc), Some("20-B:C"));
    assert_eq!(p(Agent::PowderAbc), Some("2-A : 20-B:C"));
    assert_eq!(p(Agent::Halogenated), Some("5-B:C"));
    let w = |a| capacity(a, true);
    assert_eq!(w(Agent::Water), Some("10-A"));
    assert_eq!(w(Agent::Foam), Some("6-A : 40-B"));
    assert_eq!(w(Agent::Co2), Some("10-B:C"));
    assert_eq!(w(Agent::PowderBc), Some("80-B:C"));
    assert_eq!(w(Agent::PowderAbc), Some("6-A : 80-B:C"));
    assert_eq!(w(Agent::Halogenated), None, "the standard lists no wheeled halogenated unit");
}

#[test]
fn the_nearest_unit_is_within_5_m_of_the_entrance() {
    assert!(size(&input(Risk::Low)).checks.iter().all(|c| c.id != "entrance"));
    assert_eq!(status(&size(&Input { entrance: Some(5.0), ..input(Risk::Low) }), "entrance"), Status::Ok);
    assert_eq!(status(&size(&Input { entrance: Some(5.1), ..input(Risk::Low) }), "entrance"), Status::Fail);
}

#[test]
fn a_floor_has_two_units_and_a_small_one_a_single_abc() {
    let o = size(&input(Risk::Low));
    assert_eq!(o.min_units, 2);
    assert_eq!(size(&Input { area: Some(49.9), ..input(Risk::Low) }).min_units, 1, "a single ABC powder unit under 50 m²");
    assert_eq!(size(&Input { area: Some(50.0), ..input(Risk::Low) }).min_units, 2);
    assert_eq!(size(&Input { subdivisions: 3, ..input(Risk::Low) }).min_units, 6, "each subdivision has its two");
    assert_eq!(size(&Input { area: Some(30.0), subdivisions: 2, ..input(Risk::Low) }).min_units, 4, "the single unit is for one space");
}

#[test]
fn the_walk_is_checked_against_the_table() {
    assert_eq!(status(&size(&input(Risk::Medium)), "walk"), Status::Pending);
    assert_eq!(status(&size(&Input { walk: Some(20.0), ..input(Risk::Medium) }), "walk"), Status::Ok);
    assert_eq!(status(&size(&Input { walk: Some(20.1), ..input(Risk::Medium) }), "walk"), Status::Fail);
    assert_eq!(status(&size(&Input { walk: Some(30.0), wheeled: true, ..input(Risk::Medium) }), "walk"), Status::Ok);
}

#[test]
fn the_units_given_are_counted_by_class() {
    let u = |a, bc, abc| Some(Units { a, bc, abc });
    let c = |units| size(&Input { units, ..input(Risk::Low) });
    assert_eq!(status(&c(None), "units"), Status::Pending);
    assert_eq!(status(&c(u(1, 1, 0)), "units"), Status::Ok, "one for class A and one for B and C");
    assert_eq!(status(&c(u(0, 0, 2)), "units"), Status::Ok, "two ABC powder units");
    assert_eq!(status(&c(u(0, 0, 1)), "units"), Status::Fail);
    assert_eq!(status(&c(u(2, 0, 0)), "units"), Status::Fail, "two for class A leave B and C bare");
    assert_eq!(status(&c(u(1, 0, 1)), "units"), Status::Ok, "an ABC powder stands for any class");
}

#[test]
fn the_specific_risks_are_listed_and_a_kitchen_needs_saponifying_agents() {
    let o = size(&Input { specific: vec!["kitchen".into(), "transformer".into()], ..input(Risk::Low) });
    assert!(o.checks.iter().any(|c| c.id == "specific" && c.text.contains("kitchen") && c.text.contains("transformer")), "{:?}", o.checks);
    assert!(o.checks.iter().any(|c| c.id == "kitchen" && c.text.contains("saponification")));
    assert!(size(&input(Risk::Low)).checks.iter().all(|c| c.id != "specific" && c.id != "kitchen"));
    assert!(parse_specific("boiler").is_some() && parse_specific("nothing").is_none());
}

#[test]
fn a_wheeled_unit_is_never_the_only_protection() {
    let o = size(&Input { wheeled: true, ..input(Risk::High) });
    assert!(o.checks.iter().any(|c| c.id == "wheeled" && c.text.contains("half")));
    assert!(size(&input(Risk::High)).checks.iter().all(|c| c.id != "wheeled"));
}

mod command {
    use serde_json::{Value, json};

    use crate::engine::Session;

    fn run(s: &mut Session, p: Value) -> Value {
        s.execute("extinguisher_size", &p).unwrap_or_else(|e| panic!("extinguisher_size {p}: {e}"))
    }

    #[test]
    fn a_medium_risk_floor() {
        let mut s = Session::new();
        let r = run(&mut s, json!({"risk": "medium", "area": 400, "walk": 18, "units": {"a": 1, "bc": 1}}));
        assert_eq!(r["max_walk_m"], json!(20.0));
        assert_eq!(r["min_units"], json!(2));
        assert_eq!(r["ok"], json!(true));
        assert_eq!(r["capacity"]["powder_abc"], json!("2-A : 20-B:C"));
        assert_eq!(crate::engine::find("extinguisher_size").map(|c| c.undoable), Some(false));
        let r = run(&mut s, json!({"risk": "high", "walk": 16}));
        assert_eq!(r["ok"], json!(false));
    }

    #[test]
    fn a_bad_parameter_says_which() {
        let mut s = Session::new();
        for (bad, says) in [
            (json!({}), "risk"),
            (json!({"risk": "huge"}), "risk"),
            (json!({"risk": "low", "area": -1}), "area"),
            (json!({"risk": "low", "subdivisions": 0}), "subdivisions"),
            (json!({"risk": "low", "units": {"a": -1}}), "units"),
            (json!({"risk": "low", "units": {"water": 1}}), "water"),
            (json!({"risk": "low", "specific": ["moon base"]}), "moon base"),
            (json!({"risk": "low", "kind": "trolley"}), "kind"),
            (json!({"risk": "low", "magic": 1}), "magic"),
            (json!("x"), "object"),
        ] {
            let e = s.execute("extinguisher_size", &bad).expect_err("should fail");
            assert!(e.contains(says), "{bad}: {e}");
        }
    }
}
