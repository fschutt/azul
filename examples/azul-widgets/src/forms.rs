//! The two form sections.
//!
//! "Every input type" is ONE `Form` holding the widget for every HTML
//! `<input type>` - text, password, search, email, tel, url, number, range,
//! color, file, date, month, week, time, datetime-local, checkbox, radio,
//! hidden, submit, reset, image, button - plus `<select>` with `<optgroup>`s,
//! `<textarea>` and `<input list>` + `<datalist>` (a ComboBox). Submit (the
//! button, the image button, or Enter in a text field) hands the app the
//! form's `FormData`, which the section prints; Reset puts every field back
//! to the value the page started with.
//!
//! "Raw HTML inputs" writes controls as plain HTML elements instead - in Rust
//! with `Dom::create_input(..)` and friends, and as an XML snippet mounted
//! with `Dom::create_from_parsed_xml` - and the engine replaces each with the
//! same widget before styling. They sit in a `Form` too, so its Submit shows
//! what the replaced controls hand over.
//!
//! Every value of the first form is the APP's (`FormValues`): each control is
//! built from it and reports every change back into it, so a rebuild - which
//! any callback on the page may ask for - never loses an edit. The raw
//! controls need none of that: the engine remembers what the user did to them
//! across rebuilds.
//!
//! How the form reads a control: TextInput, TextArea, the date pickers and
//! HiddenInput report their live value under their `name`; a widget that
//! keeps nothing the form can read (a checkbox, a slider, a drop-down, ...)
//! carries `name` and `value` attributes on its root, and the value is the
//! app's, rebuilt on every change (`named`). A checkbox is only named while it
//! is checked: HTML leaves an unchecked checkbox out of the form data.

use azul::{
    dom::{AttributeType, SmallAriaInfo},
    image::{ImageRef, RawImage},
    option::OptionImageRef,
    prelude::*,
    str::String as AzString,
    widgets::*,
};

use crate::{captioned, section, strs, Showcase};

/// The radio group's options (`type=radio`).
const PLANS: &[&str] = &["Free", "Team", "Enterprise"];
/// The first `<optgroup>` of the select.
const FRUIT: &[&str] = &["Apple", "Cherry", "Pear"];
/// The second `<optgroup>` of the select.
const VEGETABLES: &[&str] = &["Carrot", "Leek", "Pumpkin"];
/// The `<datalist>` of the browser field (a ComboBox).
const BROWSERS: &[&str] = &["Firefox", "Chrome", "Safari", "Edge"];

/// A small note under a control or over a block.
const NOTE_CSS: &str = "font-size: 12px; color: system:secondary-text; margin: 0px;";
/// A row of controls.
const ROW_CSS: &str = "display: flex; flex-direction: row; align-items: center; gap: 8px;";
/// One of the two side-by-side columns of fields.
const COLUMN_CSS: &str =
    "display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; min-width: 0px;";
/// The two columns.
const COLUMNS_CSS: &str = "display: flex; flex-direction: row; gap: 32px;";
/// The box the submitted form data is printed in.
const OUTPUT_CSS: &str =
    "display: flex; flex-direction: column; gap: 2px; padding: 10px; border-radius: 6px; \
     border: 1px solid system:separator; background-color: system:control-background;";
/// One `name = value` line of the submitted form data.
const OUTPUT_LINE_CSS: &str = "font-family: system:monospace; font-size: 12px; color: system:text;";
/// The placeholder line of an empty output box.
const OUTPUT_EMPTY_CSS: &str = "font-size: 12px; color: system:tertiary-text;";

/// Every value the "Every input type" form shows. Each control is built from
/// here and reports back into here.
#[derive(Clone)]
pub(crate) struct FormValues {
    full_name: AzString,
    password: AzString,
    query: AzString,
    email: AzString,
    phone: AzString,
    website: AzString,
    postcode: AzString,
    notes: AzString,
    browser: AzString,
    quantity: f32,
    volume: f32,
    accent: ColorU,
    attachment: OptionString,
    day: DatePickerState,
    month: DatePickerState,
    week: DatePickerState,
    time: TimePickerState,
    meeting: DateTimeLocalPickerState,
    newsletter: bool,
    plan: usize,
    food: usize,
}

impl FormValues {
    /// What the page starts with - and what Reset restores.
    ///
    /// Every text starts EMPTY. The form's own reset can only empty the
    /// engine's copy of a text field, not type a value back into it, so an
    /// empty initial text is the one a reset shows at once and exactly.
    fn initial() -> Self {
        Self {
            full_name: "".into(),
            password: "".into(),
            query: "".into(),
            email: "".into(),
            phone: "".into(),
            website: "".into(),
            postcode: "".into(),
            notes: "".into(),
            browser: "".into(),
            quantity: 1.0,
            volume: 40.0,
            accent: ColorU {
                r: 52,
                g: 120,
                b: 246,
                a: 255,
            },
            attachment: OptionString::None,
            day: DatePickerState {
                year: 2026,
                month: 9,
                day: 29,
            },
            month: DatePickerState {
                year: 2026,
                month: 10,
                day: 1,
            },
            // Monday 28 September 2026: ISO week 2026-W40.
            week: DatePickerState {
                year: 2026,
                month: 9,
                day: 28,
            },
            time: TimePickerState {
                hour: 9,
                minute: 30,
                is_pm: false,
                is_24h: true,
            },
            meeting: DateTimeLocalPickerState {
                date: DatePickerState {
                    year: 2026,
                    month: 10,
                    day: 5,
                },
                time: TimePickerState {
                    hour: 14,
                    minute: 0,
                    is_pm: false,
                    is_24h: true,
                },
            },
            newsletter: true,
            plan: 1,
            food: 0,
        }
    }
}

/// One entry of a submitted `FormData`, as the output box prints it.
#[derive(Clone)]
struct SubmittedLine {
    /// `<form prefix>-<name>`: the line's element id, so a test can ask
    /// whether an entry was submitted (`e2e/every_input_form.json`).
    id: String,
    /// `name = value`.
    text: String,
}

/// The state of both form sections. Lives in `Showcase::form`.
#[derive(Clone)]
pub(crate) struct FormDemo {
    values: FormValues,
    /// What the last submit handed over, one line per entry.
    submitted: Vec<SubmittedLine>,
    /// One line on the last submit or reset.
    verdict: String,
    /// The same two for the raw form.
    raw_submitted: Vec<SubmittedLine>,
    raw_verdict: String,
    /// The picture on the image submit button (`<input type=image>`), made
    /// once: a new image on every rebuild would be a new texture every frame.
    send_icon: OptionImageRef,
}

impl FormDemo {
    pub(crate) fn create() -> Self {
        Self {
            values: FormValues::initial(),
            submitted: Vec::new(),
            verdict: "Nothing submitted yet.".to_string(),
            raw_submitted: Vec::new(),
            raw_verdict: "Nothing submitted yet.".to_string(),
            send_icon: send_icon(),
        }
    }
}

/// An 18 x 18 "send" arrow (a triangle pointing right) in a mid blue that
/// reads on light and dark buttons alike.
fn send_icon() -> OptionImageRef {
    const SIZE: usize = 18;
    const INK: [u8; 4] = [52, 120, 246, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];
    let middle = SIZE as f32 / 2.0;
    let mut pixels: Vec<u8> = Vec::with_capacity(SIZE * SIZE * 4);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // Full height at the left edge, narrowing to the tip at the right.
            let half_height = (SIZE as f32 - 2.0 - fx) * 0.55;
            let inside = fx > 2.0 && (fy - middle).abs() < half_height;
            pixels.extend_from_slice(if inside { &INK } else { &CLEAR });
        }
    }
    ImageRef::create_rawimage(RawImage::create_rgba8(SIZE as u32, SIZE as u32, pixels.into(), false))
}

// ---------------------------------------------------------------------------
// Building blocks
// ---------------------------------------------------------------------------

/// A field: what it is and which HTML it stands for, over the control.
fn field(label: &str, html: &str, control: Dom) -> Dom {
    captioned(&format!("{label} \u{00B7} {html}"), control)
}

/// `name` and `value` on a control's ROOT: how a form collects a control
/// that keeps nothing the form can read. The value is the app's, and the
/// control is rebuilt on every change, so the attribute is always current.
fn named(control: Dom, name: &str, value: &str) -> Dom {
    control
        .with_attribute(AttributeType::name(name))
        .with_attribute(AttributeType::value(value))
}

fn column(items: Vec<Dom>) -> Dom {
    Dom::create_div().with_css(COLUMN_CSS).with_children(items)
}

fn note(text: &str) -> Dom {
    Dom::create_p_with_text(text).with_css(NOTE_CSS)
}

/// A control with its visible label beside it (a checkbox, a radio).
fn beside(control: Dom, label: &str) -> Dom {
    Dom::create_div()
        .with_css(ROW_CSS)
        .with_child(control)
        .with_child(Dom::create_span_with_text(label).with_css("color: system:text;"))
}

/// The submitted form data, one `name = value` line each, under the verdict.
fn output(title: &str, lines: &[SubmittedLine], verdict: &str) -> Dom {
    let mut block = Dom::create_div().with_css(OUTPUT_CSS);
    if lines.is_empty() {
        block = block.with_child(
            Dom::create_span_with_text("(no form data yet)").with_css(OUTPUT_EMPTY_CSS),
        );
    }
    for line in lines {
        block = block.with_child(
            Dom::create_span_with_text(line.text.as_str())
                .with_css(OUTPUT_LINE_CSS)
                .with_id(line.id.as_str()),
        );
    }
    captioned(
        title,
        Dom::create_div()
            .with_css("display: flex; flex-direction: column; gap: 6px;")
            .with_child(note(verdict))
            .with_child(block),
    )
}

/// `FormData` as the output box prints it (each line's id is `id_prefix`, a
/// dash and the entry's name), and one line on it. A password is submitted
/// in the clear (as in HTML); the page shows one bullet per character
/// instead.
fn describe(form_data: &FormData, id_prefix: &str) -> (Vec<SubmittedLine>, String) {
    let lines: Vec<SubmittedLine> = form_data
        .entries
        .as_slice()
        .iter()
        .map(|entry| {
            let name = entry.name.as_str();
            let value = if name == "password" {
                "\u{2022}".repeat(entry.value.as_str().chars().count())
            } else {
                entry.value.as_str().to_string()
            };
            // An id is letters, digits and dashes.
            let id_name: String = name
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
                .collect();
            SubmittedLine {
                id: format!("{id_prefix}-{id_name}"),
                text: format!("{name} = {value}"),
            }
        })
        .collect();
    let invalid: Vec<&str> = form_data
        .invalid
        .as_slice()
        .iter()
        .map(|name| name.as_str())
        .collect();
    let verdict = if form_data.is_valid() {
        format!(
            "Submitted {} value(s), every one passing its constraints.",
            form_data.entries.len()
        )
    } else {
        format!(
            "Submitted {} value(s), but {} fail their constraints ({}) and are now drawn as \
             invalid. A browser would refuse to send the form - azul hands it over and lets \
             the app decide.",
            form_data.entries.len(),
            invalid.len(),
            invalid.join(", ")
        )
    };
    (lines, verdict)
}

/// The `n`-th option of the select, counting through both optgroups.
fn food_label(n: usize) -> &'static str {
    FRUIT
        .iter()
        .chain(VEGETABLES.iter())
        .nth(n)
        .copied()
        .unwrap_or("")
}

/// `#rrggbb`, as `<input type=color>` submits it (no alpha).
fn hex(c: ColorU) -> String {
    ColorU { a: 255, ..c }.to_hex().to_string()
}

// ---------------------------------------------------------------------------
// "Every input type"
// ---------------------------------------------------------------------------

/// `theme` is the page's widget theme (the toolbar's Flat / Flora).
pub(crate) fn every_input_section(data: &RefAny, demo: &FormDemo, theme: UiTheme) -> Dom {
    let v = &demo.values;

    let text_fields = column(vec![
        field(
            "Full name",
            "type=text",
            TextInput::create()
                .with_text(v.full_name.clone())
                .with_placeholder("Ada Lovelace")
                .with_name("full-name")
                .with_accessibility_name("Full name")
                .with_on_text_input(data.clone(), on_full_name)
                .with_theme(theme)
                .dom(),
        ),
        field(
            "Password",
            "type=password, pattern .{8,}",
            TextInput::create_password()
                .with_text(v.password.clone())
                .with_placeholder("At least 8 characters")
                .with_pattern(".{8,}")
                .with_name("password")
                .with_accessibility_name("Password")
                .with_on_text_input(data.clone(), on_password)
                .with_theme(theme)
                .dom(),
        ),
        field(
            "Search",
            "type=search (\u{00D7} or Escape clears it)",
            TextInput::create_search()
                .with_text(v.query.clone())
                .with_placeholder("Search the docs")
                .with_name("q")
                .with_accessibility_name("Search")
                .with_on_text_input(data.clone(), on_query)
                .with_theme(theme)
                .dom(),
        ),
        field(
            "E-mail",
            "type=email",
            TextInput::create_email()
                .with_text(v.email.clone())
                .with_placeholder("ada@example.org")
                .with_name("email")
                .with_accessibility_name("E-mail")
                .with_on_text_input(data.clone(), on_email)
                .with_theme(theme)
                .dom(),
        ),
        field(
            "Phone",
            "type=tel, pattern [0-9 +()-]{6,}",
            TextInput::create_tel()
                .with_text(v.phone.clone())
                .with_placeholder("+44 20 7946 0000")
                .with_pattern("[0-9 +()-]{6,}")
                .with_name("phone")
                .with_accessibility_name("Phone")
                .with_on_text_input(data.clone(), on_phone)
                .with_theme(theme)
                .dom(),
        ),
        field(
            "Website",
            "type=url",
            TextInput::create_url()
                .with_text(v.website.clone())
                .with_placeholder("https://azul.rs")
                .with_name("website")
                .with_accessibility_name("Website")
                .with_on_text_input(data.clone(), on_website)
                .with_theme(theme)
                .dom(),
        ),
        field(
            "Postcode",
            "type=text, pattern [0-9]{5} - type a letter for the invalid look",
            TextInput::create()
                .with_text(v.postcode.clone())
                .with_placeholder("12345")
                .with_pattern("[0-9]{5}")
                .with_name("postcode")
                .with_accessibility_name("Postcode")
                .with_on_text_input(data.clone(), on_postcode)
                .with_theme(theme)
                .dom(),
        ),
        field(
            "Notes",
            "<textarea>",
            TextArea::create()
                .with_text(v.notes.clone())
                .with_placeholder("Anything else?")
                .with_accessibility_name("Notes")
                .with_on_text_input(data.clone(), on_notes)
                .with_theme(theme)
                .dom()
                .with_attribute(AttributeType::name("notes")),
        ),
        // ComboBox has no widget theme (yet): the same look in both.
        field(
            "Browser",
            "<input list> + <datalist>",
            named(
                ComboBox::create_with_items(strs(BROWSERS))
                    .with_placeholder("Pick or type a browser")
                    .with_text(v.browser.clone())
                    .with_accessibility_name("Browser")
                    .with_on_select(data.clone(), on_browser)
                    .dom(),
                "browser",
                v.browser.as_str(),
            ),
        ),
    ]);

    let attachment = match &v.attachment {
        OptionString::Some(path) => path.as_str().to_string(),
        OptionString::None => String::new(),
    };
    let checkbox = CheckBox::create(v.newsletter)
        .with_accessibility_name("Send me the newsletter")
        .with_on_toggle(data.clone(), on_newsletter)
        .with_theme(theme)
        .dom();
    let checkbox = if v.newsletter {
        named(checkbox, "newsletter", "on")
    } else {
        checkbox
    };

    let other_fields = column(vec![
        field(
            "Quantity",
            "type=number",
            NumberInput::create(v.quantity)
                .with_accessibility_name("Quantity")
                .with_on_value_change(data.clone(), on_quantity)
                .with_theme(theme)
                .dom()
                .with_attribute(AttributeType::name("quantity")),
        ),
        field(
            "Volume",
            "type=range",
            named(
                Slider::create(v.volume, 0.0, 100.0)
                    .with_accessibility_name("Volume")
                    .with_on_value_change(data.clone(), on_volume)
                    .with_theme(theme)
                    .dom(),
                "volume",
                &format!("{:.0}", v.volume),
            ),
        ),
        field(
            "Accent",
            "type=color",
            named(
                ColorInput::create(v.accent)
                    .with_accessibility_name("Accent")
                    .with_on_value_change(data.clone(), on_accent)
                    .with_theme(theme)
                    .dom(),
                "accent",
                &hex(v.accent),
            ),
        ),
        // FileInput has no widget theme (yet): the same look in both.
        field(
            "Attachment",
            "type=file",
            named(
                FileInput::create(v.attachment.clone())
                    .with_default_text("Choose a file\u{2026}")
                    .with_on_path_change(data.clone(), on_attachment)
                    .dom(),
                "attachment",
                &attachment,
            ),
        ),
        field(
            "Day",
            "type=date",
            DatePicker::create(v.day.year, v.day.month, v.day.day)
                .with_name("day")
                .with_accessibility_name("Day")
                .with_on_change(data.clone(), on_day)
                .with_theme(theme)
                .dom(),
        ),
        field(
            "Month",
            "type=month",
            DatePicker::create_month(v.month.year, v.month.month)
                .with_name("month")
                .with_accessibility_name("Month")
                .with_on_change(data.clone(), on_month)
                .with_theme(theme)
                .dom(),
        ),
        // The picker holds the week's Monday; built from any day, a week
        // picker shows the ISO week that day is in.
        field(
            "Week",
            "type=week",
            DatePicker::create(v.week.year, v.week.month, v.week.day)
                .with_mode(DatePickerMode::Week)
                .with_name("week")
                .with_accessibility_name("Week")
                .with_on_change(data.clone(), on_week)
                .with_theme(theme)
                .dom(),
        ),
        field(
            "Time",
            "type=time",
            named(
                TimePicker::create(v.time.hour, v.time.minute)
                    .with_24h(v.time.is_24h)
                    .with_pm(v.time.is_pm)
                    .with_accessibility_name("Time")
                    .with_on_change(data.clone(), on_time)
                    .with_theme(theme)
                    .dom(),
                "time",
                &format!("{:02}:{:02}", v.time.hour, v.time.minute),
            ),
        ),
        field(
            "Meeting",
            "type=datetime-local",
            DateTimeLocalPicker::create(
                v.meeting.date.year,
                v.meeting.date.month,
                v.meeting.date.day,
                v.meeting.time.hour,
                v.meeting.time.minute,
            )
            .with_24h(true)
            .with_name("meeting")
            .with_accessibility_name("Meeting")
            .with_on_change(data.clone(), on_meeting)
            .with_theme(theme)
            .dom(),
        ),
        field(
            "Newsletter",
            "type=checkbox",
            beside(checkbox, "Send me the newsletter"),
        ),
        field(
            "Plan",
            "type=radio",
            named(
                RadioGroup::create(strs(PLANS))
                    .with_accessibility_name("Plan")
                    .with_selected_index(v.plan)
                    .with_on_change(data.clone(), on_plan)
                    .with_theme(theme)
                    .dom(),
                "plan",
                PLANS.get(v.plan).copied().unwrap_or(""),
            ),
        ),
        field(
            "Favourite food",
            "<select> with two <optgroup>s",
            named(
                DropDown::create(strs(&[]))
                    .with_optgroup("Fruit", strs(FRUIT))
                    .with_optgroup("Vegetables", strs(VEGETABLES))
                    .with_selected(v.food)
                    .with_accessibility_name("Favourite food")
                    .with_on_choice_change(data.clone(), on_food)
                    .with_theme(theme)
                    .dom(),
                "food",
                food_label(v.food),
            ),
        ),
        // Renders nothing; submitted with the form, never reset.
        field(
            "Form id",
            "type=hidden (invisible, submitted as form-id = sign-up)",
            HiddenInput::create("form-id", "sign-up")
                .with_theme(theme)
                .dom(),
        ),
    ]);

    let image_submit = match demo.send_icon.clone() {
        OptionImageRef::Some(icon) => Button::create_image(icon, "Send (image button)"),
        // No image to show: HTML shows the alt text instead.
        OptionImageRef::None => Button::create_submit("Send (image button)"),
    };
    let buttons = Dom::create_div()
        .with_css(ROW_CSS)
        .with_child(
            Button::create_submit("Submit")
                .with_theme(theme)
                .dom()
                .with_id("form-submit"),
        )
        .with_child(
            Button::create_reset("Reset")
                .with_theme(theme)
                .dom()
                .with_id("form-reset"),
        )
        .with_child(image_submit.with_theme(theme).dom())
        .with_child(
            Button::create("A plain button")
                .with_on_click(data.clone(), on_plain_button)
                .with_theme(theme)
                .dom(),
        );

    let form = Form::create(vec![
        Dom::create_div()
            .with_css(COLUMNS_CSS)
            .with_child(text_fields)
            .with_child(other_fields),
        note("submit, reset, image and button:"),
        buttons,
    ])
    .with_on_submit(data.clone(), on_form_submit)
    .with_on_reset(data.clone(), on_form_reset)
    .with_accessibility_name("Every input type")
    .with_theme(theme)
    .dom();

    section(
        "Every input type, in a Form",
        vec![
            // Lower-case "submit" / "reset" on purpose: an E2E `click` by text
            // takes the FIRST text containing it, which should be the button.
            note(
                "One Form around a widget for every HTML input type. Either submit button, or \
                 Enter in a text field, hands the app the FormData printed below. A field that \
                 fails its type or pattern is drawn as invalid once you edit it, and every such \
                 field when you submit; the reset button puts every field back.",
            ),
            form,
            output("Submitted FormData", &demo.submitted, &demo.verdict),
        ],
    )
}

// ---------------------------------------------------------------------------
// "Raw HTML inputs"
// ---------------------------------------------------------------------------

/// The XML half of the raw controls. Mounted with `Dom::create_from_parsed_xml`
/// on every layout, so what it builds goes through the same replacement as
/// the Rust half. Every control has an `id`: the engine remembers what the
/// user did to a raw control under it.
const RAW_XML: &str = "<div>\
    <p style='font-size: 12px; margin: 0px 0px 6px 0px;'>type=month</p>\
    <input id='raw-month' type='month' name='billing-month' value='2026-09' aria-label='Billing month'/>\
    <p style='font-size: 12px; margin: 12px 0px 6px 0px;'>type=week</p>\
    <input id='raw-week' type='week' name='sprint' value='2026-W40' aria-label='Sprint'/>\
    <p style='font-size: 12px; margin: 12px 0px 6px 0px;'>type=time</p>\
    <input id='raw-time' type='time' name='alarm' value='07:30' aria-label='Alarm'/>\
    <p style='font-size: 12px; margin: 12px 0px 6px 0px;'>type=datetime-local</p>\
    <input id='raw-departure' type='datetime-local' name='departure' value='2026-10-01T09:15' aria-label='Departure'/>\
    <p style='font-size: 12px; margin: 12px 0px 6px 0px;'>type=number</p>\
    <input id='raw-seats' type='number' name='seats' value='2' min='1' max='9' aria-label='Seats'/>\
    <p style='font-size: 12px; margin: 12px 0px 6px 0px;'>type=search</p>\
    <input id='raw-find' type='search' name='find' placeholder='Find' aria-label='Find'/>\
    <p style='font-size: 12px; margin: 12px 0px 6px 0px;'>type=text with list= a datalist</p>\
    <input id='raw-city' type='text' name='city' list='raw-cities' placeholder='Pick a city' aria-label='City'/>\
    <datalist id='raw-cities'><option value='Berlin'/><option value='Paris'/><option value='Rome'/></datalist>\
    <p style='font-size: 12px; margin: 12px 0px 6px 0px;'>textarea</p>\
    <textarea id='raw-remarks' name='remarks' rows='2' aria-label='Remarks'></textarea>\
    <p style='font-size: 12px; margin: 12px 0px 6px 0px;'>type=image (no src: the alt text is the label)</p>\
    <input id='raw-image' type='image' alt='Send the raw form'/>\
    <input id='raw-source' type='hidden' name='source' value='xml'/>\
</div>";

/// The Rust half of the raw controls: `Dom::create_input` and friends, with
/// attributes, exactly as an HTML page would write them.
fn rust_controls() -> Dom {
    let input = |ty: &str, name: &str, label: &str| {
        Dom::create_input(ty, name, label, SmallAriaInfo::label(label))
    };
    column(vec![
        captioned(
            "type=text",
            input("text", "nickname", "Nickname")
                .with_id("raw-nickname")
                .with_attribute(AttributeType::placeholder("Ada")),
        ),
        captioned(
            "type=email",
            input("email", "contact", "Contact e-mail")
                .with_id("raw-contact")
                .with_attribute(AttributeType::placeholder("ada@example.org")),
        ),
        captioned(
            "type=range, min 0, max 10, value 7",
            input("range", "brightness", "Brightness")
                .with_id("raw-brightness")
                .with_attribute(AttributeType::min("0"))
                .with_attribute(AttributeType::max("10"))
                .with_attribute(AttributeType::value("7")),
        ),
        captioned(
            "type=color",
            input("color", "highlight", "Highlight")
                .with_id("raw-highlight")
                .with_attribute(AttributeType::value("#e0a526")),
        ),
        captioned(
            "type=date",
            input("date", "due", "Due date")
                .with_id("raw-due")
                .with_attribute(AttributeType::value("2026-12-24")),
        ),
        captioned(
            "type=checkbox, checked",
            beside(
                input("checkbox", "terms", "Accept the terms")
                    .with_id("raw-terms")
                    .with_attribute(AttributeType::checked_true()),
                "Accept the terms",
            ),
        ),
        // One group: the engine keeps exactly one of them checked.
        captioned(
            "type=radio, one name",
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; gap: 4px;")
                .with_child(beside(
                    input("radio", "size", "Small")
                        .with_id("raw-size-s")
                        .with_attribute(AttributeType::value("s")),
                    "Small",
                ))
                .with_child(beside(
                    input("radio", "size", "Medium")
                        .with_id("raw-size-m")
                        .with_attribute(AttributeType::value("m"))
                        .with_attribute(AttributeType::checked_true()),
                    "Medium",
                ))
                .with_child(beside(
                    input("radio", "size", "Large")
                        .with_id("raw-size-l")
                        .with_attribute(AttributeType::value("l")),
                    "Large",
                )),
        ),
        captioned(
            "<select> with <optgroup>s",
            Dom::create_select("pet", "Pet", SmallAriaInfo::label("Pet"))
                .with_id("raw-pet")
                .with_child(
                    Dom::create_optgroup_no_a11y("Mammals")
                        .with_child(Dom::create_option_no_a11y("cat", "Cat"))
                        .with_child(Dom::create_option_no_a11y("dog", "Dog")),
                )
                .with_child(
                    Dom::create_optgroup_no_a11y("Birds")
                        .with_child(Dom::create_option_no_a11y("owl", "Owl"))
                        .with_child(Dom::create_option_no_a11y("wren", "Wren")),
                ),
        ),
        captioned(
            "type=submit and type=reset",
            Dom::create_div()
                .with_css(ROW_CSS)
                .with_child(
                    input("submit", "", "Send the raw form")
                        .with_id("raw-submit")
                        .with_attribute(AttributeType::value("Send raw")),
                )
                .with_child(
                    input("reset", "", "Reset the raw form")
                        .with_id("raw-reset")
                        .with_attribute(AttributeType::value("Reset raw")),
                ),
        ),
    ])
}

/// The XML half, mounted: `create_from_parsed_xml` returns a whole document
/// (`html > body > the snippet`), so the body's content is what goes in.
fn xml_controls() -> Dom {
    let xml = match Xml::from_str(RAW_XML).into_result() {
        Ok(xml) => xml,
        Err(_) => return note("(the XML snippet did not parse)"),
    };
    let document = Dom::create_from_parsed_xml(xml);
    let body_content = document
        .children
        .as_slice()
        .first()
        .map(|body| body.children.clone());
    match body_content {
        Some(children) => Dom::create_div()
            .with_css(COLUMN_CSS)
            .with_children(children),
        None => document,
    }
}

/// `theme` is the page's widget theme; it reaches the Form and the page
/// around the raw controls, not the controls themselves (see the note).
pub(crate) fn raw_inputs_section(data: &RefAny, demo: &FormDemo, theme: UiTheme) -> Dom {
    let form = Form::create(vec![Dom::create_div()
        .with_css(COLUMNS_CSS)
        .with_child(captioned(
            "Built in Rust (Dom::create_input)",
            rust_controls(),
        ))
        .with_child(captioned(
            "Parsed from XML (Dom::create_from_parsed_xml)",
            xml_controls(),
        ))])
    .with_on_submit(data.clone(), on_raw_submit)
    .with_on_reset(data.clone(), on_raw_reset)
    .with_accessibility_name("Raw HTML inputs")
    .with_theme(theme)
    .dom();

    section(
        "Raw HTML inputs",
        vec![
            note(
                "The same controls written as plain HTML: <input type=..>, <select>, \
                 <textarea>, <datalist>. Before styling, the engine replaces each with the \
                 widget of its type, keeps what the user does to it across rebuilds, and \
                 hands its value to the enclosing Form. A raw <form> becomes a Form the same \
                 way. Opt a node out with data-azul-widget=\"none\". The replacement has no \
                 theme input yet, so these are always built flat.",
            ),
            form,
            output("Submitted FormData", &demo.raw_submitted, &demo.raw_verdict),
        ],
    )
}

// ---------------------------------------------------------------------------
// Callbacks
// ---------------------------------------------------------------------------

/// A text field's hook: keep the text, no rebuild (the field already shows
/// it).
fn keep_text(
    data: &mut RefAny,
    text: AzString,
    put: fn(&mut FormValues, AzString),
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        put(&mut s.form.values, text);
        s.interactions += 1;
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// Every other control: keep the value and rebuild, which also brings the
/// control's `value` attribute up to date.
fn keep(data: &mut RefAny, put: impl FnOnce(&mut FormValues)) -> Update {
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            put(&mut s.form.values);
            s.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

extern "C" fn on_full_name(
    mut data: RefAny,
    _: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    keep_text(&mut data, state.get_text(), |v, t| v.full_name = t)
}
extern "C" fn on_password(
    mut data: RefAny,
    _: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    keep_text(&mut data, state.get_text(), |v, t| v.password = t)
}
extern "C" fn on_query(
    mut data: RefAny,
    _: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    keep_text(&mut data, state.get_text(), |v, t| v.query = t)
}
extern "C" fn on_email(
    mut data: RefAny,
    _: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    keep_text(&mut data, state.get_text(), |v, t| v.email = t)
}
extern "C" fn on_phone(
    mut data: RefAny,
    _: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    keep_text(&mut data, state.get_text(), |v, t| v.phone = t)
}
extern "C" fn on_website(
    mut data: RefAny,
    _: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    keep_text(&mut data, state.get_text(), |v, t| v.website = t)
}
extern "C" fn on_postcode(
    mut data: RefAny,
    _: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    keep_text(&mut data, state.get_text(), |v, t| v.postcode = t)
}
extern "C" fn on_notes(
    mut data: RefAny,
    _: CallbackInfo,
    state: TextAreaState,
) -> OnTextInputReturn {
    keep_text(&mut data, state.get_text(), |v, t| v.notes = t)
}
extern "C" fn on_browser(mut data: RefAny, _: CallbackInfo, state: ComboBoxState) -> Update {
    keep(&mut data, |v| v.browser = state.text)
}
/// No rebuild while typing: the form reads the number field's live text, and
/// a rebuild mid-edit would rewrite the text from the last parsed number.
extern "C" fn on_quantity(mut data: RefAny, _: CallbackInfo, state: NumberInputState) -> Update {
    if let Some(mut s) = data.downcast_mut::<Showcase>() {
        s.form.values.quantity = state.number;
        s.interactions += 1;
    }
    Update::DoNothing
}
extern "C" fn on_volume(mut data: RefAny, _: CallbackInfo, state: SliderState) -> Update {
    keep(&mut data, |v| v.volume = state.value)
}
extern "C" fn on_accent(mut data: RefAny, _: CallbackInfo, state: ColorInputState) -> Update {
    keep(&mut data, |v| v.accent = state.color)
}
extern "C" fn on_attachment(mut data: RefAny, _: CallbackInfo, state: FileInputState) -> Update {
    keep(&mut data, |v| v.attachment = state.path)
}
extern "C" fn on_day(mut data: RefAny, _: CallbackInfo, state: DatePickerState) -> Update {
    keep(&mut data, |v| v.day = state)
}
extern "C" fn on_month(mut data: RefAny, _: CallbackInfo, state: DatePickerState) -> Update {
    keep(&mut data, |v| v.month = state)
}
extern "C" fn on_week(mut data: RefAny, _: CallbackInfo, state: DatePickerState) -> Update {
    keep(&mut data, |v| v.week = state)
}
extern "C" fn on_time(mut data: RefAny, _: CallbackInfo, state: TimePickerState) -> Update {
    keep(&mut data, |v| v.time = state)
}
extern "C" fn on_meeting(
    mut data: RefAny,
    _: CallbackInfo,
    state: DateTimeLocalPickerState,
) -> Update {
    keep(&mut data, |v| v.meeting = state)
}
extern "C" fn on_newsletter(mut data: RefAny, _: CallbackInfo, state: CheckBoxState) -> Update {
    keep(&mut data, |v| v.newsletter = state.checked)
}
extern "C" fn on_plan(mut data: RefAny, _: CallbackInfo, state: RadioGroupState) -> Update {
    keep(&mut data, |v| v.plan = state.selected_index)
}
extern "C" fn on_food(mut data: RefAny, _: CallbackInfo, choice: usize) -> Update {
    keep(&mut data, |v| v.food = choice)
}

/// Sets the verdict line of the first form and asks for a rebuild.
fn set_verdict(data: &mut RefAny, verdict: &str) -> Update {
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            s.form.verdict = verdict.to_string();
            s.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

extern "C" fn on_plain_button(mut data: RefAny, _: CallbackInfo) -> Update {
    set_verdict(
        &mut data,
        "type=button: a plain button inside the form - it neither submits nor resets it.",
    )
}

extern "C" fn on_form_submit(mut data: RefAny, _: CallbackInfo, form_data: FormData) -> Update {
    let (lines, verdict) = describe(&form_data, "form-data");
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            s.form.submitted = lines;
            s.form.verdict = verdict;
            s.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

/// Everything the user TYPED is still held by the engine until the app's
/// rebuilt page carries it - an edit the app has not acknowledged outranks a
/// rebuilt field that says otherwise. Acknowledge every edit, so the rebuild
/// that follows (from the initial values) is what every field shows.
///
/// The ack is for the whole window, so every other text field must be
/// rebuilt with its text: the page's TextInputs and TextAreas hand theirs
/// back, and the engine's form memory holds what was typed into the raw
/// fields. (Text typed into a ComboBox but never picked is the exception: it
/// falls back to the last pick.)
fn ack_typed_text(info: &mut CallbackInfo) {
    let revision = info.get_document_text_revision();
    info.mark_text_revision_synced(revision);
}

/// The form's own reset has already emptied its text fields; this puts every
/// value back to where the page started (the controls are rebuilt from them).
extern "C" fn on_form_reset(
    mut data: RefAny,
    mut info: CallbackInfo,
    _initial: FormData,
) -> Update {
    ack_typed_text(&mut info);
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            s.form.values = FormValues::initial();
            s.form.submitted = Vec::new();
            s.form.verdict =
                "Reset: every field is back at the value the page started with.".to_string();
            s.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

extern "C" fn on_raw_submit(mut data: RefAny, _: CallbackInfo, form_data: FormData) -> Update {
    let (lines, verdict) = describe(&form_data, "raw-form-data");
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            s.form.raw_submitted = lines;
            s.form.raw_verdict = verdict;
            s.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}

/// The engine resets the raw controls itself (it forgets what the user did
/// to them and rebuilds them from their HTML defaults).
extern "C" fn on_raw_reset(mut data: RefAny, mut info: CallbackInfo, _initial: FormData) -> Update {
    ack_typed_text(&mut info);
    match data.downcast_mut::<Showcase>() {
        Some(mut s) => {
            s.form.raw_submitted = Vec::new();
            s.form.raw_verdict =
                "Reset: every raw control is back at its HTML default.".to_string();
            s.interactions += 1;
            Update::RefreshDom
        }
        None => Update::DoNothing,
    }
}
