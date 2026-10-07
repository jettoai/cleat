//! Rule 3: hold the default output's left/right balance. An unreadable balance means "not ready".

use crate::config::Config;
use crate::model::{Action, DeviceSnapshot};

pub const TOLERANCE: f64 = 0.01;

pub fn reconcile(snapshot: &DeviceSnapshot, config: &Config) -> Vec<Action> {
    let (Some(wanted), Some(id), Some(current)) = (config.balance, snapshot.default_output, snapshot.output_balance)
    else {
        return vec![];
    };
    if (current as f64 - wanted).abs() <= TOLERANCE {
        return vec![];
    }
    let name = snapshot.device(id).map_or("output", |d| d.name.as_str());
    vec![Action::SetBalance(id, wanted as f32, format!("{} {:.2} -> {:.2}", name, current as f64, wanted))]
}
