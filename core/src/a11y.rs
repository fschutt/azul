//! Accessibility types for screen reader support.
//!
//! Key types:
//! - [`AccessibilityInfo`] — full accessibility metadata for a UI element
//! - [`SmallAriaInfo`] — lightweight alternative for common cases (label + role + description)
//! - [`AccessibilityRole`] — element purpose (button, link, checkbox, etc.)
//! - [`AccessibilityState`] — dynamic state (focused, checked, expanded, etc.)
//! - [`AccessibilityAction`] — actions performable on an element (click, scroll, etc.)
//!
//! These types are consumed by `layout/src/managers/a11y.rs` and mapped to
//! platform accessibility backends in `dll/src/desktop/shell2/`.

use crate::{dom::OptionDomNodeId, geom::LogicalPosition, window::OptionVirtualKeyCodeCombo};
use alloc::vec::Vec;
use azul_css::{props::basic::length::FloatValue, AzString, OptionF32, OptionString};

/// Holds information about a UI element for accessibility purposes (e.g., screen readers).
/// This is a wrapper for platform-specific accessibility APIs like MSAA.
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct AccessibilityInfo {
    /// The accessible name (e.g., button or checkbox text).
    pub accessibility_name: OptionString,
    /// The current value (e.g., slider number, link URL, or input text).
    pub accessibility_value: OptionString,
    /// Optional text description providing additional context about the element.
    /// Maps to `aria-description` / accesskit's `set_description()`.
    pub description: OptionString,
    /// Optional keyboard accelerator.
    pub accelerator: OptionVirtualKeyCodeCombo,
    /// Optional "default action" description. Only used when there is at least
    /// one `ComponentEventFilter::DefaultAction` callback present on this node.
    pub default_action: OptionString,
    /// Possible on/off states, such as focused, focusable, selected, selectable,
    /// visible, protected (for passwords), checked, etc.
    pub states: AccessibilityStateVec,
    /// A list of actions the user can perform on this element.
    /// Maps to accesskit's Action enum.
    pub supported_actions: AccessibilityActionVec,
    /// ID of another node that labels this one (for `aria-labelledby`).
    pub labelled_by: OptionDomNodeId,
    /// ID of another node that describes this one (for `aria-describedby`).
    pub described_by: OptionDomNodeId,
    /// The element's role (e.g., link, static text, checkbox).
    pub role: AccessibilityRole,
    /// For live regions that update automatically (e.g., chat messages, timers).
    /// Maps to accesskit's `Live` property.
    pub is_live_region: bool,
}

impl AccessibilityInfo {
    /// Creates an `AccessibilityInfo` with a name and a role.
    #[must_use]
    pub fn named(name: impl Into<AzString>, role: AccessibilityRole) -> Self {
        Self {
            accessibility_name: OptionString::Some(name.into()),
            role,
            ..Self::default()
        }
    }

    /// Creates an `AccessibilityInfo` labelled by another DOM node ID.
    #[must_use]
    pub fn labelled_by_node(label: crate::dom::DomNodeId, role: AccessibilityRole) -> Self {
        Self {
            labelled_by: OptionDomNodeId::Some(label),
            role,
            ..Self::default()
        }
    }

    /// Overlays `patch` onto `self`, updating only the fields explicitly set in `patch`.
    #[allow(clippy::needless_pass_by_value)] // by value: crosses the FFI, where the argument arrives owned
    pub fn assign(&mut self, patch: Self) {
        if patch.accessibility_name.is_some() {
            self.accessibility_name = patch.accessibility_name.clone();
        }
        if patch.accessibility_value.is_some() {
            self.accessibility_value = patch.accessibility_value.clone();
        }
        if patch.description.is_some() {
            self.description = patch.description.clone();
        }
        if patch.accelerator.is_some() {
            self.accelerator = patch.accelerator.clone();
        }
        if patch.default_action.is_some() {
            self.default_action = patch.default_action.clone();
        }
        if !patch.states.as_ref().is_empty() {
            self.states = patch.states.clone();
        }
        if !patch.supported_actions.as_ref().is_empty() {
            self.supported_actions = patch.supported_actions.clone();
        }
        if patch.labelled_by.is_some() {
            self.labelled_by = patch.labelled_by;
        }
        if patch.described_by.is_some() {
            self.described_by = patch.described_by;
        }
        if !matches!(patch.role, AccessibilityRole::Unknown) {
            self.role = patch.role;
        }
        if patch.is_live_region {
            self.is_live_region = true;
        }
    }

    /// [`AccessibilityInfo::assign`] as a builder: returns the merged value.
    #[must_use]
    pub fn assigned(mut self, patch: Self) -> Self {
        self.assign(patch);
        self
    }

    /// Attach a live value — a slider's position, a progress percentage.
    #[must_use]
    pub fn with_value(mut self, value: impl Into<AzString>) -> Self {
        self.accessibility_value = OptionString::Some(value.into());
        self
    }
}

impl Default for AccessibilityInfo {
    /// Provides an empty declaration with an `Unknown` role.
    fn default() -> Self {
        Self {
            accessibility_name: OptionString::None,
            accessibility_value: OptionString::None,
            description: OptionString::None,
            accelerator: OptionVirtualKeyCodeCombo::None,
            default_action: OptionString::None,
            states: AccessibilityStateVec::from_const_slice(&[]),
            supported_actions: AccessibilityActionVec::from_const_slice(&[]),
            labelled_by: OptionDomNodeId::None,
            described_by: OptionDomNodeId::None,
            role: AccessibilityRole::Unknown,
            is_live_region: false,
        }
    }
}

/// Actions that can be performed on an accessible element.
/// This is a simplified version of `accesskit::Action` to avoid direct dependency in core.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum AccessibilityAction {
    /// The default action for the element (usually a click).
    Default,
    /// Set focus to this element.
    Focus,
    /// Remove focus from this element.
    Blur,
    /// Collapse an expandable element (e.g., tree node, accordion).
    Collapse,
    /// Expand a collapsible element (e.g., tree node, accordion).
    Expand,
    /// Scroll this element into view.
    ScrollIntoView,
    /// Increment a numeric value (e.g., slider, spinner).
    Increment,
    /// Decrement a numeric value (e.g., slider, spinner).
    Decrement,
    /// Show a context menu.
    ShowContextMenu,
    /// Hide a tooltip.
    HideTooltip,
    /// Show a tooltip.
    ShowTooltip,
    /// Scroll up.
    ScrollUp,
    /// Scroll down.
    ScrollDown,
    /// Scroll left.
    ScrollLeft,
    /// Scroll right.
    ScrollRight,
    /// Replace selected text with new text.
    ReplaceSelectedText(AzString),
    /// Scroll to a specific point.
    ScrollToPoint(LogicalPosition),
    /// Set scroll offset.
    SetScrollOffset(LogicalPosition),
    /// Set text selection.
    SetTextSelection(TextSelectionStartEnd),
    /// Set sequential focus navigation starting point.
    SetSequentialFocusNavigationStartingPoint,
    /// Set the value of a control.
    SetValue(AzString),
    /// Set numeric value of a control.
    SetNumericValue(FloatValue),
    /// Custom action with ID.
    CustomAction(i32),
}

/// Represents the start and end indices of a text selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TextSelectionStartEnd {
    /// The starting index of the selection.
    pub selection_start: usize,
    /// The ending index of the selection.
    pub selection_end: usize,
}

impl_vec!(
    AccessibilityAction,
    AccessibilityActionVec,
    AccessibilityActionVecDestructor,
    AccessibilityActionVecDestructorType,
    AccessibilityActionVecSlice,
    OptionAccessibilityAction
);
impl_vec_debug!(AccessibilityAction, AccessibilityActionVec);
impl_vec_clone!(
    AccessibilityAction,
    AccessibilityActionVec,
    AccessibilityActionVecDestructor
);
impl_vec_partialeq!(AccessibilityAction, AccessibilityActionVec);
impl_vec_eq!(AccessibilityAction, AccessibilityActionVec);
impl_vec_partialord!(AccessibilityAction, AccessibilityActionVec);
impl_vec_ord!(AccessibilityAction, AccessibilityActionVec);
impl_vec_hash!(AccessibilityAction, AccessibilityActionVec);

impl_option![
    AccessibilityAction,
    OptionAccessibilityAction,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
];

impl_option!(
    AccessibilityInfo,
    OptionAccessibilityInfo,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Defines the element's purpose for accessibility APIs, informing assistive technologies
/// like screen readers about the function of a UI element.
///
/// Each variant corresponds to a
/// standard control type or UI structure.
///
/// For more details, see the [MSDN Role Constants page](https://docs.microsoft.com/en-us/windows/winauto/object-roles).
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub enum AccessibilityRole {
    /// Represents the title or caption bar of a window.
    TitleBar,
    /// Represents a menu bar at the top of a window.
    MenuBar,
    /// Represents a vertical or horizontal scroll bar.
    ScrollBar,
    /// Represents a handle or grip used for moving or resizing.
    Grip,
    /// Represents a system sound indicating an event.
    Sound,
    /// Represents the system's mouse pointer or other pointing device.
    Cursor,
    /// Represents the text insertion point indicator.
    Caret,
    /// Represents an alert or notification.
    Alert,
    /// Represents a window frame.
    Window,
    /// Represents a window's client area, where the main content is displayed.
    Client,
    /// Represents a pop-up menu.
    MenuPopup,
    /// Represents an individual item within a menu.
    MenuItem,
    /// Represents a small pop-up window that provides information.
    Tooltip,
    /// Represents the main window of an application.
    Application,
    /// Represents a document window within an application.
    Document,
    /// Represents a pane or a distinct section of a window.
    Pane,
    /// Represents a graphical chart or graph.
    Chart,
    /// Represents a dialog box or message box.
    Dialog,
    /// Represents a window's border.
    Border,
    /// Represents a group of related controls.
    Grouping,
    /// Represents a visual separator.
    Separator,
    /// Represents a toolbar containing a group of controls.
    Toolbar,
    /// Represents a status bar for displaying information.
    StatusBar,
    /// Represents a data table.
    Table,
    /// Represents a column header in a table.
    ColumnHeader,
    /// Represents a row header in a table.
    RowHeader,
    /// Represents a full column of cells in a table.
    Column,
    /// Represents a full row of cells in a table.
    Row,
    /// Represents a single cell within a table.
    Cell,
    /// Represents a hyperlink to a resource.
    Link,
    /// Represents a help balloon or pop-up.
    HelpBalloon,
    /// Represents an animated, character-like graphic object.
    Character,
    /// Represents a list of items.
    List,
    /// Represents an individual item within a list.
    ListItem,
    /// Represents an outline or tree structure.
    Outline,
    /// Represents an individual item within an outline or tree.
    OutlineItem,
    /// Represents a single tab in a tabbed interface.
    PageTab,
    /// Represents the content of a page in a property sheet.
    PropertyPage,
    /// Represents a visual indicator, like a slider thumb.
    Indicator,
    /// Represents a picture or graphical image.
    Graphic,
    /// Represents read-only text.
    StaticText,
    /// Represents editable text or a text area.
    Text,
    /// Represents a standard push button.
    PushButton,
    /// Represents a check box control.
    CheckButton,
    /// Represents a radio button.
    RadioButton,
    /// Represents a combination of a text field and a drop-down list.
    ComboBox,
    /// Represents a drop-down list box.
    DropList,
    /// Represents a progress bar.
    ProgressBar,
    /// Represents a dial or knob.
    Dial,
    /// Represents a control for entering a keyboard shortcut.
    HotkeyField,
    /// Represents a slider for selecting a value within a range.
    Slider,
    /// Represents a spin button (up/down arrows) for incrementing or decrementing a value.
    SpinButton,
    /// Represents a diagram or flowchart.
    Diagram,
    /// Represents an animation control.
    Animation,
    /// Represents a mathematical equation.
    Equation,
    /// Represents a button that drops down a list of items.
    ButtonDropdown,
    /// Represents a button that drops down a full menu.
    ButtonMenu,
    /// Represents a button that drops down a grid for selection.
    ButtonDropdownGrid,
    /// Represents blank space between other objects.
    Whitespace,
    /// Represents the container for a set of tabs.
    PageTabList,
    /// Represents a clock control.
    Clock,
    /// Represents a button with two parts: a default action and a dropdown.
    SplitButton,
    /// Represents a control for entering an IP address.
    IpAddress,
    /// Represents an element with no specific role.
    Nothing,
    /// Unknown or unspecified role.
    Unknown,
}

impl_option!(
    AccessibilityRole,
    OptionAccessibilityRole,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

/// Defines the current state of an element for accessibility APIs (e.g., focused, checked).
/// These states provide dynamic information to assistive technologies about the element's
/// condition.
///
/// See the [MSDN State Constants page](https://docs.microsoft.com/en-us/windows/win32/winauto/object-state-constants) for more details.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum AccessibilityState {
    /// The element is unavailable and cannot be interacted with.
    Unavailable,
    /// The element is selected.
    Selected,
    /// The element has the keyboard focus.
    Focused,
    /// The element is checked, toggled, or in an "on" state.
    CheckedTrue,
    /// The element is unchecked, untoggled, or in an "off" state.
    CheckedFalse,
    /// The element's content cannot be edited by the user.
    Readonly,
    /// The element is the default action in a dialog or form.
    Default,
    /// The element is expanded, showing its child items.
    Expanded,
    /// The element is collapsed, hiding its child items.
    Collapsed,
    /// The element is busy and cannot respond to user interaction.
    Busy,
    /// The element is not currently visible on the screen.
    Offscreen,
    /// The element can accept keyboard focus.
    Focusable,
    /// The element is a container whose children can be selected.
    Selectable,
    /// The element is a hyperlink.
    Linked,
    /// The element is a hyperlink that has been visited.
    Traversed,
    /// The element allows multiple of its children to be selected at once.
    Multiselectable,
    /// The element contains protected content that should not be read aloud.
    Protected,
}

impl_option!(
    AccessibilityState,
    OptionAccessibilityState,
    [Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash]
);

impl_vec!(
    AccessibilityState,
    AccessibilityStateVec,
    AccessibilityStateVecDestructor,
    AccessibilityStateVecDestructorType,
    AccessibilityStateVecSlice,
    OptionAccessibilityState
);
impl_vec_clone!(
    AccessibilityState,
    AccessibilityStateVec,
    AccessibilityStateVecDestructor
);
impl_vec_debug!(AccessibilityState, AccessibilityStateVec);
impl_vec_partialeq!(AccessibilityState, AccessibilityStateVec);
impl_vec_partialord!(AccessibilityState, AccessibilityStateVec);
impl_vec_eq!(AccessibilityState, AccessibilityStateVec);
impl_vec_ord!(AccessibilityState, AccessibilityStateVec);
impl_vec_hash!(AccessibilityState, AccessibilityStateVec);

/// Compact accessibility information for common use cases.
///
/// This is a lighter-weight alternative to `AccessibilityInfo` for cases where
/// only basic accessibility properties are needed. Developers must explicitly
/// pass `None` if they choose not to provide accessibility information.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct SmallAriaInfo {
    /// Accessible label/name
    pub label: OptionString,
    /// Element's role (button, link, etc.)
    pub role: OptionAccessibilityRole,
    /// Additional description
    pub description: OptionString,
}

impl_option!(
    SmallAriaInfo,
    OptionSmallAriaInfo,
    copy = false,
    [Debug, Clone, PartialEq, Eq, Hash]
);

impl SmallAriaInfo {
    /// Creates a `SmallAriaInfo` with the given accessible label.
    pub fn label<S: Into<AzString>>(text: S) -> Self {
        Self {
            label: OptionString::Some(text.into()),
            role: OptionAccessibilityRole::None,
            description: OptionString::None,
        }
    }

    /// Builder method for setting self.role
    #[must_use]
    pub const fn with_role(mut self, role: AccessibilityRole) -> Self {
        self.role = OptionAccessibilityRole::Some(role);
        self
    }

    /// Builder method for setting self.description
    #[must_use]
    pub fn with_description<S: Into<AzString>>(mut self, desc: S) -> Self {
        self.description = OptionString::Some(desc.into());
        self
    }

    /// Convert to full `AccessibilityInfo`
    #[must_use]
    pub fn to_full_info(&self) -> AccessibilityInfo {
        AccessibilityInfo {
            accessibility_name: self.label.clone(),
            accessibility_value: OptionString::None,
            description: self.description.clone(),
            role: match self.role {
                OptionAccessibilityRole::Some(r) => r,
                OptionAccessibilityRole::None => AccessibilityRole::Unknown,
            },
            states: Vec::new().into(),
            accelerator: OptionVirtualKeyCodeCombo::None,
            default_action: OptionString::None,
            supported_actions: Vec::new().into(),
            is_live_region: false,
            labelled_by: OptionDomNodeId::None,
            described_by: OptionDomNodeId::None,
        }
    }
}

/// Accessibility information for a `<progress>` indicator.
///
/// Mirrors HTML's `<progress value max>` plus an `indeterminate` flag for
/// progress bars whose end is unknown. Maps to `AccessibilityRole::ProgressBar`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ProgressAriaInfo {
    /// Accessible label describing the task being measured.
    pub label: OptionString,
    /// Current progress value. `None` for indeterminate progress.
    pub current_value: OptionF32,
    /// Maximum value the progress bar can reach. `None` falls back to `1.0`.
    pub max: OptionF32,
    /// `true` for spinners / progress with no known endpoint. Overrides `current_value`.
    pub indeterminate: bool,
    /// Optional extended description (`aria-describedby` equivalent).
    pub description: OptionString,
}

impl_option!(
    ProgressAriaInfo,
    OptionProgressAriaInfo,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl ProgressAriaInfo {
    /// Creates a `ProgressAriaInfo` with only an accessible label.
    #[must_use]
    pub const fn create(label: AzString) -> Self {
        Self {
            label: OptionString::Some(label),
            current_value: OptionF32::None,
            max: OptionF32::None,
            indeterminate: false,
            description: OptionString::None,
        }
    }

    /// Returns a copy with the given current value.
    #[must_use]
    pub const fn with_current_value(mut self, value: f32) -> Self {
        self.current_value = OptionF32::Some(value);
        self
    }

    /// Returns a copy with the given maximum value.
    #[must_use]
    pub const fn with_max(mut self, max: f32) -> Self {
        self.max = OptionF32::Some(max);
        self
    }

    /// Returns a copy with the indeterminate flag set.
    #[must_use]
    pub const fn with_indeterminate(mut self, indeterminate: bool) -> Self {
        self.indeterminate = indeterminate;
        self
    }

    /// Returns a copy with the given description.
    #[must_use]
    pub fn with_description(mut self, desc: AzString) -> Self {
        self.description = OptionString::Some(desc);
        self
    }

    /// Convert to full `AccessibilityInfo` so the value can be installed on a node.
    #[must_use]
    pub fn to_full_info(&self) -> AccessibilityInfo {
        let value_string = if self.indeterminate {
            OptionString::None
        } else {
            match self.current_value {
                OptionF32::Some(v) => OptionString::Some(format!("{v}").into()),
                OptionF32::None => OptionString::None,
            }
        };
        AccessibilityInfo {
            accessibility_name: self.label.clone(),
            accessibility_value: value_string,
            description: self.description.clone(),
            role: AccessibilityRole::ProgressBar,
            states: Vec::new().into(),
            accelerator: OptionVirtualKeyCodeCombo::None,
            default_action: OptionString::None,
            supported_actions: Vec::new().into(),
            is_live_region: false,
            labelled_by: OptionDomNodeId::None,
            described_by: OptionDomNodeId::None,
        }
    }
}

/// Accessibility information for a `<meter>` gauge.
///
/// Unlike `<progress>`, `<meter>` always carries a known `value`/`min`/`max`
/// triple, so those fields are required at construction time. Maps to
/// `AccessibilityRole::Indicator`.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct MeterAriaInfo {
    /// Accessible label describing what the meter measures.
    pub label: OptionString,
    /// Current value of the meter (within `[min, max]`).
    pub current_value: f32,
    /// Lower bound of the measurement range.
    pub min: f32,
    /// Upper bound of the measurement range.
    pub max: f32,
    /// Optional "low" threshold (values below this are considered low).
    pub low: OptionF32,
    /// Optional "high" threshold (values above this are considered high).
    pub high: OptionF32,
    /// Optional optimum value within the range.
    pub optimum: OptionF32,
    /// Optional extended description.
    pub description: OptionString,
}

impl_option!(
    MeterAriaInfo,
    OptionMeterAriaInfo,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl MeterAriaInfo {
    /// Creates a `MeterAriaInfo` with the required label and value/range triple.
    #[must_use]
    pub const fn create(label: AzString, current_value: f32, min: f32, max: f32) -> Self {
        Self {
            label: OptionString::Some(label),
            current_value,
            min,
            max,
            low: OptionF32::None,
            high: OptionF32::None,
            optimum: OptionF32::None,
            description: OptionString::None,
        }
    }

    /// Returns a copy with the given low threshold.
    #[must_use]
    pub const fn with_low(mut self, low: f32) -> Self {
        self.low = OptionF32::Some(low);
        self
    }

    /// Returns a copy with the given high threshold.
    #[must_use]
    pub const fn with_high(mut self, high: f32) -> Self {
        self.high = OptionF32::Some(high);
        self
    }

    /// Returns a copy with the given optimum value.
    #[must_use]
    pub const fn with_optimum(mut self, optimum: f32) -> Self {
        self.optimum = OptionF32::Some(optimum);
        self
    }

    /// Returns a copy with the given description.
    #[must_use]
    pub fn with_description(mut self, desc: AzString) -> Self {
        self.description = OptionString::Some(desc);
        self
    }

    /// Convert to full `AccessibilityInfo` so the value can be installed on a node.
    #[must_use]
    pub fn to_full_info(&self) -> AccessibilityInfo {
        AccessibilityInfo {
            accessibility_name: self.label.clone(),
            accessibility_value: OptionString::Some(format!("{}", self.current_value).into()),
            description: self.description.clone(),
            role: AccessibilityRole::Indicator,
            states: Vec::new().into(),
            accelerator: OptionVirtualKeyCodeCombo::None,
            default_action: OptionString::None,
            supported_actions: Vec::new().into(),
            is_live_region: false,
            labelled_by: OptionDomNodeId::None,
            described_by: OptionDomNodeId::None,
        }
    }
}

/// Accessibility information for a `<dialog>` element.
///
/// Captures the modal/non-modal distinction and a reference to a separate
/// node that describes the dialog (`aria-describedby`). The `role` defaults
/// to `AccessibilityRole::Dialog` but can be overridden (e.g., for alert
/// dialogs).
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct DialogAriaInfo {
    /// Accessible label / title for the dialog.
    pub label: OptionString,
    /// Optional ID of another node that describes the dialog content.
    pub described_by: OptionString,
    /// Optional inline description.
    pub description: OptionString,
    /// Role for the dialog. Defaults to `Dialog`; use `Alert` for urgent dialogs.
    pub role: AccessibilityRole,
    /// `true` if the dialog is modal (focus trapped, background inert).
    pub modal: bool,
}

impl_option!(
    DialogAriaInfo,
    OptionDialogAriaInfo,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl DialogAriaInfo {
    /// Creates a `DialogAriaInfo` with the given accessible label. Defaults
    /// to a non-modal dialog with role `Dialog`.
    #[must_use]
    pub const fn create(label: AzString) -> Self {
        Self {
            label: OptionString::Some(label),
            modal: false,
            described_by: OptionString::None,
            role: AccessibilityRole::Dialog,
            description: OptionString::None,
        }
    }

    /// Returns a copy with the given modality flag.
    #[must_use]
    pub const fn with_modal(mut self, modal: bool) -> Self {
        self.modal = modal;
        self
    }

    /// Returns a copy with `aria-describedby` pointing at the given node ID.
    #[must_use]
    pub fn with_described_by(mut self, described_by: AzString) -> Self {
        self.described_by = OptionString::Some(described_by);
        self
    }

    /// Returns a copy with the given role (defaults to `Dialog`).
    #[must_use]
    pub const fn with_role(mut self, role: AccessibilityRole) -> Self {
        self.role = role;
        self
    }

    /// Returns a copy with the given inline description.
    #[must_use]
    pub fn with_description(mut self, desc: AzString) -> Self {
        self.description = OptionString::Some(desc);
        self
    }

    /// Convert to full `AccessibilityInfo` so the value can be installed on a node.
    #[must_use]
    pub fn to_full_info(&self) -> AccessibilityInfo {
        AccessibilityInfo {
            accessibility_name: self.label.clone(),
            accessibility_value: OptionString::None,
            description: self.description.clone(),
            role: self.role,
            states: Vec::new().into(),
            accelerator: OptionVirtualKeyCodeCombo::None,
            default_action: OptionString::None,
            supported_actions: Vec::new().into(),
            is_live_region: false,
            labelled_by: OptionDomNodeId::None,
            described_by: OptionDomNodeId::None,
        }
    }
}

#[cfg(test)]
#[path = "a11y_test.rs"]
mod a11y_test;
