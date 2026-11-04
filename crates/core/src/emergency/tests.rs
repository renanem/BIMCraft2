//! Emergency lighting as IT 18/2025 of the CBPMESP asks: the lux, the spacing, the exemption of a small room, the voltage, the exit
//! luminaires of a crowded room, and how many points a corridor or a room takes.

use super::*;
use crate::checks::Status;

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9
}

fn input() -> Input {
    Input {
        level_change: false,
        lux: None,
        lux_max: None,
        autonomy_h: None,
        lumens: None,
        height: None,
        ceiling: false,
        spacing: None,
        wall: None,
        area: None,
        population: None,
        direct_exit: false,
        fire_load: false,
        voltage: None,
        existing: false,
        rcd: false,
        occupancy: None,
        capacity: None,
        enclosed: false,
        generator: false,
        switch_time: None,
        length: None,
        room: None,
    }
}

fn status(o: &Outcome, id: &str) -> Status {
    o.checks.iter().find(|c| c.id == id).map(|c| c.status).unwrap_or_else(|| panic!("no check {id}"))
}

#[test]
fn the_least_lux_is_3_on_the_level_and_5_where_it_changes() {
    assert!(near(min_lux(false), 3.0));
    assert!(near(min_lux(true), 5.0));
    assert_eq!(status(&check_all(&input()), "lux"), Status::Pending);
    assert_eq!(status(&check_all(&Input { lux: Some(3.0), ..input() }), "lux"), Status::Ok);
    assert_eq!(status(&check_all(&Input { lux: Some(2.9), ..input() }), "lux"), Status::Fail);
    assert_eq!(status(&check_all(&Input { lux: Some(4.0), level_change: true, ..input() }), "lux"), Status::Fail, "a stair wants 5");
    assert_eq!(status(&check_all(&Input { lux: Some(5.0), level_change: true, ..input() }), "lux"), Status::Ok);
}

#[test]
fn points_are_15_m_apart_and_7_5_m_from_a_wall_at_most() {
    assert_eq!(status(&check_all(&input()), "spacing"), Status::Pending);
    assert_eq!(status(&check_all(&Input { spacing: Some(15.0), wall: Some(7.5), ..input() }), "spacing"), Status::Ok);
    assert_eq!(status(&check_all(&Input { spacing: Some(15.1), ..input() }), "spacing"), Status::Fail);
    assert_eq!(status(&check_all(&Input { wall: Some(7.6), ..input() }), "spacing"), Status::Fail);
    let o = check_all(&Input { spacing: Some(20.0), ..input() });
    assert!(o.checks.iter().any(|c| c.id == "spacing" && c.text.contains("NBR 10898")), "another spacing is allowed by its parameters");
}

#[test]
fn a_corridor_and_a_room_take_as_many_points_as_their_15_m_cells() {
    assert_eq!(points_along(10.0), 1);
    assert_eq!(points_along(15.0), 1, "7.5 m to each wall");
    assert_eq!(points_along(15.1), 2);
    assert_eq!(points_along(30.0), 2);
    assert_eq!(points_along(31.0), 3);
    assert_eq!(check_all(&Input { length: Some(40.0), ..input() }).points, Some(3));
    assert_eq!(check_all(&Input { room: Some((20.0, 10.0)), ..input() }).points, Some(2), "2 by 1 cells");
    assert_eq!(check_all(&Input { room: Some((31.0, 16.0)), ..input() }).points, Some(6), "3 by 2 cells");
    assert_eq!(check_all(&input()).points, None);
}

#[test]
fn a_small_room_with_a_direct_exit_is_exempt() {
    let e = |area, pop, direct| check_all(&Input { area: Some(area), population: Some(pop), direct_exit: direct, ..input() });
    assert!(e(50.0, 49.0, true).exempt);
    assert!(!e(50.1, 49.0, true).exempt);
    assert!(!e(50.0, 50.0, true).exempt, "fewer than 50 people");
    assert!(!e(50.0, 49.0, false).exempt, "the exit must open to the corridor");
    assert!(!check_all(&input()).exempt);
    assert_eq!(status(&e(50.0, 49.0, true), "exemption"), Status::Ok);
    assert!(check_all(&input()).checks.iter().all(|c| c.id != "exemption"));
}

#[test]
fn where_there_is_fire_load_the_voltage_is_30_v_at_most() {
    assert!(
        check_all(&Input { voltage: Some(127.0), ..input() }).checks.iter().all(|c| c.id != "voltage"),
        "only where there is fire load"
    );
    let v = |voltage, existing, rcd| check_all(&Input { fire_load: true, voltage, existing, rcd, ..input() });
    assert_eq!(status(&v(None, false, false), "voltage"), Status::Pending);
    assert_eq!(status(&v(Some(30.0), false, false), "voltage"), Status::Ok);
    assert_eq!(status(&v(Some(127.0), false, false), "voltage"), Status::Fail);
    assert_eq!(status(&v(Some(127.0), true, false), "voltage"), Status::Fail, "the exception needs the 30 mA device");
    assert_eq!(status(&v(Some(127.0), true, true), "voltage"), Status::Ok, "an existing installation that cannot lower it");
    assert_eq!(status(&v(Some(127.0), false, true), "voltage"), Status::Fail, "a new one lowers the voltage");
}

#[test]
fn a_crowded_closed_room_of_some_occupations_has_beacons_over_its_exits() {
    let e = |occupancy: &str, capacity, enclosed| {
        check_all(&Input { occupancy: Some(occupancy.into()), capacity: Some(capacity), enclosed, ..input() })
    };
    assert!(e("F-3", 101.0, true).checks.iter().any(|c| c.id == "exit_beacons" && c.text.contains("permanently")));
    for ok in ["F-3", "F-5", "F-6", "F-7", "F-10", "F-11"] {
        assert!(e(ok, 101.0, true).checks.iter().any(|c| c.id == "exit_beacons"), "{ok}");
    }
    assert!(e("F-3", 100.0, true).checks.iter().all(|c| c.id != "exit_beacons"), "more than 100 people");
    assert!(e("F-3", 101.0, false).checks.iter().all(|c| c.id != "exit_beacons"), "in a closed room");
    assert!(e("A-2", 500.0, true).checks.iter().all(|c| c.id != "exit_beacons"));
}

#[test]
fn a_generator_that_switches_slowly_needs_batteries_by_the_other_standards_time() {
    assert!(check_all(&input()).checks.iter().all(|c| c.id != "switchover"));
    let o = check_all(&Input { generator: true, ..input() });
    assert_eq!(status(&o, "switchover"), Status::Pending);
    let o = check_all(&Input { generator: true, switch_time: Some(12.0), ..input() });
    assert_eq!(status(&o, "switchover"), Status::Pending, "the limit is NBR 10898's, which is not in the standard read here");
    assert!(o.checks.iter().any(|c| c.id == "switchover" && c.text.contains("NBR 10898")));
}

#[test]
fn autonomy_is_two_hours_and_the_ratio_under_20_to_1() {
    let a = |h| check_all(&Input { autonomy_h: h, ..input() });
    assert_eq!(status(&a(None), "autonomy"), Status::Pending);
    assert_eq!(status(&a(Some(2.0)), "autonomy"), Status::Ok);
    assert_eq!(status(&a(Some(1.0)), "autonomy"), Status::Fail);
    let r = |lo, hi| check_all(&Input { lux: lo, lux_max: hi, ..input() });
    assert_eq!(status(&r(Some(5.0), Some(99.0)), "ratio"), Status::Ok);
    assert_eq!(status(&r(Some(5.0), Some(100.0)), "ratio"), Status::Fail, "20:1 is not under 20:1");
    assert_eq!(status(&r(None, Some(50.0)), "ratio"), Status::Pending);
    assert_eq!(status(&r(Some(5.0), None), "ratio"), Status::Pending);
}

#[test]
fn tabela_a1_is_as_printed() {
    assert_eq!(table_a1(300.0, 2.2, true), Some((4.4, 8.8)));
    assert_eq!(table_a1(300.0, 1.8, false), Some((3.6, 7.2)));
    assert_eq!(table_a1(300.0, 1.8, true), Some((4.4, 8.8)), "no ceiling row at 1.8 m: the next one");
    assert_eq!(table_a1(500.0, 2.8, true), Some((5.6, 11.2)));
    assert_eq!(table_a1(500.0, 2.6, true), Some((5.6, 11.2)), "a row is the highest it serves");
    assert_eq!(table_a1(750.0, 3.5, true), Some((7.0, 14.0)));
    assert_eq!(table_a1(1000.0, 7.0, true), Some((7.5, 15.0)));
    assert_eq!(table_a1(2000.0, 12.0, true), Some((7.5, 15.0)));
    assert_eq!(table_a1(2000.0, 12.5, true), None, "above the table");
    assert_eq!(table_a1(600.0, 2.0, true), None, "a flux the table does not list");
    let c = |spacing, h| check_all(&Input { lumens: Some(300.0), height: Some(h), ceiling: true, spacing, ..input() });
    assert_eq!(status(&c(Some(8.8), 2.2), "table_a1"), Status::Ok);
    assert_eq!(status(&c(Some(9.0), 2.2), "table_a1"), Status::Fail);
    assert_eq!(status(&c(None, 3.0), "table_a1"), Status::Pending);
}

mod command {
    use serde_json::{Value, json};

    use crate::engine::Session;

    fn run(s: &mut Session, p: Value) -> Value {
        s.execute("emergency_light", &p).unwrap_or_else(|e| panic!("emergency_light {p}: {e}"))
    }

    #[test]
    fn a_corridor_of_a_theatre() {
        let mut s = Session::new();
        let r =
            run(&mut s, json!({"length": 42, "lux": 3.5, "spacing": 14, "wall": 7, "occupancy": "F-5", "capacity": 300, "enclosed": true}));
        assert_eq!(r["min_lux"], json!(3.0));
        assert_eq!(r["points"], json!(3));
        assert_eq!(r["ok"], json!(true));
        assert!(r["pending"].as_array().is_some_and(|p| p.iter().any(|x| x == "exit_beacons")));
        let r = run(&mut s, json!({"surface": "level_change", "lux": 3.5}));
        assert_eq!(r["min_lux"], json!(5.0));
        assert_eq!(r["ok"], json!(false));
        assert_eq!(crate::engine::find("emergency_light").map(|c| c.undoable), Some(false));
    }

    #[test]
    fn a_bad_parameter_says_which() {
        let mut s = Session::new();
        for (bad, says) in [
            (json!({"lux": -1}), "lux"),
            (json!({"surface": "ramp"}), "surface"),
            (json!({"length": 10, "room": {"width": 5, "depth": 5}}), "room"),
            (json!({"room": {"width": 5}}), "depth"),
            (json!({"room": 3}), "room"),
            (json!({"direct_exit": "yes"}), "direct_exit"),
            (json!({"occupancy": 5}), "occupancy"),
            (json!({"magic": 1}), "magic"),
            (json!("x"), "object"),
        ] {
            let e = s.execute("emergency_light", &bad).expect_err("should fail");
            assert!(e.contains(says), "{bad}: {e}");
        }
    }
}
