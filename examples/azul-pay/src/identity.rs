//! What a payment method tells about the payer, and which part of it the app asks for.
//!
//! Azlin keeps nothing about who pays: the files are encrypted and the drive is not linked to the
//! payment. A payment method may still tell its provider who pays - a card network sees the
//! cardholder's name, a mandate names the account holder, some providers want a national ID. The
//! token server's offer names, per method, two lists of detail names:
//!
//! ```json
//! {"method": "card", "needs_identity": ["name"], "identity_in_app": []}
//! {"method": "pix", "needs_identity": ["name", "email", "national_id"],
//!  "identity_in_app": ["name", "email", "national_id"]}
//! ```
//!
//! - `needs_identity`: everything the provider learns, wherever it is typed. The pill says so
//!   before the method is picked.
//! - `identity_in_app`: the part the provider wants in the merchant's request because its own
//!   page does not ask for it. The popover asks for exactly these, they go once with
//!   `POST /v1/checkout` as `payer`, and the app keeps none of them. The rest the provider's own
//!   page asks for. The app never asks for a postal address: a method that wants one from it is
//!   not shown.
//!
//! A method without either list asks for nothing beyond what paying with it implies.
//! [`Payer`] never shows what was typed: its `Debug` names the filled fields only.

use std::fmt;

use serde_json::Value;

/// One detail about the payer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PayerField {
    Name,
    Email,
    /// A national identity number (Brazil's CPF, for example).
    NationalId,
    Phone,
    TaxId,
    /// A postal address.
    Address,
}

impl PayerField {
    pub const ALL: [PayerField; 6] = [
        PayerField::Name,
        PayerField::Email,
        PayerField::NationalId,
        PayerField::Phone,
        PayerField::TaxId,
        PayerField::Address,
    ];

    /// The wire name (`name`, `email`, `national_id`, ...).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            PayerField::Name => "name",
            PayerField::Email => "email",
            PayerField::NationalId => "national_id",
            PayerField::Phone => "phone",
            PayerField::TaxId => "tax_id",
            PayerField::Address => "address",
        }
    }

    /// The detail's English words (`email address`); an app says them in its own language by
    /// the wire name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            PayerField::Name => "name",
            PayerField::Email => "email address",
            PayerField::NationalId => "national ID number",
            PayerField::Phone => "phone number",
            PayerField::TaxId => "tax number",
            PayerField::Address => "postal address",
        }
    }

    /// A wire name, in any case.
    #[must_use]
    pub fn parse(text: &str) -> Option<PayerField> {
        let text = text.trim();
        PayerField::ALL
            .into_iter()
            .find(|f| f.as_str().eq_ignore_ascii_case(text))
    }
}

/// The details the app itself may ask for; an address stays with the provider's own page.
pub const APP_FIELDS: &[PayerField] = &[
    PayerField::Name,
    PayerField::Email,
    PayerField::NationalId,
    PayerField::TaxId,
    PayerField::Phone,
];

/// What a method tells its provider about the payer, and which part the app asks for; no fields
/// for a method that needs nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Identity {
    /// Everything the provider learns, in the server's order, each once (the app's part
    /// included): what the pill's note names.
    pub fields: Vec<PayerField>,
    /// The part the provider wants in the merchant's request: the popover's fields, sent once
    /// with the checkout. Empty where the provider's own page asks for everything.
    pub in_app: Vec<PayerField>,
}

impl Identity {
    /// The method needs nothing about the payer.
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.fields.is_empty()
    }

    /// The fields the app has to ask for.
    #[must_use]
    pub fn app_fields(&self) -> &[PayerField] {
        &self.in_app
    }

    /// What the provider's own page asks for: everything it learns that the app does not ask.
    #[must_use]
    pub fn page_fields(&self) -> Vec<PayerField> {
        self.fields
            .iter()
            .copied()
            .filter(|f| !self.in_app.contains(f))
            .collect()
    }

    /// Reads a method's `needs_identity` and `identity_in_app` (see the module docs): nothing for
    /// missing or `null` lists.
    ///
    /// # Errors
    ///
    /// Why the method cannot be shown: a list that is none, an unknown detail, or a detail the app
    /// never asks for itself in `identity_in_app`.
    pub fn parse(method: &Value) -> Result<Identity, String> {
        let learns = details(&method["needs_identity"], "needs_identity")?;
        let in_app = details(&method["identity_in_app"], "identity_in_app")?;
        if let Some(field) = in_app.iter().find(|f| !APP_FIELDS.contains(f)) {
            return Err(format!("the app never asks for the payer's {}", field.as_str()));
        }
        let mut fields = learns;
        for field in &in_app {
            if !fields.contains(field) {
                fields.push(*field);
            }
        }
        Ok(Identity { fields, in_app })
    }
}

/// The details a list `value` names, each once; `key` for the error.
fn details(value: &Value, key: &str) -> Result<Vec<PayerField>, String> {
    let list = match value {
        Value::Null => return Ok(Vec::new()),
        Value::Array(list) => list,
        _ => return Err(format!("its {key} is no list")),
    };
    let mut out = Vec::new();
    for item in list {
        let text = item.as_str().unwrap_or_default();
        let Some(field) = PayerField::parse(text) else {
            return Err(format!("its {key} names an unknown payer detail {text:?}"));
        };
        if !out.contains(&field) {
            out.push(field);
        }
    }
    Ok(out)
}

/// What the payer typed into the popover for a method the app asks for: sent once with the
/// checkout and dropped. `Debug` names the filled fields, never their text.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Payer {
    pub name: String,
    pub email: String,
    pub national_id: String,
    pub tax_id: String,
    pub phone: String,
}

impl fmt::Debug for Payer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let filled: Vec<&str> = APP_FIELDS
            .iter()
            .filter(|field| !self.get(**field).trim().is_empty())
            .map(|field| field.as_str())
            .collect();
        write!(f, "Payer {{ filled: {filled:?} }}")
    }
}

impl Payer {
    /// What was typed for `field` (empty for a field the app never asks for).
    #[must_use]
    pub fn get(&self, field: PayerField) -> &str {
        match field {
            PayerField::Name => &self.name,
            PayerField::Email => &self.email,
            PayerField::NationalId => &self.national_id,
            PayerField::TaxId => &self.tax_id,
            PayerField::Phone => &self.phone,
            PayerField::Address => "",
        }
    }

    /// The field `field` says `text` now (a field the app never asks for is ignored).
    pub fn set(&mut self, field: PayerField, text: &str) {
        let slot = match field {
            PayerField::Name => &mut self.name,
            PayerField::Email => &mut self.email,
            PayerField::NationalId => &mut self.national_id,
            PayerField::TaxId => &mut self.tax_id,
            PayerField::Phone => &mut self.phone,
            PayerField::Address => return,
        };
        *slot = text.to_string();
    }

    /// Nothing typed at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        APP_FIELDS.iter().all(|f| self.get(*f).trim().is_empty())
    }

    /// The fields `identity` asks the app for that are empty or cannot be what they say (an email
    /// without `@` and a dot after it).
    #[must_use]
    pub fn missing(&self, identity: &Identity) -> Vec<PayerField> {
        identity
            .app_fields()
            .iter()
            .copied()
            .filter(|field| !filled(*field, self.get(*field)))
            .collect()
    }

    /// Only the fields `identity` asks the app for, trimmed; `None` when it asks for none - so
    /// nothing is sent for a method that does not ask.
    #[must_use]
    pub fn for_checkout(&self, identity: &Identity) -> Option<Payer> {
        let asked = identity.app_fields();
        if asked.is_empty() {
            return None;
        }
        let mut out = Payer::default();
        for field in asked {
            out.set(*field, self.get(*field).trim());
        }
        Some(out)
    }

    /// The JSON of `POST /v1/checkout`'s `payer`: the filled fields only.
    #[must_use]
    pub fn to_json(&self) -> Value {
        let mut map = serde_json::Map::new();
        for field in APP_FIELDS {
            let text = self.get(*field).trim();
            if !text.is_empty() {
                map.insert(field.as_str().to_string(), Value::from(text));
            }
        }
        Value::Object(map)
    }
}

/// Whether `text` fills `field`.
fn filled(field: PayerField, text: &str) -> bool {
    let text = text.trim();
    match field {
        PayerField::Email => text.split_once('@').is_some_and(|(user, host)| {
            !user.is_empty() && host.contains('.') && !host.ends_with('.')
        }),
        _ => !text.is_empty(),
    }
}

/// `fields` as an English list: `name`, `name and email address`, `name, email address and
/// national ID number`.
#[must_use]
pub fn english_list(fields: &[PayerField]) -> String {
    let words: Vec<&str> = fields.iter().map(|f| f.label()).collect();
    match words.split_last() {
        None => String::new(),
        Some((last, [])) => (*last).to_string(),
        Some((last, first)) => format!("{} and {last}", first.join(", ")),
    }
}
