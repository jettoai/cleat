//! Rule 7's outside edges: the routing SPI, the pairing list, and the child process both lean on.

pub mod child;
pub mod pairings;
pub mod response;
pub mod smart_routing;

pub use pairings::{BluetoothInventory, SystemProfilerPairings};
pub use response::{Outcome, RouteResponse};
pub use smart_routing::{Deliver, RouteRequesting, SmartRoutingClient};
