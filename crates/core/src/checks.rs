//! The checks a sizing command replies with, each of one kind, so a reader can tell what the standard requires from what is a
//! practice or the project's own assumption. A check whose input was not given is `pending`, never a verdict, and never stops the
//! others. `reservoir_size`, `extinguisher_size`, `emergency_light` and `sprinkler_check` share them.

use serde_json::{Value, json};

/// What a check rests on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Requirement,
    Historical,
    Practice,
    Assumption,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Requirement => "current requirement",
            Kind::Historical => "historical reference",
            Kind::Practice => "engineering practice",
            Kind::Assumption => "configurable assumption",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Ok,
    Fail,
    Pending,
}

impl Status {
    pub fn name(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Fail => "fail",
            Status::Pending => "pending",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Check {
    pub id: &'static str,
    pub kind: Kind,
    pub status: Status,
    pub text: String,
    pub source: &'static str,
}

/// A requirement of `source`, passed or failed.
pub fn verdict(ok: bool) -> Status {
    if ok { Status::Ok } else { Status::Fail }
}

/// The checks as the commands reply with them.
pub fn to_json(checks: &[Check]) -> Vec<Value> {
    checks
        .iter()
        .map(|k| json!({"id": k.id, "kind": k.kind.name(), "status": k.status.name(), "text": k.text, "source": k.source}))
        .collect()
}

/// The ids of the checks left open.
pub fn pending(checks: &[Check]) -> Vec<&'static str> {
    checks.iter().filter(|k| k.status == Status::Pending).map(|k| k.id).collect()
}

/// Whether none failed: what `ok` means in a reply.
pub fn none_failed(checks: &[Check]) -> bool {
    checks.iter().all(|k| k.status != Status::Fail)
}
