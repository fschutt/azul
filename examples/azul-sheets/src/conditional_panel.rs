//! HOME > Conditional Formatting: the side panel that adds a rule to the
//! selection (Excel's Highlight Cells Rules and the average rules, in its
//! three looks), clears the rules of the selection and lists the sheet's
//! rules. IronCalc evaluates them; the grid shows the result through the
//! cells' styles.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, DropDownOnChoiceChangeCallbackType, TextInputOnTextInputCallbackType,
    },
    prelude::*,
    str::String as AzString,
    vec::StringVec,
    widgets::{Button, ButtonType, DropDown, OnTextInputReturn, TextInput, TextInputState},
};

use crate::{
    a1_area,
    engine::{CondLook, CondRule},
    ids, on_panel_close, run, state_text, text_return, with_app,
    worker::Command,
    AppState,
};

/// The rules offered, in the panel's order.
pub const KINDS: [&str; 8] = [
    "Greater than",
    "Less than",
    "Between",
    "Equal to",
    "Text that contains",
    "Duplicate values",
    "Above average",
    "Below average",
];

/// The panel's choices while it is open.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CondDraft {
    /// An index into [`KINDS`].
    pub kind: usize,
    pub value1: String,
    pub value2: String,
    /// An index into [`CondLook::ALL`].
    pub look: usize,
}

impl CondDraft {
    /// How many values the rule takes (0, 1 or 2).
    #[must_use]
    pub const fn values(&self) -> usize {
        match self.kind {
            2 => 2,
            0 | 1 | 3 | 4 => 1,
            _ => 0,
        }
    }

    /// The rule the choices make, or what is missing.
    pub fn rule(&self) -> Result<CondRule, &'static str> {
        let (a, b) = (self.value1.trim().to_string(), self.value2.trim().to_string());
        if self.values() >= 1 && a.is_empty() {
            return Err("Enter a value for the rule.");
        }
        if self.values() == 2 && b.is_empty() {
            return Err("Enter both values of the rule.");
        }
        Ok(match self.kind {
            0 => CondRule::GreaterThan(a),
            1 => CondRule::LessThan(a),
            2 => CondRule::Between(a, b),
            3 => CondRule::EqualTo(a),
            4 => CondRule::TextContains(a),
            5 => CondRule::Duplicates,
            6 => CondRule::AboveAverage,
            _ => CondRule::BelowAverage,
        })
    }

    /// The look picked.
    #[must_use]
    pub fn look(&self) -> CondLook {
        CondLook::ALL.get(self.look).copied().unwrap_or_default()
    }
}

/// What a control of the panel sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    Kind,
    Look,
    Value1,
    Value2,
}

struct CondRef {
    app: RefAny,
    which: Which,
}

fn cond_ref(app: &RefAny, which: Which) -> RefAny {
    RefAny::new(CondRef { app: app.clone(), which })
}

fn strs<'a>(items: impl IntoIterator<Item = &'a str>) -> StringVec {
    StringVec::from_vec(items.into_iter().map(AzString::from).collect())
}

fn label(text: &str) -> Dom {
    Dom::create_p_with_text(AzString::from(text)).with_css("font-size: 12px; font-weight: 600; margin: 8px 0px 3px 0px;")
}

fn line(text: &str) -> Dom {
    Dom::create_p_with_text(AzString::from(text)).with_css("font-size: 12px; margin: 2px 0px;")
}

fn value_field(app: &RefAny, which: Which, text: &str, name: &str) -> Dom {
    TextInput::create()
        .with_text(AzString::from(text))
        .with_placeholder(AzString::from(name))
        .with_on_text_input(cond_ref(app, which), on_value as TextInputOnTextInputCallbackType)
        .with_accessibility_name(AzString::from(name))
        .dom()
}

/// The panel.
pub(crate) fn panel(s: &AppState, app: &RefAny) -> Dom {
    let d = &s.cond;
    let mut p = Dom::create_div()
        .with_id(ids::SIDE_PANEL)
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 0; width: 280px; padding: 8px; \
             border-left: 1px solid rgba(128, 128, 128, 0.35); overflow-y: auto;",
        )
        .with_child(
            Dom::create_p_with_text(AzString::from("Conditional Formatting"))
                .with_css("font-weight: 600; margin: 4px 0px 8px 0px;"),
        )
        .with_child(line(&format!("Format the cells of {} that match:", a1_area(s.current_area()))))
        .with_child(label("Rule"))
        .with_child(
            DropDown::create(strs(KINDS))
                .with_selected(d.kind)
                .with_accessibility_name("Rule")
                .with_on_choice_change(cond_ref(app, Which::Kind), on_choice as DropDownOnChoiceChangeCallbackType)
                .dom(),
        );
    if d.values() >= 1 {
        p.add_child(value_field(app, Which::Value1, &d.value1, "Value"));
    }
    if d.values() == 2 {
        p.add_child(line("and"));
        p.add_child(value_field(app, Which::Value2, &d.value2, "Second value"));
    }
    p.add_child(label("Format with"));
    p.add_child(
        DropDown::create(strs(CondLook::ALL.iter().map(|l| l.label())))
            .with_selected(d.look)
            .with_accessibility_name("Format with")
            .with_on_choice_change(cond_ref(app, Which::Look), on_choice as DropDownOnChoiceChangeCallbackType)
            .dom(),
    );
    p.add_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; margin-top: 10px;")
            .with_child(
                Button::create(AzString::from("Apply"))
                    .with_button_type(ButtonType::Primary)
                    .with_on_click(app.clone(), on_apply as ButtonOnClickCallbackType)
                    .dom(),
            )
            .with_child(Dom::create_div().with_css("width: 8px;"))
            .with_child(
                Button::create(AzString::from("Clear Rules"))
                    .with_on_click(app.clone(), on_clear as ButtonOnClickCallbackType)
                    .dom(),
            ),
    );
    p.add_child(label("Rules on this sheet"));
    if s.cache.snapshot.conditional.is_empty() {
        p.add_child(line("None yet."));
    }
    for c in &s.cache.snapshot.conditional {
        p.add_child(line(&format!("{}: {}", a1_area(c.area), c.description)));
    }
    p.with_child(
        Button::create(AzString::from("Close"))
            .with_on_click(app.clone(), on_panel_close as ButtonOnClickCallbackType)
            .dom()
            .with_css("flex-grow: 0; margin-top: 8px;"),
    )
}

extern "C" fn on_choice(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some((mut app, which)) = data.downcast_ref::<CondRef>().map(|r| (r.app.clone(), r.which)) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |_, _, s| match which {
        Which::Kind => s.cond.kind = index.min(KINDS.len() - 1),
        Which::Look => s.cond.look = index.min(CondLook::ALL.len() - 1),
        Which::Value1 | Which::Value2 => {}
    })
}

/// A value typed: kept, no rebuild (the field shows its own text).
extern "C" fn on_value(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let Some((mut app, which)) = data.downcast_ref::<CondRef>().map(|r| (r.app.clone(), r.which)) else {
        return text_return(Update::DoNothing);
    };
    if let Some(mut s) = app.downcast_mut::<AppState>() {
        let text = state_text(&state);
        match which {
            Which::Value1 => s.cond.value1 = text,
            Which::Value2 => s.cond.value2 = text,
            Which::Kind | Which::Look => {}
        }
    }
    text_return(Update::DoNothing)
}

extern "C" fn on_apply(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |info, app, s| match s.cond.rule() {
        Ok(rule) => {
            let area = s.current_area();
            let look = s.cond.look();
            s.message = format!("{} on {}.", rule.describe(), a1_area(area));
            run(info, app, s, Command::AddConditional { area, rule, look });
        }
        Err(why) => s.message = why.to_string(),
    })
}

extern "C" fn on_clear(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |info, app, s| {
        let area = s.current_area();
        run(info, app, s, Command::ClearConditional { area });
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_choices_make_the_rule_and_say_what_is_missing() {
        let mut d = CondDraft::default();
        assert_eq!(d.rule(), Err("Enter a value for the rule."));
        d.value1 = String::from(" 5 ");
        assert_eq!(d.rule(), Ok(CondRule::GreaterThan(String::from("5"))));
        d.kind = 2;
        assert_eq!(d.rule(), Err("Enter both values of the rule."));
        d.value2 = String::from("9");
        assert_eq!(d.rule(), Ok(CondRule::Between(String::from("5"), String::from("9"))));
        d.kind = 5;
        assert_eq!(d.rule(), Ok(CondRule::Duplicates), "no value needed");
        assert_eq!(d.values(), 0);
        d.look = 2;
        assert_eq!(d.look(), CondLook::Green);
        d.look = 99;
        assert_eq!(d.look(), CondLook::LightRed);
    }
}
