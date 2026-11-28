//! The network the app reaches without a dialog: iroh's LAN discovery must be OFF until the app asks for it. A phone shows the
//! Local Network dialog (iOS) / takes the multicast lock (Android) the moment discovery starts, so an endpoint made with the
//! default config must not announce itself on the Wi-Fi: this row binds one and reads what it says about discovery.

use azul::{
    iroh::{IrohConfig, IrohEndpoint},
    str::String as AzString,
    window::PlatformCapability,
};

use super::{describe_capability, plain, Ctx, ProbeDef, Step};
use crate::report::Verdict;

pub static PROBES: &[ProbeDef] = &[plain("network.iroh_lan", "network", "LAN discovery of an iroh endpoint is off by default", iroh_lan)];

fn iroh_lan(_ctx: &mut Ctx<'_>) -> Step {
    let (available, backend) = describe_capability(&PlatformCapability::iroh());
    if !available {
        return Step::Done(Verdict::Unavailable(format!("iroh is not available in this build: {backend}")));
    }
    let mut endpoint = IrohEndpoint::bind(IrohConfig::create(AzString::from("azselftest/lan/1")));
    if !endpoint.is_bound() {
        endpoint.close();
        return Step::Done(Verdict::Unavailable(format!("the endpoint did not bind (no network?): {backend}")));
    }
    let status = endpoint.local_discovery_status();
    let verdict = if status.as_str() == "off" {
        Verdict::Pass(format!("a default endpoint does not announce itself on the LAN (status {:?}) via {backend}", status.as_str()))
    } else {
        Verdict::Fail(format!(
            "a default endpoint announces itself on the LAN: local discovery says {:?} (it is opt-in: IrohConfig::with_local_discovery(true))",
            status.as_str()
        ))
    };
    endpoint.close();
    Step::Done(verdict)
}
