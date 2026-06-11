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

    // Reboot-safety gate + autofix demo: would the kernel GRUB defaults to
    // actually reach root? (The check that would have caught the nvidia
    // incident, where a new kernel lacked the boot disk's controller driver.)
    #[cfg(target_os = "linux")]
    if let Some(kver) = azul::desktop::extra::video_codec::provision::newest_installed_kernel() {
        use azul::desktop::extra::video_codec::provision::{reboot_safety_check, repair_kernel_plan};
        println!("\n=== Reboot-safety check (newest installed kernel) ===");
        let s = reboot_safety_check(&kver);
        println!("  kernel : {kver}");
        println!("  safe   : {}", s.safe);
        println!("  detail : {}", s.detail);
        if !s.safe {
            // if broken { fix_it(plan) } — detect a borked install and offer/run
            // the autofix (install modules-extra + rebuild initramfs).
            let repair = repair_kernel_plan(&kver);
            if repair.possible {
                println!("  -> this kernel is NOT bootable; autofix plan:");
                println!("     {}", repair.summary);
                for (i, c) in repair.commands.iter().enumerate() {
                    println!("       {}. {}", i + 1, c.display);
                }
                if std::env::args().any(|a| a == "--repair") {
                    println!("  -> applying autofix (pkexec will prompt)…");
                    let r = repair.run();
                    println!("     ok={} commands_run={} msg={}", r.ok, r.commands_run, r.message);
                    let after = reboot_safety_check(&kver);
                    println!("     re-check after fix: safe={} — {}", after.safe, after.detail);
                } else {
                    println!("     (pass --repair to apply it now)");
                }
            }
        } else {
            println!("  -> bootable; ready for hardware video decode after reboot.");
        }
    }

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
        // The app's cue to show "driver installed — reboot now?". A GUI would pop
        // a dialog; here, with --reboot, we act on it; otherwise we just offer.
        let do_reboot = std::env::args().any(|a| a == "--reboot");
        if do_reboot {
            println!("\n>>> Driver installed — rebooting now…");
            match azul::desktop::extra::video_codec::provision::reboot_now() {
                Ok(()) => println!("reboot initiated"),
                Err(e) => println!("could not reboot automatically: {e}"),
            }
        } else {
            println!(
                "\n>>> Driver installed. Reboot to load it (re-run with --reboot to do it now), \
                 then re-run — `available` should flip to true."
            );
        }
    }
}
