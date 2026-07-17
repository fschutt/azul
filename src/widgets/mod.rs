//! Built-in widgets for the Azul GUI system

/// Implements `Display, Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Hash`
/// for a Callback with a `.cb` field.
///
/// This is necessary to work around for <https://github.com/rust-lang/rust/issues/54508>
///
/// # Host-invoker plumbing for managed-FFI bindings
///
/// Widget callbacks have varying shapes — some are
/// `(RefAny, CallbackInfo) -> Update` (Button), others add a state
/// struct (CheckBox/Tab/etc.), a few have two extras (`ListView`). The
/// macro therefore does **not** auto-emit an `impl_managed_callback!`
/// invocation; per-widget files apply it themselves with the right
/// extras list. The base invocation still produces the standard
/// `Display`/`Debug`/`Clone`/`From<CallbackType>`/`From<Callback>` impls
/// that all widget callbacks share.
#[macro_export]
macro_rules! impl_widget_callback {
    (
        $callback_wrapper:ident,
        $option_callback_wrapper:ident,
        $callback_value:ident,
        $callback_ty:ident
    ) => {
        #[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
        #[repr(C)]
        pub struct $callback_wrapper {
            pub refany: RefAny,
            pub callback: $callback_value,
        }

        #[repr(C)]
        pub struct $callback_value {
            pub cb: $callback_ty,
            /// For FFI: stores the foreign callable (e.g., `PyFunction`)
            /// Native Rust code sets this to None
            pub ctx: azul_core::refany::OptionRefAny,
        }

        azul_css::impl_option!(
            $callback_wrapper,
            $option_callback_wrapper,
            copy = false,
            [Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash]
        );

        impl $callback_value {
            /// Create a new callback with just a function pointer (for native Rust code)
            pub fn create<I: Into<$callback_value>>(cb: I) -> $callback_value {
                cb.into()
            }
        }

        impl ::core::fmt::Display for $callback_value {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                write!(f, "{:?}", self)
            }
        }

        impl ::core::fmt::Debug for $callback_value {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                let callback = stringify!($callback_value);
                write!(f, "{} @ 0x{:x}", callback, self.cb as *const () as usize)
            }
        }

        impl Clone for $callback_value {
            fn clone(&self) -> Self {
                $callback_value {
                    cb: self.cb.clone(),
                    ctx: self.ctx.clone(),
                }
            }
        }

        impl core::hash::Hash for $callback_value {
            fn hash<H>(&self, state: &mut H)
            where
                H: ::core::hash::Hasher,
            {
                state.write_usize(self.cb as *const () as usize);
            }
        }

        impl PartialEq for $callback_value {
            fn eq(&self, rhs: &Self) -> bool {
                self.cb as *const () as usize == rhs.cb as usize
            }
        }

        impl PartialOrd for $callback_value {
            fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
                Some((self.cb as *const () as usize).cmp(&(other.cb as usize)))
            }
        }

        impl Ord for $callback_value {
            fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {
                (self.cb as *const () as usize).cmp(&(other.cb as usize))
            }
        }

        impl Eq for $callback_value {}

        /// Allow creating callback from a raw function pointer
        /// Sets callable to None (for native Rust/C usage)
        impl From<$callback_ty> for $callback_value {
            fn from(cb: $callback_ty) -> $callback_value {
                $callback_value {
                    cb,
                    ctx: azul_core::refany::OptionRefAny::None,
                }
            }
        }

        /// Allow creating widget callback from a generic Callback
        /// This enables Python/FFI code to pass generic callbacks to widget methods
        impl From<$crate::callbacks::Callback> for $callback_value {
            // transmute target ($callback_value's cb fn-ptr type) varies per macro
            // instantiation, so an explicit annotation can't be written generically here.
            #[allow(clippy::missing_transmute_annotations, clippy::useless_transmute)]
            fn from(cb: $crate::callbacks::Callback) -> $callback_value {
                $callback_value {
                    cb: unsafe { core::mem::transmute(cb.cb) },
                    ctx: cb.ctx,
                }
            }
        }
    };
}

/// Button widget
pub mod button;
/// Checkbox widget
pub mod check_box;
/// Box displaying a color with a callback for value changes
pub mod color_input;
/// File input widget
pub mod file_input;
/// Label widget (centered text)
pub mod label;
/// Drop-down select widget
pub mod drop_down;
/// Frame container widget
pub mod frame;
/// List view widget
pub mod list_view;
/// Shared core for the video-ish widgets (camera/screencap/video): the
/// `VideoFrame` type + the GL-texture `present_frame` writeback.
///
/// See
/// `capture_common.rs`.
pub mod capture_common;
/// Camera-preview widget (P6) — a "dumb widget" owning a background capture
/// thread + a GL-texture ImageRef; no camera logic in core.
///
/// Same RefAny-
/// dataset + merge-callback design as the map widget. See `camera.rs`.
pub mod camera;
/// Screen-capture widget (P6) — identical "dumb widget" architecture to the
/// camera widget, capturing a display/window instead.
///
/// See `screencap.rs`.
pub mod screencap;
/// Video-playback widget (P6) — same "dumb widget" architecture, decoding a
/// video source (vk-video) into a GL texture.
///
/// See `video.rs`.
pub mod video;
/// Microphone-capture widget (P7) — same "dumb widget" architecture as the
/// capture widgets, audio instead of video (no GL): a background thread feeds
/// each `AudioFrame` to the user's `on_frame` hook.
///
/// See `microphone.rs`.
pub mod microphone;
/// Map widget — MVT tile + MapCSS → SVG → DOM (AzulMaps goal app, P3).
///
/// Cache lives in a dataset RefAny owned by a merge callback so it
/// survives relayout. See `layout/src/widgets/map.rs` for the design.
pub mod map;
/// Software menu-bar widget (Linux fallback when there is no native global menu).
///
/// Renders a window's `Menu` as a horizontal bar; items open dropdowns via the
/// unified `WindowPosition::RelativeToParentWindow` popup path.
pub mod menubar;
/// Node graph widget
pub mod node_graph;
/// Same as text input, but only allows numeric input
pub mod number_input;
/// Progress bar widget
pub mod progressbar;
/// Ribbon widget
pub mod ribbon;
/// Tab container widgets
pub mod tabs;
/// Single line text input widget
pub mod text_input;
/// Titlebar widget for custom window chrome
pub mod titlebar;
/// Tree view widget
pub mod tree_view;
/// Switch / toggle widget.
///
/// Boolean on/off with a sliding knob; see `switch.rs`.
pub mod switch;
/// Divider / separator rule widget (horizontal or vertical).
///
/// See `divider.rs`.
pub mod divider;
/// Card container widget.
///
/// Elevated/bordered content box (no title); see `card.rs`.
pub mod card;
/// Badge widget.
///
/// A small rounded count/status pill (stateless); see `badge.rs`.
pub mod badge;
/// Slider / range widget.
///
/// Draggable thumb on a track → numeric value; see `slider.rs`.
pub mod slider;
/// Segmented control widget.
///
/// Joined row of mutually-exclusive buttons; see `segmented.rs`.
pub mod segmented;
/// Radio-group widget.
///
/// Vertical/horizontal group of mutually-exclusive options (exactly one selected) with a circular indicator; see `radio_group.rs`.
pub mod radio_group;
/// Tooltip widget.
///
/// Shows a small text popup near an anchor on hover; see `tooltip.rs`.
pub mod tooltip;
/// Multi-line text input (text area) widget.
///
/// See `text_area.rs`.
pub mod text_area;
/// Alert / banner widget.
///
/// A coloured inline message box with an optional dismissible close button; see `alert.rs`.
pub mod alert;
/// Accordion / expander widget.
///
/// One or more collapsible titled sections; see `accordion.rs`.
pub mod accordion;
/// Avatar widget.
///
/// A circular image/initials badge (stateless); see `avatar.rs`.
pub mod avatar;
/// Chip / tag widget.
///
/// A compact rounded pill with a label + optional removable "x" (stateful when removable, mirrors alert's dismiss); see `chip.rs`.
pub mod chip;
/// Spinner / activity widget.
///
/// A static indeterminate busy ring (stateless; no animation — see the file's PARTIAL/TODO2 note); see `spinner.rs`.
pub mod spinner;
/// Popover widget.
///
/// A click-triggered floating panel holding arbitrary content, anchored to a `Dom` (the click-toggled sibling of tooltip); see `popover.rs`.
pub mod popover;
/// Combobox widget.
///
/// An editable text field with a click-toggled drop-down list of options (drop_down's select + text_input's editable field); see `combobox.rs`.
pub mod combobox;
/// Modal / dialog widget.
///
/// An in-app overlay dialog (backdrop + centred panel + arbitrary content), shown/hidden via state toggle; see `modal.rs`.
pub mod modal;
/// Toast / snackbar widget.
///
/// A transient floating notification banner pinned to a corner, manually dismissed via "x" (auto-timeout needs a host timer — see the file's TODO2); a near-clone of `alert.rs` positioned as an overlay; see `toast.rs`.
pub mod toast;
/// Breadcrumb widget.
///
/// A horizontal trail of clickable crumb links separated by "/", ending in the current (non-clickable) page; see `breadcrumb.rs`.
pub mod breadcrumb;
/// Pagination widget.
///
/// A `Prev` / page-numbers / `Next` page navigator with an active-page restyle (segmented-style); see `pagination.rs`.
pub mod pagination;
/// Stepper / wizard widget.
///
/// A horizontal numbered-step progress indicator with connector lines and an accent/muted restyle on step change (segmented-style + progressbar-style filled connector); see `stepper.rs`.
pub mod stepper;
/// Split-pane / splitter widget.
///
/// A two-pane (horizontal/vertical) container with a draggable divider that live-resizes the panes via `set_css_property` (the frame two-box layout + the map/slider pointer-drag state machine); see `split_pane.rs`.
pub mod split_pane;
/// Time picker widget.
///
/// Two clamped numeric up/down spinners (hour + minute) side by side with an optional AM/PM toggle for 12-hour mode (the number_input clamp/retext path + segmented's clickable-cell navigation); see `time_picker.rs`.
pub mod time_picker;
/// Calendar date picker widget.
///
/// A month header (‹ / `Month YYYY` / ›) above a weekday-labelled 7-column day grid computed from real calendar math; clicking a day selects + restyles it (segmented-style), and the per-cell day number is carried drop_down-style. Month nav fires on_change but cannot rebuild the grid in-widget (prominent module TODO2); see `date_picker.rs`.
pub mod date_picker;
// /// Spreadsheet (virtualized view) widget
// pub mod spreadsheet;
