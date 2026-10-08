//! The "Add drive" dialog as data (no azul types: tested without a window). Two choices first:
//!
//! - **Buy storage**: Azlin's storage tiers from the token server (`GET /v1/tiers`), each with
//!   its size and price, monthly or yearly; on a development token server "Create test drive"
//!   (a drive without payment), everywhere "Buy" - the payment page opens in the browser and
//!   the dialog waits for the drive.
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

use azcloud_kit::{Tier, Tiers};
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
        self.testing || !matches!(self.step, BuyStep::Idle)
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
