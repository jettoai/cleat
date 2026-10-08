//! `cleat reclaim [device]` (Swift `CLI.swift:215-306`): one routing request by hand, and what came
//! back. With no argument it asks for whatever `reclaim` lists; `reclaimEnabled` is not consulted,
//! because this is a button someone pressed.

use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use super::fail;
use crate::config::{paths, Config};
use crate::engine::RECLAIM_SCORE;
use crate::identity::Identity;
use crate::reclaim::{BluetoothInventory, Outcome, RouteRequesting, SmartRoutingClient, SystemProfilerPairings};
use crate::rules::reclaim::{in_rule_order, request_reason};

/// Observed in about ten milliseconds every time; this long only so a wedged daemon ends the command.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(8);

pub fn verdict(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Routed => "routed here".into(),
        Outcome::AlreadyRouted => "already here".into(),
        Outcome::HeldByRemote(d) => format!("held by the remote device ({d})"),
        Outcome::Busy => "a previous request is still running".into(),
        Outcome::Refused(d) => format!("refused ({d})"),
    }
}

pub fn reclaim(args: &[String]) -> i32 {
    let (tx, rx) = mpsc::channel();
    let bundle_id = Identity::current().bundle_id.unwrap_or_else(|| "ai.jetto.cleat".into());
    let mut client = SmartRoutingClient::new(
        bundle_id,
        Arc::new(move |_name, _address, response| {
            let _ = tx.send(response);
        }),
    );
    if !client.is_available() {
        return fail("reclaim: this macOS has no BTAudioRoutingRequest, the rule is off here");
    }

    let config_path = paths::config_path();
    let wanted: Vec<String> = if args.is_empty() {
        let Ok(config) = Config::load(&config_path) else {
            return fail(&format!("reclaim: no usable config at {}", paths::tilde(&config_path)));
        };
        if config.reclaim.is_empty() {
            return fail(&format!("reclaim: nothing listed under \"reclaim\" in {}", paths::tilde(&config_path)));
        }
        config.reclaim
    } else {
        args.to_vec()
    };

    let listed = in_rule_order(
        SystemProfilerPairings::default().paired_headsets().into_iter().filter(|h| h.is_listed(&wanted)).collect(),
    );
    if listed.is_empty() {
        return fail(&format!("reclaim: none of {} is paired with this Mac", wanted.join(", ")));
    }
    let Some(target) = listed.iter().find(|h| h.is_connected) else {
        let names: Vec<&str> = listed.iter().map(|h| h.name.as_str()).collect();
        let verb = if listed.len() == 1 { "is" } else { "are" };
        return fail(&format!("reclaim: {} {verb} paired but not connected", names.join(", ")));
    };

    println!("asking for {} ({}) with score {RECLAIM_SCORE}", target.name, target.address);
    client.request(&target.name, &target.address, RECLAIM_SCORE, &request_reason(target));
    let Ok(answer) = rx.recv_timeout(RESPONSE_TIMEOUT) else {
        return fail(&format!("reclaim: no answer in {}s", RESPONSE_TIMEOUT.as_secs()));
    };

    let outcome = answer.outcome();
    println!("action:  {}", answer.action.map_or("-".into(), |a| a.to_string()));
    println!("reason:  {}", answer.reason.as_deref().unwrap_or("-"));
    if let Some(e) = &answer.error {
        println!("error:   {e}");
    }
    println!("verdict: {}", verdict(&outcome));
    if matches!(outcome, Outcome::Routed | Outcome::AlreadyRouted) { 0 } else { 1 }
}
