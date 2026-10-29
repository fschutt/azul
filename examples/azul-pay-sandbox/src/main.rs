//! `azul-pay-sandbox`: the payment sandbox as a program, for any app's backend in any language.
//!
//! ```text
//! azul-pay-sandbox [--port 8784] [--webhook-url http://127.0.0.1:8081/v1/webhook/{provider}]
//!                  [--providers stripe,gocardless,dlocal,ebanx,komoju] [--step-secs 2]
//!                  [--stripe-secret-key K] [--stripe-webhook-secret S] [--stripe-publishable-key P]
//!                  [--gocardless-access-token T] [--gocardless-webhook-secret S]
//!                  [--dlocal-login L] [--dlocal-trans-key K] [--dlocal-secret-key S]
//!                  [--ebanx-integration-key K] [--ebanx-notification-token T]
//!                  [--local-test-stack]
//! ```
//!
//! A key's value may be `@<file>` to read it from a file. It prints where each provider's API is
//! and the keys to configure the backend with, then serves until it is stopped. A release build
//! starts only for a local test stack that says so: `--local-test-stack`, or
//! `AZ_PAY_SANDBOX_ALLOW_RELEASE=1`.

use azul_pay_sandbox::{guard, start, Config, Kind};

fn value(raw: &str) -> Result<String, String> {
    match raw.strip_prefix('@') {
        Some(path) => std::fs::read_to_string(path)
            .map(|s| s.trim().to_string())
            .map_err(|e| format!("{path}: {e}")),
        None => Ok(raw.to_string()),
    }
}

fn parse(args: &[String]) -> Result<Config, String> {
    let mut cfg = Config::default();
    cfg.local_test_stack = guard::release_opted_in(std::env::var(guard::ALLOW_RELEASE_ENV).ok().as_deref());
    let mut i = 0;
    while let Some(flag) = args.get(i) {
        if flag == "--help" || flag == "-h" {
            return Err(String::new());
        }
        // the one switch without a value
        if flag == "--local-test-stack" {
            cfg.local_test_stack = true;
            i += 1;
            continue;
        }
        let next = args.get(i + 1).ok_or_else(|| format!("{flag} needs a value"))?;
        match flag.as_str() {
            "--port" => cfg.port = next.parse().map_err(|_| format!("--port: not a port: {next}"))?,
            "--webhook-url" => cfg.webhook_url = next.clone(),
            "--providers" => {
                let mut list = Vec::new();
                for name in next.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                    list.push(Kind::parse(name).ok_or_else(|| format!("--providers: no provider {name} (stripe, gocardless, dlocal, ebanx, komoju)"))?);
                }
                cfg.providers = list;
            }
            "--step-secs" => {
                cfg.step_secs = next.parse().ok().filter(|s: &i64| (1..=3600).contains(s)).ok_or("--step-secs: 1 to 3600")?;
            }
            "--stripe-secret-key" => cfg.stripe.secret_key = value(next)?,
            "--stripe-webhook-secret" => cfg.stripe.webhook_secret = value(next)?,
            "--stripe-publishable-key" => cfg.stripe.publishable_key = value(next)?,
            "--gocardless-access-token" => cfg.gocardless.secret_key = value(next)?,
            "--gocardless-webhook-secret" => cfg.gocardless.webhook_secret = value(next)?,
            "--dlocal-login" => cfg.dlocal.login = value(next)?,
            "--dlocal-trans-key" => cfg.dlocal.trans_key = value(next)?,
            // dLocal signs its notifications with the same secret key it checks requests with
            "--dlocal-secret-key" => {
                cfg.dlocal.secret_key = value(next)?;
                cfg.dlocal.webhook_secret = cfg.dlocal.secret_key.clone();
            }
            "--ebanx-integration-key" => cfg.ebanx.secret_key = value(next)?,
            "--ebanx-notification-token" => cfg.ebanx.webhook_secret = value(next)?,
            other => return Err(format!("unknown argument {other}")),
        }
        i += 2;
    }
    Ok(cfg)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cfg = match parse(&args) {
        Ok(c) => c,
        Err(why) => {
            if !why.is_empty() {
                eprintln!("azul-pay-sandbox: {why}");
            }
            eprintln!("usage: azul-pay-sandbox [--port 8784] [--webhook-url URL] [--providers stripe,gocardless,dlocal,ebanx,komoju] [--step-secs 2] [--stripe-secret-key K] [--stripe-webhook-secret S] [--stripe-publishable-key P] [--gocardless-access-token T] [--gocardless-webhook-secret S] [--dlocal-login L] [--dlocal-trans-key K] [--dlocal-secret-key S] [--ebanx-integration-key K] [--ebanx-notification-token T] [--local-test-stack]");
            std::process::exit(2);
        }
    };
    let providers = cfg.providers.clone();
    let (stripe, gocardless, hooks) = (cfg.stripe.clone(), cfg.gocardless.clone(), cfg.webhook_url.clone());
    let (dlocal, ebanx, komoju) = (cfg.dlocal.clone(), cfg.ebanx.clone(), cfg.komoju.clone());
    let running = match start(cfg) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("azul-pay-sandbox: {e}");
            std::process::exit(1);
        }
    };
    println!("azul-pay-sandbox (TEST ONLY: test cards and test IBANs, this computer only) on {}", running.base_url());
    println!("  webhooks to {hooks}");
    for kind in providers {
        match kind {
            Kind::Stripe => println!(
                "  {}: API {}  secret key {}  webhook secret {}  publishable key {}",
                kind.id(),
                running.api_url(kind),
                stripe.secret_key,
                stripe.webhook_secret,
                stripe.publishable_key
            ),
            Kind::GoCardless => println!(
                "  {}: API {}  access token {}  webhook secret {}",
                kind.id(),
                running.api_url(kind),
                gocardless.secret_key,
                gocardless.webhook_secret
            ),
            Kind::DLocal => println!(
                "  {}: API {}  login {}  trans key {}  secret key {}",
                kind.id(),
                running.api_url(kind),
                dlocal.login,
                dlocal.trans_key,
                dlocal.secret_key
            ),
            Kind::Ebanx => println!(
                "  {}: API {}  integration key {}  notification URL: the webhook URL with ?token={}",
                kind.id(),
                running.api_url(kind),
                ebanx.secret_key,
                ebanx.webhook_secret
            ),
            Kind::Komoju => println!(
                "  {}: API {}/api/v1  secret key {}  webhook secret token {}",
                kind.id(),
                running.api_url(kind),
                komoju.secret_key,
                komoju.webhook_secret
            ),
        }
    }
    running.wait();
}
