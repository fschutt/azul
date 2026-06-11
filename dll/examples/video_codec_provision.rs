//! Example: hardware video-decode capability + driver provisioning.
//!
//! Demonstrates the `video_codec::provision` API a downstream app uses for the
//! "you don't seem to have hardware codecs installed, but drivers are available
//! — want me to install them?" flow:
//!
//!   1. probe whether this machine can hardware-decode H.264 right now;
//!   2. if not, build the remediation plan and PRINT the exact commands that
//!      would run (consent UI);
//!   3. only with `--install` (or `AZ_PROVISION_INSTALL=1`) actually run them —
//!      pkexec pops a graphical password prompt; we never touch the password.
//!
//! Run (probe + show plan, safe):
//!     cargo run -p azul-dll --example video_codec_provision --features link-static
//! Run and actually install:
//!     cargo run -p azul-dll --example video_codec_provision --features link-static -- --install
//!
//! Does NOT open a window or run the event loop.

use azul::desktop::extra::video_codec::provision::{probe_hw_decode, ProvisionPlan};

fn main() {
    println!("=== Hardware video-decode capability ===");
    let probe = probe_hw_decode();
    println!("  available     : {}", probe.available);
    println!("  backend       : {}", probe.backend);
    println!("  detail        : {}", probe.detail);
    println!("  can_remediate : {}", probe.can_remediate);

    if probe.available {
        println!("\nHardware decode is ready — nothing to install.");
        return;
    }

    println!("\n=== Remediation plan ===");
    let plan = ProvisionPlan::detect();
    if !plan.possible {
        println!("  No driver-install plan for this machine: {}", plan.summary);
        return;
    }
    println!("  summary        : {}", plan.summary);
    println!("  needs_elevation: {}", plan.needs_elevation);
    println!("  elevation      : {}  (pkexec = OS shows its own password/biometric box; we never see the password)", plan.elevation);
    println!("  needs_reboot   : {}", plan.needs_reboot);
    println!("  commands I would run:");
    for (i, c) in plan.commands.iter().enumerate() {
        println!("    {}. {}   (elevated={})", i + 1, c.display, c.elevated);
    }

    let do_install = std::env::args().any(|a| a == "--install")
        || std::env::var("AZ_PROVISION_INSTALL").is_ok();
    if !do_install {
        println!(
            "\n(dry run — pass --install to actually run the above; pkexec will \
             prompt for your password.)"
        );
        return;
    }

    println!("\n=== Installing (pkexec will prompt for your password) ===");
    let result = plan.run();
    println!("  ok             : {}", result.ok);
    println!("  commands_run   : {}", result.commands_run);
    println!("  reboot_required: {}", result.reboot_required);
    println!("  message        : {}", result.message);
    if result.reboot_required {
        // The app's cue to show "driver installed — reboot now". Here, in the
        // CLI, we just print it.
        println!("\n>>> Driver installed. Reboot now, then re-run — `available` should flip to true.");
    }
}
