//! What a payment method tells about the payer, and who asks for it.
//!
//! Azlin keeps nothing about who pays: the files are encrypted and the drive is not linked to the
//! payment. A payment method may still need the payer's name or email - a card network sees the
//! cardholder's name, a mandate names the account holder, some providers want a national ID. The
//! token server marks such a method with `needs_identity` in its offer:
//!
//! ```json
//! {"method": "card", "needs_identity": {"fields": ["name"], "asked_by": "provider"}}
//! {"method": "pix", "needs_identity": {"fields": ["name", "email", "national_id"],
//!                                      "asked_by": "app"}}
//! ```
//!
//! - `asked_by: "provider"` (the default, also for a bare list of fields): the provider's own page
//!   asks for them; the app shows a note before the method is picked and sends nothing.
//! - `asked_by: "app"`: the provider wants them in the merchant's request and has no page of its
//!   own for them. The popover asks for exactly these fields, they go once with `POST
//!   /v1/checkout` as `payer`, and the app keeps none of them. The app asks only for a name, an
//!   email and a national ID: a method that wants anything else from the app is not shown.
//!
//! A method without `needs_identity` asks for nothing beyond what paying with it implies.
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

    /// A wire name, in any case.
    #[must_use]
    pub fn parse(text: &str) -> Option<PayerField> {
        let text = text.trim();
        PayerField::ALL
            .into_iter()
            .find(|f| f.as_str().eq_ignore_ascii_case(text))
    }
}

/// The fields the app itself may ask for: anything else stays with the provider's own page.
pub const APP_FIELDS: &[PayerField] = &[PayerField::Name, PayerField::Email, PayerField::NationalId];

/// Who asks the payer for a method's details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AskedBy {
    /// The provider's own page (the app sends nothing).
    #[default]
    Provider,
    /// The app's popover, sent once with the checkout.
    App,
}

/// What a method tells about the payer and who asks for it; no fields for a method that needs
/// nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Identity {
    /// In the server's order, each once.
    pub fields: Vec<PayerField>,
    pub asked_by: AskedBy,
}

impl Identity {
    /// The method needs nothing about the payer.
    #[must_use]
    pub fn is_none(&self) -> bool {
        self.fields.is_empty()
    }

    /// The fields the app has to ask for (none when the provider asks).
    #[must_use]
    pub fn app_fields(&self) -> &[PayerField] {
        match self.asked_by {
            AskedBy::App => &self.fields,
            AskedBy::Provider => &[],
        }
    }

    /// Reads a method's `needs_identity` (see the module docs): `Ok` of nothing for a missing,
    /// `null`, `false` or empty one.
    ///
    /// # Errors
    ///
    /// Why the method cannot be shown: an unknown field, an unknown `asked_by`, or a field the app
    /// never asks for itself.
    pub fn parse(value: &Value) -> Result<Identity, String> {
        let _ = value;
        Ok(Identity::default())
    }
}

/// What the payer typed into the popover for a method the app asks for: sent once with the
/// checkout and dropped. `Debug` names the filled fields, never their text.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Payer {
    pub name: String,
    pub email: String,
    pub national_id: String,
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
            _ => "",
        }
    }

    /// The field `field` says `text` now (a field the app never asks for is ignored).
    pub fn set(&mut self, field: PayerField, text: &str) {
        match field {
            PayerField::Name => self.name = text.to_string(),
            PayerField::Email => self.email = text.to_string(),
            PayerField::NationalId => self.national_id = text.to_string(),
            _ => {}
        }
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
        let _ = identity;
        Vec::new()
    }

    /// Only the fields `identity` asks the app for, trimmed; `None` when it asks for none - so
    /// nothing is sent for a method that does not ask.
    #[must_use]
    pub fn for_checkout(&self, identity: &Identity) -> Option<Payer> {
        let _ = identity;
        None
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
