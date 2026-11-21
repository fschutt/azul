# azul-pay-sandbox

A **local test tool** for an app's purchase flow: payment-provider-shaped test APIs, small test
pages and signed webhooks, so a purchase can be tested end to end on one computer without an
account at any provider. It **does not reproduce any provider's real UI or branding**, and it
**refuses real payment data**: only the providers' documented test data works (test cards, test
IBANs, and for the local methods the codes, phone numbers and email patterns the providers
document).

What it stands in for:

| Provider shape | API base URL to replace | Pages | Webhooks |
|---|---|---|---|
| Stripe | `https://api.stripe.com` -> `http://127.0.0.1:8784/stripe` | split-screen checkout (`ui_mode=hosted_page`), the card fields page for a payment popover (`ui_mode=elements`, `/fields/fake-stripe/v1`), a 3-D Secure step | `Stripe-Signature: t=..,v1=..` (HMAC-SHA256 of `"<t>.<body>"`) |
| GoCardless | `https://api-sandbox.gocardless.com` -> `http://127.0.0.1:8784/gocardless` | the Billing Request Flow: account holder and IBAN, the mandate, the bank's confirmation | `Webhook-Signature` (HMAC-SHA256 of the body), events batched as `{"events": [...]}` |
| dLocal | `https://sandbox.dlocal.com` -> `http://127.0.0.1:8784/dlocal` | the hosted checkout (`payment_method_flow: REDIRECT`) of Pix, Boleto and OXXO, and a fake phone | `X-Date` and `Authorization: V2-HMAC-SHA256, Signature: ..` (HMAC-SHA256 under the secret key of login, date and body), the payment object as the body |
| EBANX | `https://sandbox.ebanxpay.com` -> `http://127.0.0.1:8784/ebanx` | the Payment Page of Pix, Boleto and OXXO, and a fake phone | a form naming the payment's hash only, posted to the webhook URL with `?token=<notification token>`; unsigned (EBANX signs with RSA), so the backend reads the payment back through `/ws/query` |

## How any app tests a purchase

1. Run the sandbox:

   ```sh
   azul-pay-sandbox --port 8784 --webhook-url 'http://127.0.0.1:8081/v1/webhook/{provider}'
   ```

   It prints each provider's API address and the test keys (defaults: Stripe `sk_test_sandbox`,
   `whsec_sandbox`, `pk_test_sandbox`; GoCardless `sandbox_token`, `sandbox_webhook_secret`;
   each can be set with a flag, `@file` reads it from a file). `{provider}` in the webhook URL
   becomes `fake-stripe` or `fake-gocardless`. From Rust, `azul_pay_sandbox::start(Config { .. })`
   does the same in process and gives the base URL; `azul_pay_sandbox::Core` is the same thing
   without a socket, for tests.
2. Point the backend's provider base URLs at the sandbox and give it the test keys. Nothing else
   changes: the backend creates a Checkout Session or a Billing Request exactly as it would at
   the provider.
3. Open the session's page (its `url`, or the fields page with `#pk=..&cs=<client secret>`) in the
   app's payment popover or a browser, and pay with a test card (`4242 4242 4242 4242`, any
   future expiry, any CVC) or a test IBAN (`DE89 3704 0044 0532 0130 00`).
4. The sandbox posts the signed webhook to the backend, which approves the order.

The fields page talks to the app the way a hosted-fields page in a payment popover does: it
navigates its main frame to `/_bridge/<message>?...` (`ready`, `height`, `brand`, `complete`,
`last4`, `error`, `result`; the app cancels these navigations, a browser gets a 204 and stays)
and hears the app through new fragments (`#...&cmd=confirm&name=..&n=<seq>`, `cmd=reset`).

### Test data

Cards (Stripe's documented test cards, https://docs.stripe.com/testing, read 2026-10-17):
`4242 4242 4242 4242`, `5555 5555 5555 4444`, `3782 822463 10005` and the country cards pay;
`4000 0000 0000 0002` (declined), `...9995` (insufficient funds), `...0069` (expired), `...0127`
(wrong CVC), `...0119` (processing error), `4242 4242 4242 4241` (incorrect number) are declined
with Stripe's messages; `4000 0025 0000 3155`, `4000 0027 6000 3184` ask for 3-D Secure;
`4000 0000 0000 0259` is paid and then disputed.

IBANs: `DE89 3704 0044 0532 0130 00`, `AT61 1904 3002 3457 3201` are collected;
`AT86 1904 3002 3547 3202`, `DE91 1000 0000 0123 4567 89` are returned; `AT59 1904 3002 3547 3203`
is collected and then disputed (the full list is in `src/testdata.rs`).

### Local payment methods

The Stripe shape also takes the local methods the payment methods registry
(`azul-pay-methods`, `data/methods.toml`) has Stripe's documented test data for: iDEAL | Wero,
Wero, Bancontact, EPS, Przelewy24, BLIK, TWINT, Swish, MobilePay, Vipps, MB WAY, Multibanco,
Bizum, Satispay, Revolut Pay, Amazon Pay, Pay by Bank, UPI, Pix, Alipay, WeChat Pay, the Korean
methods, Konbini, PayPay, OXXO, PayNow, PromptPay, FPX and GrabPay. Name one in
`payment_method_types[0]`; a type without documented test data is refused when the session is
made. Each flow has one small page:

- a bank redirect: a list of "Test Bank A / B / C" (never a real bank) and Authorize / Fail;
- a code (BLIK): the documented code `123456`, then Approve / Decline / Let it time out on the
  fake phone; a documented email (`customer_declined@example.com`, ...) fails it;
- a QR (TWINT, Swish, UPI, WeChat Pay, Wero, ...): a box whose "code" is the loopback URL of the
  fake phone, where the payment is approved or declined - never a real payment payload;
- a phone number (MB WAY): only the documented numbers (`+351911111112` pays after a while,
  `+351911111113`-`116` fail with their documented codes); the other apps (MobilePay, Vipps,
  Bizum) are approved on the fake phone;
- Pix and the vouchers (Multibanco, OXXO, Konbini): a documented email
  (`succeed_immediately@example.com`, `fill_never@example.com`, ...) decides whether the timeline
  pays the code or voucher or lets it expire.

Stripe's refusals are mirrored: a method paid in another currency needs
`adaptive_pricing[enabled]=true` (the page then shows the converted price at a marked fake rate),
a method that cannot recur is refused in subscription mode, WeChat Pay needs
`payment_method_options[wechat_pay][client]=web`.

### GoCardless's other schemes

Besides SEPA (`sepa_core`, the test IBANs above), the GoCardless shape takes every scheme the
registry has GoCardless's documented sandbox account for, in its own currency
(https://docs.gocardless.com/docs/developer-resources/test-bank-details): Bacs (GBP, sort code
and account `200000 55779911`), Autogiro (SEK, `5491 0000003 198112289874`), Betalingsservice (DKK,
`345 3179681 0101701234`), BECS (AUD, `082-082 012345678`), BECS NZ (NZD, `12 3113 0003869-00`),
PAD (CAD, `0003 00006 1234567`) and ACH (USD, `026073150 2715500356`). The flow asks for that
scheme's account fields and takes only the documented account; a scheme in another currency is
refused as GoCardless refuses it.

### dLocal's and EBANX's hosted pages

Both shapes take the local methods of Latin America the registry has documented test data for
(dLocal: Pix `PQ`, Boleto `BL`, OXXO `OX`; EBANX: `pix`, `boleto`, `oxxo`), in the method's own
currency. Their APIs want the payer's identity as the real ones do (dLocal the name, email and
document of `payer`; EBANX the `name` and `email`): it must be there, and it is dropped at once;
reads and notifications name a fixed test payer ("[TEST] payer") instead. The payer's page shows
the order and a QR box or a voucher whose "payment" is the fake phone, where Pay or Decline
decides (dLocal's sandbox tools set PAID or REJECTED, EBANX's sandbox confirms or cancels).

dLocal's shape checks every request's `X-Login`, `X-Trans-Key`, `X-Date` and signature (default
keys `dlocal_sandbox_login`, `dlocal_sandbox_trans_key`, `dlocal_sandbox_secret`); payments read
back (`GET /payments/{id}`), cancel (`POST /payments/{id}/cancel`) and refund (`POST /refunds`:
PENDING, then SUCCESS with its own notification). EBANX's shape wants its integration key in
every body (default `ebanx_sandbox_key`) and answers with HTTP 200 and its own
`status`; `/ws/query`, `/ws/cancel` (an open page only) and `/ws/refund` (RE, then CO with a
`refund` notification) behave as EBANX documents. Its notifications go to the webhook URL with
`?token=ebanx_sandbox_notification_token`, the token a backend checks.

### Dev switches

`POST /_dev/switch {"name": ...}` makes the next card decline (`decline_next`), ask for 3-D
Secure (`three_ds_next`), the next debit come back (`return_next_debit`), the next payment be
charged back (`chargeback_next`), the next webhook be dropped (`drop_next_webhook`) or sent twice
(`duplicate_next_webhook`). `POST /_dev/hang {"prefix": "/stripe/v1/", "secs": 60}` leaves the next request under the prefix
unanswered for that long (a provider that hangs: the token server gives it up after its own timeout).
`POST /_dev/drop {"count": N}` loses the next N webhooks, `POST /_dev/drop {"id": "cs_..."}` every
webhook that names the id (a payment's, a checkout session's, a mandate's) until `/_dev/reset`:
all of a payment's events lost, which only a reconcile against the provider finds.
`POST /_dev/delay {"secs": N}` holds webhooks back,
`POST /_dev/advance {"months": N}` charges the next months of every recurring plan,
`GET /_dev/state` shows the objects' outcomes and the webhook log, `GET /_dev/keys` each
provider's API address and test keys (what a backend is configured with). A webhook the backend does
not answer with 2xx is sent again, at most five times.

### The pages in a browser

`node browser_test.mjs` (Node 22 or later, Chrome or Chromium; no packages) runs the pages in
headless Chrome with a fresh temporary profile: a test card paid, declined, stopped by the page's
own check, refused as a real number, through 3-D Secure; the fields page over its bridge; a SEPA
mandate through the Direct Debit flow; the test look of every page. It starts the sandbox it
finds in `target/debug` (or the command after `--`) on a free port and receives and verifies the
webhooks itself. One PASS / FAIL line per flow; the exit code is the number of failures.

## The rules (each one has a test)

1. **Loopback only.** It listens on 127.0.0.1 only; there is no setting for another address. A
   request whose `Host` is not a loopback name or address is refused, which also defeats DNS
   rebinding. It refuses to start with a webhook URL off this computer.
2. **Unmistakably fake look.** A fixed ribbon on top and a diagonal "TEST - NOT A REAL PAYMENT
   PAGE" watermark across every form, a hazard yellow-and-black frame in a palette no provider
   uses, every title starts with "[TEST]". Provider names appear only as descriptive text
   ("Stripe-shaped test provider"), never as a logo or wordmark; no images, fonts or anything
   from another origin. The layout and flow follow the real ones; the look does not.
3. **Test data only.** Only the documented test card numbers and test IBANs are taken; any other
   number, even a valid one, is refused ("This sandbox only accepts test card numbers") and is
   neither stored nor logged. No card number, CVC, expiry, IBAN or name is ever kept or put into
   a webhook: only outcomes are kept. The payer's identity a dLocal or EBANX request carries is
   checked to be there and dropped: never kept, shown back or sent.
4. **No embedding elsewhere.** Every answer carries `Content-Security-Policy: frame-ancestors
   'self' http://127.0.0.1:* http://localhost:* http://[::1]:*`, `X-Frame-Options: SAMEORIGIN`,
   `Referrer-Policy: no-referrer` and `X-Content-Type-Options: nosniff`.
5. **Never in release.** A release build refuses to start the sandbox, unless a local test
   stack opts in explicitly (`--local-test-stack`, `AZ_PAY_SANDBOX_ALLOW_RELEASE=1`, or
   `Config::local_test_stack` for a program that links it): the opt-in lifts this check alone,
   and the sandbox still serves on 127.0.0.1 only and takes test data only. An app that links it
   behind a feature makes that feature fail to compile in a release build, and its payment
   registry takes the sandbox's origins only in builds with its development registry.
6. **It says so.** This README, the ribbon and the index page (`GET /`) state that it is a local
   test tool.
