use azul_core::sensors::SensorKind;

use super::*;

/// An app that reads no sensor starts none: nothing is wanted until something is read (the pump used to start the
/// backend only for an app with a `SensorChanged` node, and an app that read a sensor from a timer saw `None` for ever).
#[test]
fn nothing_is_wanted_until_something_is_read() {
    let demand = SensorDemand::new();
    for kind in [
        SensorKind::Accelerometer,
        SensorKind::Gyroscope,
        SensorKind::Magnetometer,
        SensorKind::Barometer,
        SensorKind::StepCounter,
        SensorKind::Proximity,
    ] {
        assert!(!demand.is_wanted(kind, 0), "{kind:?}");
        assert!(!demand.is_wanted(kind, 1_000_000), "{kind:?}");
    }
    let channel = SensorChannel::new();
    assert!(!channel.wanted_any(&[SensorKind::Accelerometer, SensorKind::Barometer]));
    assert_eq!(channel.started_groups(), 0, "no backend group runs at launch");
}

/// Reading a kind wants THAT kind, not the others: an app that reads the compass has no business waking the barometer
/// (whose use raises the Motion & Fitness prompt on an iPhone).
#[test]
fn a_read_wants_its_own_kind_and_no_other() {
    let mut demand = SensorDemand::new();
    demand.ask(SensorKind::Magnetometer, 10_000);
    assert!(demand.is_wanted(SensorKind::Magnetometer, 10_000));
    assert!(!demand.is_wanted(SensorKind::Barometer, 10_000), "the barometer was not read");
    assert!(!demand.is_wanted(SensorKind::StepCounter, 10_000), "the step counter was not read");
    assert!(!demand.is_wanted(SensorKind::Accelerometer, 10_000));
}

/// An app that keeps reading keeps the sensor; one that stops loses it when the lease ends.
#[test]
fn a_read_wants_the_kind_for_the_lease_and_not_a_moment_longer() {
    let mut demand = SensorDemand::new();
    demand.ask(SensorKind::Gyroscope, 5_000);
    assert!(demand.is_wanted(SensorKind::Gyroscope, 5_000 + SENSOR_LEASE_MS - 1), "still within the lease");
    assert!(!demand.is_wanted(SensorKind::Gyroscope, 5_000 + SENSOR_LEASE_MS), "the lease is over");
    // reading again renews it
    demand.ask(SensorKind::Gyroscope, 5_000 + SENSOR_LEASE_MS - 1);
    assert!(demand.is_wanted(SensorKind::Gyroscope, 5_000 + SENSOR_LEASE_MS + 100));
}

#[test]
fn a_clock_that_ran_backwards_does_not_panic_or_expire_a_read() {
    let mut demand = SensorDemand::new();
    demand.ask(SensorKind::Accelerometer, 9_000);
    assert!(demand.is_wanted(SensorKind::Accelerometer, 1_000), "an earlier `now` is within the lease, not an overflow");
}

/// The channel is the app's, so the demand of one app is not the demand of another.
#[test]
fn the_demand_belongs_to_the_channel_of_the_app() {
    let ours = SensorChannel::new();
    let theirs = SensorChannel::new();
    ours.want(SensorKind::Accelerometer);
    assert!(ours.wanted_any(&[SensorKind::Accelerometer]));
    assert!(!ours.wanted_any(&[SensorKind::Gyroscope]));
    assert!(!theirs.wanted_any(&[SensorKind::Accelerometer]), "another app's channel wants nothing");
}

#[test]
fn a_backend_keeps_what_it_started_in_the_channel_not_in_a_global() {
    let channel = SensorChannel::new();
    channel.set_group_started(0b001, true);
    channel.set_group_started(0b100, true);
    assert_eq!(channel.started_groups(), 0b101);
    channel.set_group_started(0b001, false);
    assert_eq!(channel.started_groups(), 0b100);
    channel.set_group_started(0b001, false);
    assert_eq!(channel.started_groups(), 0b100, "stopping what is stopped changes nothing");
}
