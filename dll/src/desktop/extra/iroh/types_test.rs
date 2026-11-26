use azul_css::AzString;

use super::*;

#[test]
fn a_new_config_has_local_discovery_on_and_the_setter_switches_it() {
    let config = IrohConfig::create(AzString::from_const_str("app/1"));
    assert!(config.local_discovery, "local discovery is on by default");

    let off = config.clone().with_local_discovery(false);
    assert!(!off.local_discovery);
    assert!(off.clone().with_local_discovery(true).local_discovery);

    // the setter changes that field and nothing else
    assert_eq!(
        IrohConfig {
            local_discovery: true,
            ..off
        },
        config
    );
}
