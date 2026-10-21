//! The tiers, a development sign-up, a checkout through the browser, a provider or cash, its
//! status and its abandon, and a voucher.

use super::*;

/// One storage tier: its size and its prices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tier {
    /// `100GB`, `1TB`: what a sign-up or a checkout names.
    pub id: String,
    pub quota_bytes: u64,
    /// The price of a month, in cents of `currency`; `None` when the server names none.
    pub price_cents_month: Option<u64>,
    /// The price of a year (paid yearly).
    pub price_cents_year: Option<u64>,
    /// `EUR`.
    pub currency: String,
    /// The first month costs nothing.
    pub first_month_free: bool,
}

/// `99` cents as `0.99`.
fn amount(cents: u64) -> String {
    format!("{}.{:02}", cents / 100, cents % 100)
}

impl Tier {
    /// The size as people read it: `100 GB`, `1 TB`, `12 TB` (decimal units, as the tiers are
    /// sold).
    #[must_use]
    pub fn quota_text(&self) -> String {
        const TB: u64 = 1_000_000_000_000;
        const GB: u64 = 1_000_000_000;
        let bytes = self.quota_bytes;
        if bytes >= TB && bytes % TB == 0 {
            format!("{} TB", bytes / TB)
        } else if bytes >= TB {
            format!("{:.1} TB", bytes as f64 / TB as f64)
        } else if bytes % GB == 0 {
            format!("{} GB", bytes / GB)
        } else {
            format!("{:.1} GB", bytes as f64 / GB as f64)
        }
    }

    /// The price as people read it: `EUR 0.99 a month`, or with `yearly` `EUR 9.90 a year`;
    /// `None` when the server names no price.
    #[must_use]
    pub fn price_text(&self, yearly: bool) -> Option<String> {
        let (cents, per) = if yearly {
            (self.price_cents_year?, "a year")
        } else {
            (self.price_cents_month?, "a month")
        };
        Some(format!("{} {} {per}", self.currency, amount(cents)))
    }
}

/// The token server's tier list (`GET /v1/tiers`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Tiers {
    pub tiers: Vec<Tier>,
    /// The payment methods a checkout takes: `sepa`, `card`, ...
    pub methods: Vec<String>,
    /// The sentence a buyer agrees to (the right of withdrawal ends when the service starts).
    pub withdrawal_consent: Option<String>,
}

impl Tiers {
    /// Whether the token server takes cash by post (its methods name `cash`: it has its cash key
    /// and the operator's address; cash contract v1).
    #[must_use]
    pub fn takes_cash(&self) -> bool {
        self.methods
            .iter()
            .any(|m| m.trim() == crate::cash::CASH_METHOD)
    }

    /// Reads the answer of `GET /v1/tiers`.
    pub fn parse(text: &str) -> Result<Tiers, TokenError> {
        let value: Value = serde_json::from_str(text)
            .map_err(|_| TokenError::Protocol(String::from("the tier list is not JSON")))?;
        let list = value["tiers"]
            .as_array()
            .ok_or_else(|| TokenError::Protocol(String::from("the answer has no tier list")))?;
        let mut tiers = Vec::with_capacity(list.len());
        for tier in list {
            let id = tier["id"].as_str().unwrap_or_default().trim().to_string();
            let quota_bytes = tier["quota_bytes"].as_u64().unwrap_or(0);
            if id.is_empty() || quota_bytes == 0 {
                return Err(TokenError::Protocol(String::from(
                    "a tier has no id or no size",
                )));
            }
            tiers.push(Tier {
                id,
                quota_bytes,
                price_cents_month: tier["price_cents_month"].as_u64(),
                price_cents_year: tier["price_cents_year"].as_u64(),
                currency: tier["currency"].as_str().unwrap_or("EUR").to_string(),
                first_month_free: tier["first_month_free"].as_bool().unwrap_or(false),
            });
        }
        let methods = value["methods"]
            .as_array()
            .map(|m| {
                m.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let withdrawal_consent = value["legal"]["withdrawal_consent"]
            .as_str()
            .map(str::to_string);
        Ok(Tiers {
            tiers,
            methods,
            withdrawal_consent,
        })
    }
}

// ==== The checkout ====

/// A checkout: where the browser pays for a tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkout {
    pub checkout_id: String,
    /// The payment page to open in the browser.
    pub pay_url: String,
    pub tier: String,
    pub months: u32,
    pub amount_cents: u64,
    pub currency: String,
    /// The token server's own test provider takes the payment (a development server).
    pub mock: bool,
}

/// What `GET /v1/checkout/options` is asked for (CHECKOUT-PLAN §3.11): the tier, the months
/// paid at once, the payer's country and currency, the surfaces this app can show (`fields`,
/// `page`, `browser`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionsQuery<'a> {
    pub tier: &'a str,
    pub months: u32,
    pub country: &'a str,
    pub currency: &'a str,
    pub surfaces: &'a [&'a str],
}

/// How a checkout is paid (claim contract v1, extended): the provider and method of the payer's
/// pill, the first surface to open, the VAT country (the buyer's declared country of residence),
/// the consent the order needs, and - only for a method whose provider wants them in the
/// merchant's request - the payer's details. `Debug` names those details, never their text.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CheckoutVia<'a> {
    pub provider: &'a str,
    pub method: &'a str,
    pub surface: &'a str,
    pub vat_country: &'a str,
    pub withdrawal_consent: bool,
    /// `(field, text)` pairs (`name`, `email`, `national_id`) sent once as `payer`; empty for
    /// every method that asks the app for nothing.
    pub payer: &'a [(&'a str, &'a str)],
}

impl fmt::Debug for CheckoutVia<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let payer: Vec<&str> = self.payer.iter().map(|(field, _)| *field).collect();
        f.debug_struct("CheckoutVia")
            .field("provider", &self.provider)
            .field("method", &self.method)
            .field("surface", &self.surface)
            .field("vat_country", &self.vat_country)
            .field("withdrawal_consent", &self.withdrawal_consent)
            .field("payer", &payer)
            .finish()
    }
}

/// Where a checkout stands (`GET /v1/checkout/{id}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckoutStatus {
    /// Not paid yet.
    Pending,
    /// Paid: the new drive, opened from the sign-up the token server sealed to the checkout's
    /// claim key (it answers it to every poll for 30 days).
    Approved(Box<DriveBundle>),
    /// The payment was declined or reversed: why.
    Declined(String),
    /// The token server no longer has the checkout (it expired, or it is unknown: a 404): why.
    Gone(String),
}

/// What a voucher bought.
#[derive(Debug)]
pub enum VoucherRedeemed {
    /// The drive's period grew by `days_added` (its months and its value pro rata, AZLINSEC17
    /// F29), to `period_until` (seconds since 1970).
    Extended {
        days_added: u32,
        period_until: Option<u64>,
    },
    /// A new drive of the voucher's tier (its sign-up: save it before its first refresh).
    NewDrive(Box<DriveBundle>),
}

/// The checkout an answer of `POST /v1/checkout` describes (`months`: asked for, when it names
/// none). Its id and payment page may be empty: the caller says which it needs.
fn checkout_of(value: &Value, months: u32) -> Checkout {
    let text = |key: &str| value[key].as_str().unwrap_or_default().trim().to_string();
    Checkout {
        checkout_id: text("checkout_id"),
        pay_url: text("pay_url"),
        tier: text("tier"),
        months: value["months"]
            .as_u64()
            .and_then(|m| u32::try_from(m).ok())
            .unwrap_or(months),
        amount_cents: value["amount_cents"].as_u64().unwrap_or(0),
        currency: value["currency"].as_str().unwrap_or("EUR").to_string(),
        mock: value["mock"].as_bool().unwrap_or(false),
    }
}

impl<'a> TokenServer<'a> {
    /// The storage tiers and their prices.
    pub fn tiers(&self) -> Result<Tiers, TokenError> {
        let value = self.call(Method::Get, "/v1/tiers", None, None)?;
        Tiers::parse(&value.to_string())
    }

    /// The storage tiers priced for a buyer in `country` (ISO alpha-2, any case): the signed
    /// ladder's row in that country's currency where it has one, else the ladder as it is. No
    /// country: [`Self::tiers`].
    pub fn tiers_in(&self, country: &str) -> Result<Tiers, TokenError> {
        let country = country.trim().to_ascii_uppercase();
        if country.is_empty() {
            return self.tiers();
        }
        let path = format!("/v1/tiers?country={}", uri_encode(&country, true));
        let value = self.call(Method::Get, &path, None, None)?;
        Tiers::parse(&value.to_string())
    }

    /// A drive named `name` of tier `tier` without payment - a development token server only;
    /// a production one answers [`TokenError::is_checkout_only`].
    pub fn create_dev_drive(&self, name: &str, tier: &str) -> Result<DriveBundle, TokenError> {
        let body = json!({ "name": name.trim(), "tier": tier.trim() });
        let value = self.call(Method::Post, "/v1/drives", None, Some(&body))?;
        DriveBundle::from_value(&value)
    }

    /// A checkout of `tier` for `months` months (1, 3, 6, 12 or 24) paid by `method` (`sepa`,
    /// `card`, ...): where the browser pays. Its sign-up is sealed to `claim` (the public half
    /// is sent, `claim_key`); the drive comes with [`Self::checkout_status`], opened with it.
    pub fn checkout(
        &self,
        tier: &str,
        months: u32,
        method: &str,
        claim: &ClaimKey,
    ) -> Result<Checkout, TokenError> {
        let body = json!({
            "tier": tier.trim(),
            "months": months,
            "method": method.trim(),
            "claim_key": claim.public_base64(),
        });
        let value = self.call(Method::Post, "/v1/checkout", None, Some(&body))?;
        let checkout = checkout_of(&value, months);
        if checkout.checkout_id.is_empty() || checkout.pay_url.is_empty() {
            return Err(TokenError::Protocol(String::from(
                "the checkout has no id or no payment page",
            )));
        }
        Ok(checkout)
    }

    /// The payment options for `query` (`GET /v1/checkout/options`): the offer's JSON text, for
    /// azul-pay to read and narrow; `None` from a token server without them (a 404: an older one,
    /// whose checkout is claim contract v1's - a payment page in the browser).
    pub fn checkout_options(&self, query: &OptionsQuery<'_>) -> Result<Option<String>, TokenError> {
        let path = format!(
            "/v1/checkout/options?tier={}&months={}&country={}&currency={}&surfaces={}",
            uri_encode(query.tier.trim(), true),
            query.months,
            uri_encode(&query.country.trim().to_ascii_uppercase(), true),
            uri_encode(query.currency.trim(), true),
            uri_encode(&query.surfaces.join(","), true),
        );
        match self.call(Method::Get, &path, None, None) {
            Ok(value) => Ok(Some(value.to_string())),
            Err(TokenError::Refused { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// A checkout of `tier` for `months` months paid through `via` (claim contract v1,
    /// extended): its sign-up sealed to `claim` (the public half is sent), and the whole answer -
    /// its `surface` (a fields page with a client secret, a hosted page, a page for the browser)
    /// and `return` pages, for azul-pay to check. The answer holds a client secret: never print
    /// or `Debug` it.
    ///
    /// # Errors
    ///
    /// No answer, a refusal, or an answer without a checkout id.
    pub fn checkout_via(
        &self,
        tier: &str,
        months: u32,
        via: &CheckoutVia<'_>,
        claim: &ClaimKey,
    ) -> Result<(Checkout, Value), TokenError> {
        let mut body = json!({
            "tier": tier.trim(),
            "months": months,
            "provider": via.provider.trim(),
            "method": via.method.trim(),
            "surface": via.surface.trim(),
            "vat_country": via.vat_country.trim().to_ascii_uppercase(),
            "withdrawal_consent": via.withdrawal_consent,
            "claim_key": claim.public_base64(),
        });
        // The payer's details go only where the method's provider wants them in this request;
        // the token server passes them on and keeps none.
        if !via.payer.is_empty() {
            let payer: serde_json::Map<String, Value> = via
                .payer
                .iter()
                .map(|(field, text)| ((*field).to_string(), Value::from(text.trim())))
                .collect();
            body["payer"] = Value::Object(payer);
        }
        let value = self.call(Method::Post, "/v1/checkout", None, Some(&body))?;
        let checkout = checkout_of(&value, months);
        if checkout.checkout_id.is_empty() {
            return Err(TokenError::Protocol(String::from("the checkout has no id")));
        }
        Ok((checkout, value))
    }

    /// A cash checkout of `tier` for `months` months (cash contract v1): `POST /v1/checkout
    /// {"method": "cash"}` with the VAT country and the order's consent, its sign-up sealed to
    /// `claim` - no provider, no surface: the token server answers the activation code, the
    /// address to post the cash to and the checkout's end, all in the answer for azul-pay to
    /// read. 400 `cash_unavailable` from a server that takes no cash.
    ///
    /// # Errors
    ///
    /// No answer, a refusal, or an answer without a checkout id.
    pub fn checkout_cash(
        &self,
        tier: &str,
        months: u32,
        vat_country: &str,
        withdrawal_consent: bool,
        claim: &ClaimKey,
    ) -> Result<(Checkout, Value), TokenError> {
        let body = json!({
            "tier": tier.trim(),
            "months": months,
            "method": crate::cash::CASH_METHOD,
            "vat_country": vat_country.trim().to_ascii_uppercase(),
            "withdrawal_consent": withdrawal_consent,
            "claim_key": claim.public_base64(),
        });
        let value = self.call(Method::Post, "/v1/checkout", None, Some(&body))?;
        let checkout = checkout_of(&value, months);
        if checkout.checkout_id.is_empty() {
            return Err(TokenError::Protocol(String::from("the checkout has no id")));
        }
        Ok((checkout, value))
    }

    /// The checkout `checkout_id` on another surface (`POST /v1/checkout/{id}/surface`, the
    /// fallback chain: the hosted page, the browser): the answer, its `surface` for azul-pay to
    /// check. It holds a client secret: never print it.
    pub fn checkout_surface(&self, checkout_id: &str, kind: &str) -> Result<Value, TokenError> {
        let path = format!("/v1/checkout/{}/surface", check_id(checkout_id.trim())?);
        self.call_or_null(Method::Post, &path, None, Some(&json!({ "kind": kind.trim() })))
    }

    /// Abandons the checkout `checkout_id` (`POST /v1/checkout/{id}/abandon`): the provider
    /// session expires, so a forgotten browser tab can never take money for it.
    pub fn abandon_checkout(&self, checkout_id: &str) -> Result<(), TokenError> {
        let path = format!("/v1/checkout/{}/abandon", check_id(checkout_id.trim())?);
        self.send(Method::Post, &path, None, Some(&json!({})))
            .map(|_| ())
    }

    /// Where the checkout `checkout_id` stands; once it is paid, the new drive - its sign-up
    /// opened with `claim`, the key the checkout named (every poll for 30 days answers it).
    ///
    /// # Errors
    ///
    /// No answer or a refusal; [`TokenError::Protocol`] for an approved checkout whose sign-up
    /// is not sealed to `claim` for this checkout (or is no drive bundle).
    pub fn checkout_status(
        &self,
        checkout_id: &str,
        claim: &ClaimKey,
    ) -> Result<CheckoutStatus, TokenError> {
        let id = checkout_id.trim();
        if id.is_empty() {
            return Err(TokenError::Config(String::from("There is no checkout to ask about.")));
        }
        let path = format!("/v1/checkout/{}", uri_encode(id, true));
        let value = match self.call(Method::Get, &path, None, None) {
            Ok(value) => value,
            Err(TokenError::Refused {
                status: 404,
                code,
                message,
            }) => {
                let why = match (message.is_empty(), code.is_empty()) {
                    (false, _) => message,
                    (true, false) => code,
                    (true, true) => String::from("the token server does not know the checkout"),
                };
                return Ok(CheckoutStatus::Gone(why));
            }
            Err(e) => return Err(e),
        };
        let reason = |default: &str| value["reason"].as_str().unwrap_or(default).to_string();
        match value["status"].as_str().unwrap_or_default() {
            // A cash checkout waits for its letter like any unpaid one (cash contract v1).
            "pending" | "awaiting_cash" => Ok(CheckoutStatus::Pending),
            "approved" => {
                // The plaintext `signup` of a token server without claims is never taken: anyone
                // who knew the checkout id could have read it.
                let sealed = value["sealed_signup"]
                    .as_str()
                    .filter(|s| !s.trim().is_empty())
                    .ok_or_else(|| {
                        TokenError::Protocol(String::from(
                            "the approved checkout has no sealed sign-up",
                        ))
                    })?;
                let signup = claim
                    .open(id, sealed)
                    .map_err(|e| TokenError::Protocol(e.to_string()))?;
                Ok(CheckoutStatus::Approved(Box::new(DriveBundle::parse(
                    &signup,
                )?)))
            }
            "declined" | "reversed" => Ok(CheckoutStatus::Declined(reason(
                "the payment was declined",
            ))),
            // The operator did not take the letter's cash (the amount, the slip).
            "rejected" => Ok(CheckoutStatus::Declined(reason(
                "the cash sent was not accepted",
            ))),
            "expired" => Ok(CheckoutStatus::Gone(reason(
                "the checkout expired at the token server",
            ))),
            other => Err(TokenError::Protocol(format!(
                "the checkout is \"{other}\", neither pending, approved, declined nor expired"
            ))),
        }
    }

    /// A voucher `code` (`POST /v1/vouchers/redeem`): on the drive `drive` (its id and this
    /// device's drive token, which it does not spend) the days it added; without one a new drive
    /// of `tier` (empty: the voucher's own, else the token server's default).
    ///
    /// # Errors
    ///
    /// [`TokenError::Config`] for an empty code (nothing is sent); the token server's refusals
    /// (`voucher_invalid`, `voucher_too_small`, `bad_tier`).
    pub fn redeem_voucher(
        &self,
        code: &str,
        drive: Option<(&str, &str)>,
        tier: &str,
    ) -> Result<VoucherRedeemed, TokenError> {
        let code = code.trim();
        if code.is_empty() {
            return Err(TokenError::Config(String::from("There is no voucher code.")));
        }
        let Some((drive_id, drive_token)) = drive else {
            let mut body = json!({ "code": code });
            if !tier.trim().is_empty() {
                body["tier"] = json!(tier.trim());
            }
            let value = self.call(Method::Post, "/v1/vouchers/redeem", None, Some(&body))?;
            return Ok(VoucherRedeemed::NewDrive(Box::new(
                DriveBundle::from_value(&value)?,
            )));
        };
        let body = json!({ "code": code, "drive_id": check_id(drive_id)? });
        let value = self.call(
            Method::Post,
            "/v1/vouchers/redeem",
            Some(token_of(drive_token)?),
            Some(&body),
        )?;
        // An older token server answers whole months only.
        let days_added = value["days_added"]
            .as_u64()
            .or_else(|| value["months_added"].as_u64().map(|m| m.saturating_mul(30)))
            .and_then(|d| u32::try_from(d).ok())
            .unwrap_or(0);
        Ok(VoucherRedeemed::Extended {
            days_added,
            period_until: value["period_until"]
                .as_str()
                .and_then(azul_storage::time::parse_iso8601),
        })
    }
}
