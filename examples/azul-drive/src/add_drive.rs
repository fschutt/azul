//! The "Add drive" dialog as data (no azul types: tested without a window). Two choices first:
//!
//! - **Buy storage**: Azlin's storage tiers from the token server (`GET /v1/tiers`), each with
//!   its size and price, monthly or yearly; on a development token server "Create test drive"
//!   (a drive without payment), everywhere "Buy". With the token server's payment options
//!   (`GET /v1/checkout/options`, narrowed by azul-pay's registry) Buy storage shows a pill per
//!   payment method for the payer's country and period, the consent the order needs, and Buy
//!   runs azul-pay's checkout machine: the provider's hosted fields in a popover, its hosted
//!   page, or the system browser. Without them (an older token server) the v1 checkout's
//!   payment page opens in the browser. Either way the dialog waits for the drive (the claim).
//! - **Connect data source**: the sources of azul-storage's catalog this build can open, in
//!   their groups (S3-compatible storage and a folder on this computer always; OpenDAL's
//!   services with the feature `opendal`; databases as tables with `sql`), then the chosen
//!   source's form - generated from its fields - with "Test connection" and "Add drive".
//!
//! The dialog's view is `ui_add_drive`; what its buttons start is in `actions`. `Debug` never
//! shows a typed secret.

use std::{
    fmt,
    sync::{atomic::AtomicBool, Arc},
};

use azcloud_kit::{PendingCheckout, Tier, Tiers};
use azul_pay::{
    offer::Offer,
    pills::{self, Choice, Pill, PillContext},
    registry::{Method, SurfaceKind},
    surface::Look,
    State as PayState,
};
use azul_storage::{
    catalog::{self, FieldKind, FormValues, NewDrive, ServiceGroup, ServiceSpec},
    config::DriveEntry,
};

/// The name a bought drive gets unless the user types another.
pub(crate) const DEFAULT_CLOUD_NAME: &str = "Azlin Storage";

/// The dialog's pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AddPage {
    /// Buy storage or Connect data source.
    Choose,
    /// The tiers and their prices.
    Buy,
    /// The sources, in their groups.
    Sources,
    /// One source's form.
    Form,
}

/// The tier list of Buy storage.
#[derive(Clone, Debug)]
pub(crate) enum TiersState {
    /// Not asked for yet.
    NotLoaded,
    /// Asked for; the answer is on its way.
    Loading,
    Loaded(Tiers),
    /// Why there is none.
    Failed(String),
}

/// What Buy storage is doing.
#[derive(Clone, Debug)]
pub(crate) enum BuyStep {
    Idle,
    /// A development server signs up a test drive.
    Creating,
    /// The checkout is being made.
    StartingCheckout,
    /// The payment page is open in the browser; the dialog polls the checkout until the drive
    /// is there (`cancel` stops the polling).
    Paying {
        checkout_id: String,
        cancel: Arc<AtomicBool>,
    },
}

/// The dialog.
pub(crate) struct AddDialog {
    pub page: AddPage,
    /// Which opening of a dialog this is: a job's answer for an older one is dropped.
    pub serial: u64,

    // ---- Connect data source ----
    /// The chosen source (a catalog id).
    pub service: Option<&'static str>,
    /// The drive's name.
    pub name: String,
    /// The form's values by field key (the secrets too, while typed).
    pub values: FormValues,
    /// The drive whose keys are typed in again (its keyring entry is gone).
    pub editing: Option<String>,
    /// A connection test is running.
    pub testing: bool,
    /// The last test's sentence: `Ok` it worked, `Err` why not.
    pub tested: Option<Result<String, String>>,
    /// What to fix before the drive can be added (shown under the form).
    pub error: String,

    // ---- Buy storage ----
    pub tiers: TiersState,
    /// The chosen tier, an index of the list.
    pub tier: usize,
    /// Paid yearly (else monthly).
    pub yearly: bool,
    /// The bought drive's name.
    pub buy_name: String,
    pub step: BuyStep,
    /// Buy storage's status line: what it is doing, or why it stopped.
    pub notice: String,

    // ---- Buy storage's payment (azul-pay) ----
    /// The token server's payment options.
    pub offer: OfferState,
    /// The pill chosen (by its method); `None`: the default pill.
    pub pill: Option<Method>,
    /// The provider chosen in a pill of two (an index of the offer's providers).
    pub pill_provider: Option<usize>,
    /// The payer's country (ISO alpha-2): which pills show, the VAT country.
    pub country: String,
    /// The consent the order needs is ticked.
    pub consent: bool,
    /// The checkout's state machine.
    pub pay: PayState,
    /// The cardholder name typed in the popover (personal data: never printed).
    pub card_name: String,
    /// The checkout the machine runs, as the keyring's list keeps it (what the poll asks with).
    pub kept: Option<PendingCheckout>,
    /// The look the fields page should take (`flora-light`, `flat-dark`).
    pub look_name: String,
}

/// The surfaces AzDrive can show: the popover's hosted fields, a hosted page in the web view,
/// the system browser (no native sheet, no native IBAN field yet).
pub(crate) const PAY_SURFACES: &[SurfaceKind] = &[
    SurfaceKind::PopoverFields,
    SurfaceKind::WebviewPage,
    SurfaceKind::SystemBrowser,
];

/// The countries of Buy storage's "Country" choice (ISO alpha-2, the name shown).
pub(crate) const COUNTRIES: &[(&str, &str)] = &[
    ("DE", "Germany"),
    ("AT", "Austria"),
    ("CH", "Switzerland"),
    ("FR", "France"),
    ("NL", "Netherlands"),
    ("BE", "Belgium"),
    ("LU", "Luxembourg"),
    ("IT", "Italy"),
    ("ES", "Spain"),
    ("PL", "Poland"),
    ("SE", "Sweden"),
    ("GB", "United Kingdom"),
    ("US", "United States"),
];

/// The country of a locale (`de_DE.UTF-8`, `en-GB`, `nl_NL@euro`) when it is one of
/// [`COUNTRIES`].
#[must_use]
pub(crate) fn country_of_locale(locale: &str) -> Option<String> {
    let base = locale.split(['.', '@']).next().unwrap_or_default();
    let (_, region) = base.split_once(['_', '-'])?;
    let region = region.to_ascii_uppercase();
    COUNTRIES
        .iter()
        .any(|(code, _)| *code == region)
        .then_some(region)
}

/// The payer's country this run starts with ([`country_from`] the environment).
#[must_use]
pub(crate) fn default_country() -> String {
    country_from(|name| std::env::var(name).ok())
}

/// The payer's country from the environment `var`: `AZLIN_COUNTRY` (a code of [`COUNTRIES`],
/// any case), else the locale's (`LC_ALL`, `LC_MESSAGES`, `LANG`), else Germany.
#[must_use]
pub(crate) fn country_from(var: impl Fn(&str) -> Option<String>) -> String {
    let named = var("AZLIN_COUNTRY")
        .map(|code| code.trim().to_ascii_uppercase())
        .filter(|code| COUNTRIES.iter().any(|(c, _)| c == code));
    named
        .or_else(|| {
            ["LC_ALL", "LC_MESSAGES", "LANG"]
                .iter()
                .filter_map(|name| var(*name))
                .find_map(|locale| country_of_locale(&locale))
        })
        .unwrap_or_else(|| String::from("DE"))
}

/// The token server's payment options for Buy storage.
#[derive(Clone, Debug)]
pub(crate) enum OfferState {
    NotLoaded,
    Loading,
    /// The token server has none (an older one): the v1 checkout, a payment page in the
    /// browser.
    Legacy,
    Loaded(Offer),
    /// Why there are none (Buy storage falls back to the v1 checkout).
    Failed(String),
}

impl fmt::Debug for AddDialog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let secrets: Vec<&str> = self
            .spec()
            .map(|s| s.secret_keys().collect())
            .unwrap_or_default();
        let values: Vec<(&str, &str)> = self
            .values
            .iter()
            .map(|(k, v)| {
                let shown = if secrets.contains(&k.as_str()) {
                    "<hidden>"
                } else {
                    v.as_str()
                };
                (k.as_str(), shown)
            })
            .collect();
        f.debug_struct("AddDialog")
            .field("page", &self.page)
            .field("serial", &self.serial)
            .field("service", &self.service)
            .field("name", &self.name)
            .field("values", &values)
            .field("editing", &self.editing)
            .field("testing", &self.testing)
            .field("tested", &self.tested)
            .field("error", &self.error)
            .field("tiers", &self.tiers)
            .field("tier", &self.tier)
            .field("yearly", &self.yearly)
            .field("step", &self.step)
            .field("offer", &self.offer)
            .field("pill", &self.pill)
            .field("pill_provider", &self.pill_provider)
            .field("country", &self.country)
            .field("consent", &self.consent)
            .field("pay", &self.pay)
            .field("card_name", &"<hidden>")
            .finish_non_exhaustive()
    }
}

impl AddDialog {
    /// A new dialog on its two choices.
    #[must_use]
    pub(crate) fn new(serial: u64) -> Self {
        AddDialog {
            page: AddPage::Choose,
            serial,
            service: None,
            name: String::new(),
            values: FormValues::new(),
            editing: None,
            testing: false,
            tested: None,
            error: String::new(),
            tiers: TiersState::NotLoaded,
            tier: 0,
            yearly: false,
            buy_name: DEFAULT_CLOUD_NAME.to_string(),
            step: BuyStep::Idle,
            notice: String::new(),
            offer: OfferState::NotLoaded,
            pill: None,
            pill_provider: None,
            country: default_country(),
            consent: false,
            pay: PayState::Choosing,
            card_name: String::new(),
            kept: None,
            look_name: String::from("flat-light"),
        }
    }

    /// The form of the drive `entry` again (its keyring entry is gone): its settings filled
    /// in, its secrets to type anew.
    #[must_use]
    pub(crate) fn editing(entry: &DriveEntry, serial: u64) -> Self {
        let mut dialog = AddDialog::new(serial);
        let (spec, values) = catalog::form_values(entry);
        if let Some(spec) = spec {
            let mut filled = spec.defaults();
            filled.extend(values);
            dialog.service = Some(spec.id);
            dialog.values = filled;
            dialog.page = AddPage::Form;
        }
        dialog.name = entry.name.clone();
        dialog.editing = Some(entry.id.clone());
        dialog
    }

    /// The chosen source.
    #[must_use]
    pub(crate) fn spec(&self) -> Option<&'static ServiceSpec> {
        self.service.and_then(catalog::service)
    }

    /// Buy storage.
    pub(crate) fn choose_buy(&mut self) {
        self.page = AddPage::Buy;
        self.notice.clear();
    }

    /// Connect data source.
    pub(crate) fn choose_connect(&mut self) {
        self.page = AddPage::Sources;
    }

    /// The form of the source `id` at its defaults; `false` (and nothing changes) when this
    /// build cannot open it.
    pub(crate) fn open_service(&mut self, id: &str) -> bool {
        let Some(spec) = catalog::service(id).filter(|s| s.available()) else {
            return false;
        };
        self.service = Some(spec.id);
        self.values = spec.defaults();
        self.name = spec.name.to_string();
        self.testing = false;
        self.tested = None;
        self.error.clear();
        self.page = AddPage::Form;
        true
    }

    /// One page back: a form to the sources, the sources and Buy storage to the choices.
    pub(crate) fn back(&mut self) {
        self.page = match self.page {
            AddPage::Form if self.editing.is_none() => AddPage::Sources,
            AddPage::Form => AddPage::Form,
            AddPage::Sources | AddPage::Buy | AddPage::Choose => AddPage::Choose,
        };
        self.error.clear();
    }

    /// A changed form is not the one tested, and its error is fixed or another.
    fn changed(&mut self) {
        self.tested = None;
        self.error.clear();
    }

    /// Field `key` now says `text`.
    pub(crate) fn set_value(&mut self, key: &str, text: &str) {
        self.values.insert(key.to_string(), text.to_string());
        self.changed();
    }

    /// The drive's name.
    pub(crate) fn set_name(&mut self, text: &str) {
        self.name = text.to_string();
        self.changed();
    }

    /// Flips the switch `key` (a field of kind Bool).
    pub(crate) fn toggle(&mut self, key: &str) {
        let on = !self.bool_value(key);
        self.set_value(key, if on { "true" } else { "false" });
    }

    /// The choice `key` takes its word number `index` (no such word: unchanged).
    pub(crate) fn choose(&mut self, key: &str, index: usize) {
        let word = self.spec().and_then(|s| s.field(key)).and_then(|f| match f.kind {
            FieldKind::Choice(words) => words.get(index).copied(),
            _ => None,
        });
        if let Some(word) = word {
            self.set_value(key, word);
        }
    }

    /// What field `key` says (`""` for nothing).
    #[must_use]
    pub(crate) fn value(&self, key: &str) -> &str {
        self.values.get(key).map_or("", String::as_str)
    }

    /// Whether the switch `key` is on.
    #[must_use]
    pub(crate) fn bool_value(&self, key: &str) -> bool {
        self.value(key) == "true"
    }

    /// The form as it is: what to fix first, as a sentence.
    pub(crate) fn check(&self) -> Result<(), String> {
        let spec = self
            .spec()
            .ok_or_else(|| String::from("Choose a source first."))?;
        catalog::check(spec, &self.name, &self.values)
    }

    /// The drive the form describes, with the id `id`.
    pub(crate) fn build(&self, id: &str) -> Result<NewDrive, String> {
        let spec = self
            .spec()
            .ok_or_else(|| String::from("Choose a source first."))?;
        catalog::build_entry(spec, id, &self.name, &self.values)
    }

    /// The page for scripts: `choose`, `buy`, `sources`, `form <source>`.
    #[must_use]
    pub(crate) fn page_line(&self) -> String {
        match self.page {
            AddPage::Choose => String::from("choose"),
            AddPage::Buy => String::from("buy"),
            AddPage::Sources => String::from("sources"),
            AddPage::Form => format!("form {}", self.service.unwrap_or("-")),
        }
    }

    /// The chosen tier, once the list is in.
    #[must_use]
    pub(crate) fn chosen_tier(&self) -> Option<&Tier> {
        match &self.tiers {
            TiersState::Loaded(tiers) => tiers.tiers.get(self.tier),
            _ => None,
        }
    }

    /// Months paid at once: 12 yearly, else 1.
    #[must_use]
    pub(crate) fn months(&self) -> u32 {
        if self.yearly {
            12
        } else {
            1
        }
    }

    /// The Buy button's words: `Buy 100 GB - EUR 0.99 a month`.
    #[must_use]
    pub(crate) fn buy_label(&self) -> String {
        match self.chosen_tier() {
            Some(tier) => match tier.price_text(self.yearly) {
                Some(price) => format!("Buy {} - {price}", tier.quota_text()),
                None => format!("Buy {}", tier.quota_text()),
            },
            None => String::from("Buy"),
        }
    }

    /// The payment page is open and the dialog waits for the drive.
    #[must_use]
    pub(crate) fn paying(&self) -> bool {
        matches!(self.step, BuyStep::Paying { .. })
    }

    /// Something runs that the dialog waits for (a test, a sign-up, a checkout, a payment).
    #[must_use]
    pub(crate) fn busy(&self) -> bool {
        self.testing || !matches!(self.step, BuyStep::Idle) || self.pay.busy()
    }

    // ---- The payment (azul-pay) ----

    /// The offer, once it is in.
    #[must_use]
    pub(crate) fn offer(&self) -> Option<&Offer> {
        match &self.offer {
            OfferState::Loaded(offer) => Some(offer),
            _ => None,
        }
    }

    /// The token server's offer is in: the default pill is chosen.
    pub(crate) fn offer_loaded(&mut self, offer: Offer) {
        self.offer = OfferState::Loaded(offer);
        self.pill = None;
        self.pill_provider = None;
    }

    /// Who pays what: this payer's country, the tier's currency, the months paid at once, what
    /// AzDrive can show.
    #[must_use]
    pub(crate) fn pill_context(&self) -> PillContext<'_> {
        PillContext {
            country: &self.country,
            currency: self.chosen_tier().map_or("EUR", |t| t.currency.as_str()),
            months: self.months(),
            recurring: false,
            surfaces: PAY_SURFACES,
        }
    }

    /// The pills to show (none without an offer).
    #[must_use]
    pub(crate) fn pills(&self) -> Vec<Pill> {
        self.offer()
            .map(|offer| pills::pills(offer, &self.pill_context()))
            .unwrap_or_default()
    }

    /// Buy storage pays through the offer's pills (else on the v1 payment page).
    #[must_use]
    pub(crate) fn pays_with_pills(&self) -> bool {
        !self.pills().is_empty()
    }

    /// The pill chosen - the default one when none is, or the chosen one went away - with the
    /// provider switched to in it.
    #[must_use]
    pub(crate) fn chosen_pill(&self) -> Option<Pill> {
        let shown = self.pills();
        let pill = self
            .pill
            .and_then(|m| shown.iter().find(|p| p.method == m))
            .or_else(|| shown.get(pills::default_pill(&shown)))?;
        Some(match self.pill_provider {
            Some(index) => pill.with_provider(index),
            None => pill.clone(),
        })
    }

    /// A pill was clicked: its method is chosen, with its default provider.
    pub(crate) fn choose_pill(&mut self, method: Method) {
        self.pill = Some(method);
        self.pill_provider = None;
        self.notice.clear();
    }

    /// The pill's other provider was chosen (an index of the offer's providers).
    pub(crate) fn choose_provider(&mut self, index: usize) {
        self.pill = self.chosen_pill().map(|p| p.method);
        self.pill_provider = Some(index);
    }

    /// What the order button starts: the chosen pill's provider and method.
    #[must_use]
    pub(crate) fn choice(&self) -> Option<Choice> {
        let offer = self.offer()?;
        Choice::of(offer, &self.chosen_pill()?, &self.pill_context())
    }

    /// The payer's country is `code` now: whether it changed (a code of [`COUNTRIES`] only).
    /// The options are asked for again.
    pub(crate) fn set_country(&mut self, code: &str) -> bool {
        let code = code.trim().to_ascii_uppercase();
        if code == self.country || !COUNTRIES.iter().any(|(c, _)| *c == code) {
            return false;
        }
        self.country = code;
        self.offer = OfferState::NotLoaded;
        self.pill = None;
        self.pill_provider = None;
        true
    }

    /// For scripts: each pill's method and chosen provider, `sepa_debit:gocardless card:stripe`.
    #[must_use]
    pub(crate) fn pills_line(&self) -> String {
        let Some(offer) = self.offer() else {
            return String::new();
        };
        self.pills()
            .iter()
            .map(|p| format!("{}:{}", p.method.as_str(), p.provider(offer).spec.id))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// How the fields page should look.
    #[must_use]
    pub(crate) fn look(&self) -> Look {
        Look {
            locale: String::from("en"),
            look: self.look_name.clone(),
        }
    }
}

/// The sources the Sources page lists: every group that has a source this build can open, with
/// those sources in the catalog's order; and how many sources this build cannot open.
#[must_use]
pub(crate) fn source_groups() -> (Vec<(ServiceGroup, Vec<&'static ServiceSpec>)>, usize) {
    let mut groups = Vec::new();
    let mut unavailable = 0;
    for group in ServiceGroup::ALL {
        let (open, closed): (Vec<&'static ServiceSpec>, Vec<&'static ServiceSpec>) =
            catalog::services_in(group).partition(|s| s.available());
        unavailable += closed.len();
        if !open.is_empty() {
            groups.push((group, open));
        }
    }
    (groups, unavailable)
}
