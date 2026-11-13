---
slug: deploying/payment-sandbox
title: Testing Purchases
language: en
canonical_slug: deploying/payment-sandbox
audience: external
maturity: wip
guide_order: 234
topic_only: false
short_desc: Test an app's purchase flow end to end on one computer, with no provider account
prerequisites: [hello-world]
tracked_files:
  - examples/azul-pay-sandbox/src/lib.rs
  - examples/azul-pay-sandbox/src/guard.rs
  - examples/azul-pay-sandbox/src/main.rs
  - examples/azul-pay-sandbox/README.md
  - examples/azul-pay-methods/src/lib.rs
---

# Testing Purchases

An app that sells something has two halves: the app shows the payment page, and a backend
creates the payment at the provider and waits for its signed webhook. `azul-pay-sandbox` stands in
for the providers so both halves can be tested on one computer, without an account anywhere and
without real money. It speaks Stripe-, GoCardless-, dLocal-, EBANX- and KOMOJU-shaped APIs, has
small test pages for cards, SEPA mandates and the local payment methods of many countries, and
signs its webhooks the way the real providers do.

The sandbox is its own program, not part of the Azul library. That makes it the same from every
language: an app in C, Python, Java or any other binding opens the sandbox's page in its payment
window like any other URL, and its backend - in whatever language - talks HTTP to it. Nothing in
a released app changes, because nothing of the sandbox is linked into it.

## Running it

```sh
cargo run -p azul-pay-sandbox -- --port 8784 \
    --webhook-url 'http://127.0.0.1:8081/v1/webhook/{provider}'
```

It prints each provider's API address and the test keys (Stripe `sk_test_sandbox`,
`whsec_sandbox`, `pk_test_sandbox`; GoCardless `sandbox_token`, `sandbox_webhook_secret`; each
can be set with a flag). `{provider}` in the webhook URL becomes `fake-stripe`,
`fake-gocardless` and so on. A Rust backend can also start it in process with
`azul_pay_sandbox::start(Config { .. })`, or use `azul_pay_sandbox::Core` without a socket in
its unit tests.

## A purchase, step by step

1. Point the backend's provider base URL at the sandbox (for Stripe,
   `https://api.stripe.com` becomes `http://127.0.0.1:8784/stripe`) and give it the test keys.
   The backend creates its Checkout Session or Billing Request exactly as it would at the provider.
2. The app opens the session's page in its payment window.
3. Pay with a test card (`4242 4242 4242 4242`, any future expiry, any CVC) or a test IBAN
   (`DE89 3704 0044 0532 0130 00`). Declines, 3-D Secure, returned debits and chargebacks have
   their own documented test numbers, and `POST /_dev/switch` makes the next payment fail in a
   chosen way.
4. The sandbox posts the signed webhook to the backend, which approves the order.

The full list of test data, the local methods and the dev switches is in
`examples/azul-pay-sandbox/README.md`. Which methods exist in which country, and which providers
carry them, is the registry `azul-pay-methods` (`data/methods.toml`), which an app's backend and
its payment buttons can read too.

## What it refuses

The sandbox is built so it cannot be mistaken for, or turned into, a real payment page:

- it listens on 127.0.0.1 only and refuses requests whose `Host` is not a loopback name, and a
  webhook URL off this computer;
- it takes only the providers' documented test data: any other card number or IBAN, even a valid
  one, is refused and never stored or logged;
- every page carries a "TEST - NOT A REAL PAYMENT PAGE" ribbon and watermark, no provider logos,
  and cannot be framed by another site;
- a release build refuses to start it.
