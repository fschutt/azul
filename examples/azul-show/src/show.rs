//! The slide show (full screen in the main window) and the presenter view
//! (the second window: the current slide, the next one, the notes, the
//! elapsed time, the controls and a strip to jump), and the show's keys.

use azul::{
    callbacks::{ButtonOnClickCallbackType, RefAny},
    css::{EventFilter, HoverEventFilter},
    dom::{Dom, VirtualKeyCode},
    str::String as AzString,
    widgets::Button,
};

use crate::{
    app::{command, AppState, Command, ShowRuntime},
    commands::on_command,
    model::{Blank, Deck, TransitionKind},
    render::{self, css_color, RenderOptions},
};

/// The command a key runs in the show; digits collect in `typed` for
/// "number + Enter".
#[must_use]
pub fn show_key_command(key: VirtualKeyCode, typed: &mut String) -> Option<Command> {
    use VirtualKeyCode as K;
    let digit = match key {
        K::Key0 | K::Numpad0 => Some('0'),
        K::Key1 | K::Numpad1 => Some('1'),
        K::Key2 | K::Numpad2 => Some('2'),
        K::Key3 | K::Numpad3 => Some('3'),
        K::Key4 | K::Numpad4 => Some('4'),
        K::Key5 | K::Numpad5 => Some('5'),
        K::Key6 | K::Numpad6 => Some('6'),
        K::Key7 | K::Numpad7 => Some('7'),
        K::Key8 | K::Numpad8 => Some('8'),
        K::Key9 | K::Numpad9 => Some('9'),
        _ => None,
    };
    if let Some(d) = digit {
        typed.push(d);
        return None;
    }
    let cmd = match key {
        K::Return | K::NumpadEnter if !typed.is_empty() => {
            let n = typed.parse::<usize>().unwrap_or(1).max(1);
            Some(Command::ShowGoto(n - 1))
        }
        K::Right | K::Down | K::Space | K::PageDown | K::Return | K::NumpadEnter | K::N => Some(Command::ShowNext),
        K::Left | K::Up | K::Back | K::PageUp | K::P => Some(Command::ShowPrev),
        K::B => Some(Command::ShowBlank(Blank::Black)),
        K::W => Some(Command::ShowBlank(Blank::White)),
        K::Escape => Some(Command::ShowEnd),
        K::Home => Some(Command::ShowGoto(0)),
        _ => None,
    };
    typed.clear();
    cmd
}

/// The slide the show is on, fitted into `w` x `h`, its builds played to the
/// show's step and the playing build at its progress.
fn shown_slide(st: &AppState, deck: &Deck, rt: &ShowRuntime, index: usize, step: usize, scale: f32, playing: bool) -> Dom {
    let ids: Vec<u64> = rt
        .play
        .as_ref()
        .filter(|_| playing)
        .map(|p| p.ids.clone())
        .unwrap_or_default();
    let progress = rt.play.as_ref().map_or(1.0, crate::app::Play::progress);
    let opts = RenderOptions {
        scale,
        editing: None,
        prompts: false,
        step: Some(step),
        playing: if ids.is_empty() { None } else { Some((ids.as_slice(), progress)) },
        media: &st.media,
        hooks: None,
    };
    render::slide_dom(deck, &deck.slides[index.min(deck.slides.len() - 1)], &opts)
}

/// The main window during the show: black around the slide, the slide
/// fitted, the transition into it playing; a click goes forward.
#[must_use]
pub fn show_screen(app: &RefAny, st: &AppState, window_w: f32, window_h: f32) -> Dom {
    let root = |child: Dom, background: &str| {
        Dom::create_div()
            .with_css(format!(
                "display: flex; align-items: center; justify-content: center; flex-grow: 1; width: 100%; \
                 height: 100%; overflow: hidden; background: {background}; cursor: default;"
            ))
            .with_callback(
                EventFilter::Hover(HoverEventFilter::MouseUp),
                command(app, Command::ShowNext),
                on_command,
            )
            .with_child(child)
    };
    let (Some(ed), Some(rt)) = (st.editor.as_ref(), st.show.as_ref()) else {
        return root(Dom::create_div(), "#000000");
    };
    let deck = &ed.deck;
    if rt.state.ended {
        return root(
            Dom::create_p_with_text("End of slide show, click to exit.").with_css("margin: 0px; color: #f0f0f0; font-size: 20px;"),
            "#000000",
        );
    }
    match rt.state.blank {
        Some(Blank::Black) => return root(Dom::create_div(), "#000000"),
        Some(Blank::White) => return root(Dom::create_div(), "#ffffff"),
        None => {}
    }
    let (sw, sh) = (deck.size.width(), deck.size.height());
    let scale = (window_w / sw).min(window_h / sh).max(0.05);
    let (w, h) = (sw * scale, sh * scale);
    let current = shown_slide(st, deck, rt, rt.state.slide, rt.state.step, scale, true);
    let slide = match rt.play.as_ref().and_then(|p| p.transition_from.map(|from| (from, p.progress()))) {
        Some((from, p)) => {
            let from_steps = deck.slides.get(from).map_or(0, |s| s.build_steps().len());
            let old = shown_slide(st, deck, rt, from, from_steps, scale, false);
            let kind = deck.slides[rt.state.slide].transition.kind;
            let layer = |left: f32, opacity: f32, clip_w: f32, child: Dom| {
                Dom::create_div()
                    .with_css(format!(
                        "position: absolute; left: {left:.1}px; top: 0px; width: {clip_w:.1}px; height: {h:.1}px; \
                         overflow: hidden; opacity: {opacity:.3};"
                    ))
                    .with_child(child)
            };
            let (old_layer, new_layer) = match kind {
                TransitionKind::Push => (layer(-p * w, 1.0, w, old), layer((1.0 - p) * w, 1.0, w, current)),
                TransitionKind::Wipe => (layer(0.0, 1.0, w, old), layer(0.0, 1.0, (p * w).max(0.0), current)),
                TransitionKind::Fade | TransitionKind::None => (layer(0.0, 1.0, w, old), layer(0.0, p, w, current)),
            };
            Dom::create_div()
                .with_css(format!("position: relative; width: {w:.1}px; height: {h:.1}px; overflow: hidden;"))
                .with_child(old_layer)
                .with_child(new_layer)
        }
        None => current,
    };
    root(slide, "#000000")
}

fn control(app: &RefAny, label: &str, icon: &str, cmd: Command) -> Dom {
    Button::create(AzString::from(label))
        .with_icon(AzString::from(icon))
        .with_on_click(command(app, cmd), on_command as ButtonOnClickCallbackType)
        .dom()
        .with_css("margin: 0px 8px 0px 0px;")
}

/// `h:mm:ss` of `secs`.
#[must_use]
pub fn clock(secs: u64) -> String {
    format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
}

/// The presenter window: the current slide, the next one, the notes, the
/// elapsed time and the slide number, the controls, the strip to jump.
#[must_use]
pub fn presenter(app: &RefAny, st: &AppState) -> Dom {
    let (Some(ed), Some(rt)) = (st.editor.as_ref(), st.show.as_ref()) else {
        return Dom::create_div()
            .with_css("display: flex; align-items: center; justify-content: center; flex-grow: 1;")
            .with_child(Dom::create_p_with_text("The slide show has ended."));
    };
    let deck = &ed.deck;
    let current_scale = 640.0 / deck.size.width();
    let next_scale = 360.0 / deck.size.width();
    let current = shown_slide(st, deck, rt, rt.state.slide, rt.state.step, current_scale, true);
    let next = match rt.state.upcoming(deck) {
        Some(n) => shown_slide(st, deck, rt, n, 0, next_scale, false),
        None => Dom::create_div()
            .with_css(format!(
                "width: 360px; height: {:.0}px; background: #202020; color: #d0d0d0; display: flex; \
                 align-items: center; justify-content: center;",
                deck.size.height() * next_scale
            ))
            .with_child(Dom::create_p_with_text("End of slide show")),
    };
    let notes = deck.slides.get(rt.state.slide).map_or("", |s| s.notes.as_str());
    let shown = deck.shown_slides();
    let position = shown.iter().position(|&i| i == rt.state.slide).map_or(0, |p| p + 1);
    let heading = |text: &str| {
        Dom::create_p_with_text(text).with_css("margin: 0px 0px 6px 0px; font-size: 12px; font-weight: bold; color: #b0b0b0;")
    };
    let mut strip = Dom::create_div().with_css("display: flex; flex-direction: row; flex-wrap: wrap; margin: 8px 0px 0px 0px;");
    for &i in &shown {
        let label = format!("{}", i + 1);
        let mut b = Button::create(AzString::from(label.as_str()))
            .with_on_click(command(app, Command::ShowGoto(i)), on_command as ButtonOnClickCallbackType)
            .dom()
            .with_css("margin: 0px 4px 4px 0px;");
        if i == rt.state.slide {
            b = b.with_css(format!("border: 2px solid {};", css_color(deck.theme.colors.accent)));
        }
        strip.add_child(b);
    }
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; flex-grow: 1; padding: 16px; background: #2b2b2b; \
             color: #f0f0f0; overflow-y: auto;",
        )
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row;")
                .with_child(
                    Dom::create_div()
                        .with_css("display: flex; flex-direction: column; margin: 0px 16px 0px 0px;")
                        .with_child(heading(&format!("CURRENT (slide {})", rt.state.slide + 1)))
                        .with_child(current),
                )
                .with_child(
                    Dom::create_div()
                        .with_css("display: flex; flex-direction: column; flex-grow: 1;")
                        .with_child(heading("NEXT"))
                        .with_child(next)
                        .with_child(Dom::create_div().with_css("height: 12px;"))
                        .with_child(heading("NOTES"))
                        .with_child(
                            Dom::create_p_with_text(if notes.is_empty() { "(no notes)" } else { notes })
                                .with_css("margin: 0px; font-size: 18px; line-height: 1.35;"),
                        ),
                ),
        )
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center; margin: 12px 0px 0px 0px;")
                .with_child(
                    Dom::create_p_with_text(format!(
                        "Elapsed {}   Slide {position} of {}{}",
                        clock(rt.started.elapsed().as_secs()),
                        shown.len(),
                        match rt.state.blank {
                            Some(Blank::Black) => "   (black screen)",
                            Some(Blank::White) => "   (white screen)",
                            None => "",
                        }
                    ))
                    .with_css("margin: 0px 16px 0px 0px; font-size: 16px;"),
                )
                .with_child(control(app, "Previous", "navigate_before", Command::ShowPrev))
                .with_child(control(app, "Next", "navigate_next", Command::ShowNext))
                .with_child(control(app, "Black", "square", Command::ShowBlank(Blank::Black)))
                .with_child(control(app, "End Show", "close", Command::ShowEnd)),
        )
        .with_child(strip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_show_keys_go_forward_back_blank_end_and_jump_by_number() {
        let mut typed = String::new();
        assert_eq!(show_key_command(VirtualKeyCode::Space, &mut typed), Some(Command::ShowNext));
        assert_eq!(show_key_command(VirtualKeyCode::Right, &mut typed), Some(Command::ShowNext));
        assert_eq!(show_key_command(VirtualKeyCode::Left, &mut typed), Some(Command::ShowPrev));
        assert_eq!(show_key_command(VirtualKeyCode::Back, &mut typed), Some(Command::ShowPrev));
        assert_eq!(show_key_command(VirtualKeyCode::B, &mut typed), Some(Command::ShowBlank(Blank::Black)));
        assert_eq!(show_key_command(VirtualKeyCode::W, &mut typed), Some(Command::ShowBlank(Blank::White)));
        assert_eq!(show_key_command(VirtualKeyCode::Escape, &mut typed), Some(Command::ShowEnd));
        assert_eq!(show_key_command(VirtualKeyCode::Key1, &mut typed), None);
        assert_eq!(show_key_command(VirtualKeyCode::Key2, &mut typed), None);
        assert_eq!(show_key_command(VirtualKeyCode::Return, &mut typed), Some(Command::ShowGoto(11)));
        assert!(typed.is_empty());
        assert_eq!(show_key_command(VirtualKeyCode::Return, &mut typed), Some(Command::ShowNext));
    }

    #[test]
    fn the_clock_reads_hours_minutes_seconds() {
        assert_eq!(clock(0), "0:00:00");
        assert_eq!(clock(462), "0:07:42");
        assert_eq!(clock(3_725), "1:02:05");
    }
}
