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

use alloc::vec::Vec;
use azul_css::{
    AzString, OptionF32, OptionString,
    props::basic::length::FloatValue,
};
use crate::{
    dom::OptionDomNodeId,
    geom::LogicalPosition,
    window::OptionVirtualKeyCodeCombo,
};

/// Holds information about a UI element for accessibility purposes (e.g., screen readers).
/// This is a wrapper for platform-specific accessibility APIs like MSAA.
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
#[repr(C)]
pub struct AccessibilityInfo {
    /// Get the "name" of the `IAccessible`, for example the
    /// name of a button, checkbox or menu item. Try to use unique names
    /// for each item in a dialog so that voice dictation software doesn't
    /// have to deal with extra ambiguity.
    pub accessibility_name: OptionString,
    /// Get the "value" of the `IAccessible`, for example a number in a slider,
    /// a URL for a link, the text a user entered in a field.
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
    /// Get an enumerated value representing what this `IAccessible` is used for,
    /// for example is it a link, static text, editable text, a checkbox, or a table cell, etc.
    pub role: AccessibilityRole,
    /// For live regions that update automatically (e.g., chat messages, timers).
    /// Maps to accesskit's `Live` property.
    pub is_live_region: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TextSelectionStartEnd {
    pub selection_start: usize,
    pub selection_end: usize,
}

impl_vec!(AccessibilityAction, AccessibilityActionVec, AccessibilityActionVecDestructor, AccessibilityActionVecDestructorType, AccessibilityActionVecSlice, OptionAccessibilityAction);
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
    /// - **Purpose**: To identify the title bar containing the window title and system commands.
    /// - **When to use**: This role is typically inserted by the operating system for standard
    ///   windows.
    /// - **Example**: The bar at the top of an application window displaying its name and the
    ///   minimize, maximize, and close buttons.
    TitleBar,

    /// Represents a menu bar at the top of a window.
    /// - **Purpose**: To contain a set of top-level menus for an application.
    /// - **When to use**: For the main menu bar of an application, such as one containing "File,"
    ///   "Edit," and "View."
    /// - **Example**: The "File", "Edit", "View" menu bar at the top of a text editor.
    MenuBar,

    /// Represents a vertical or horizontal scroll bar.
    /// - **Purpose**: To enable scrolling through content that is larger than the visible area.
    /// - **When to use**: For any scrollable region of content.
    /// - **Example**: The bar on the side of a web page that allows the user to scroll up and
    ///   down.
    ScrollBar,

    /// Represents a handle or grip used for moving or resizing.
    /// - **Purpose**: To provide a user interface element for manipulating another element's size
    ///   or position.
    /// - **When to use**: For handles that allow resizing of windows, panes, or other objects.
    /// - **Example**: The small textured area in the bottom-right corner of a window that can be
    ///   dragged to resize it.
    Grip,

    /// Represents a system sound indicating an event.
    /// - **Purpose**: To associate a sound with a UI event, providing an auditory cue.
    /// - **When to use**: When a sound is the primary representation of an event.
    /// - **Example**: A system notification sound that plays when a new message arrives.
    Sound,

    /// Represents the system's mouse pointer or other pointing device.
    /// - **Purpose**: To indicate the screen position of the user's pointing device.
    /// - **When to use**: This role is managed by the operating system.
    /// - **Example**: The arrow that moves on the screen as you move the mouse.
    Cursor,

    /// Represents the text insertion point indicator.
    /// - **Purpose**: To show the current text entry or editing position.
    /// - **When to use**: This role is typically managed by the operating system for text input
    ///   fields.
    /// - **Example**: The blinking vertical line in a text box that shows where the next character
    ///   will be typed.
    Caret,

    /// Represents an alert or notification.
    /// - **Purpose**: To convey an important, non-modal message to the user.
    /// - **When to use**: For non-intrusive notifications that do not require immediate user
    ///   interaction.
    /// - **Example**: A small, temporary "toast" notification that appears to confirm an action,
    ///   like "Email sent."
    Alert,

    /// Represents a window frame.
    /// - **Purpose**: To serve as the container for other objects like a title bar and client
    ///   area.
    /// - **When to use**: This is a fundamental role, typically managed by the windowing system.
    /// - **Example**: The main window of any application, which contains all other UI elements.
    Window,

    /// Represents a window's client area, where the main content is displayed.
    /// - **Purpose**: To define the primary content area of a window.
    /// - **When to use**: For the main content region of a window. It's often the default role for
    ///   a custom control container.
    /// - **Example**: The area of a web browser where the web page content is rendered.
    Client,

    /// Represents a pop-up menu.
    /// - **Purpose**: To display a list of `MenuItem` objects that appears when a user performs an
    ///   action.
    /// - **When to use**: For context menus (right-click menus) or drop-down menus.
    /// - **Example**: The menu that appears when you right-click on a file in a file explorer.
    MenuPopup,

    /// Represents an individual item within a menu.
    /// - **Purpose**: To represent a single command, option, or separator within a menu.
    /// - **When to use**: For individual options inside a `MenuBar` or `MenuPopup`.
    /// - **Example**: The "Save" option within the "File" menu.
    MenuItem,

    /// Represents a small pop-up window that provides information.
    /// - **Purpose**: To offer brief, contextual help or information about a UI element.
    /// - **When to use**: For informational pop-ups that appear on mouse hover.
    /// - **Example**: The small box of text that appears when you hover over a button in a
    ///   toolbar.
    Tooltip,

    /// Represents the main window of an application.
    /// - **Purpose**: To identify the top-level window of an application.
    /// - **When to use**: For the primary window that represents the application itself.
    /// - **Example**: The main window of a calculator or notepad application.
    Application,

    /// Represents a document window within an application.
    /// - **Purpose**: To represent a contained document, typically in a Multiple Document
    ///   Interface (MDI) application.
    /// - **When to use**: For individual document windows inside a larger application shell.
    /// - **Example**: In a photo editor that allows multiple images to be open in separate
    ///   windows, each image window would be a `Document`.
    Document,

    /// Represents a pane or a distinct section of a window.
    /// - **Purpose**: To divide a window into visually and functionally distinct areas.
    /// - **When to use**: For sub-regions of a window, like a navigation pane, preview pane, or
    ///   sidebar.
    /// - **Example**: The preview pane in an email client that shows the content of the selected
    ///   email.
    Pane,

    /// Represents a graphical chart or graph.
    /// - **Purpose**: To display data visually in a chart format.
    /// - **When to use**: For any type of chart, such as a bar chart, line chart, or pie chart.
    /// - **Example**: A bar chart displaying monthly sales figures.
    Chart,

    /// Represents a dialog box or message box.
    /// - **Purpose**: To create a secondary window that requires user interaction before returning
    ///   to the main application.
    /// - **When to use**: For modal or non-modal windows that prompt the user for information or a
    ///   response.
    /// - **Example**: The "Open File" or "Print" dialog in most applications.
    Dialog,

    /// Represents a window's border.
    /// - **Purpose**: To identify the border of a window, which is often used for resizing.
    /// - **When to use**: This role is typically managed by the windowing system.
    /// - **Example**: The decorative and functional frame around a window.
    Border,

    /// Represents a group of related controls.
    /// - **Purpose**: To logically group other objects that share a common purpose.
    /// - **When to use**: For grouping controls like a set of radio buttons or a fieldset with a
    ///   legend.
    /// - **Example**: A "Settings" group box in a dialog that contains several related checkboxes.
    Grouping,

    /// Represents a visual separator.
    /// - **Purpose**: To visually divide a space or a group of controls.
    /// - **When to use**: For visual separators in menus, toolbars, or between panes.
    /// - **Example**: The horizontal line in a menu that separates groups of related menu items.
    Separator,

    /// Represents a toolbar containing a group of controls.
    /// - **Purpose**: To group controls, typically buttons, for quick access to frequently used
    ///   functions.
    /// - **When to use**: For a bar of buttons or other controls, usually at the top of a window
    ///   or pane.
    /// - **Example**: The toolbar at the top of a word processor with buttons for "Bold,"
    ///   "Italic," and "Underline."
    Toolbar,

    /// Represents a status bar for displaying information.
    /// - **Purpose**: To display status information about the current state of the application.
    /// - **When to use**: For a bar, typically at the bottom of a window, that displays messages.
    /// - **Example**: The bar at the bottom of a web browser that shows the loading status of a
    ///   page.
    StatusBar,

    /// Represents a data table.
    /// - **Purpose**: To present data in a two-dimensional grid of rows and columns.
    /// - **When to use**: For grid-like data presentation.
    /// - **Example**: A spreadsheet or a table of data in a database application.
    Table,

    /// Represents a column header in a table.
    /// - **Purpose**: To provide a label for a column of data.
    /// - **When to use**: For the headers of columns in a `Table`.
    /// - **Example**: The header row in a spreadsheet with labels like "Name," "Date," and
    ///   "Amount."
    ColumnHeader,

    /// Represents a row header in a table.
    /// - **Purpose**: To provide a label for a row of data.
    /// - **When to use**: For the headers of rows in a `Table`.
    /// - **Example**: The numbered rows on the left side of a spreadsheet.
    RowHeader,

    /// Represents a full column of cells in a table.
    /// - **Purpose**: To represent an entire column as a single accessible object.
    /// - **When to use**: When it is useful to interact with a column as a whole.
    /// - **Example**: The "Amount" column in a financial data table.
    Column,

    /// Represents a full row of cells in a table.
    /// - **Purpose**: To represent an entire row as a single accessible object.
    /// - **When to use**: When it is useful to interact with a row as a whole.
    /// - **Example**: A row representing a single customer's information in a customer list.
    Row,

    /// Represents a single cell within a table.
    /// - **Purpose**: To represent a single data point or control within a `Table`.
    /// - **When to use**: For individual cells in a grid or table.
    /// - **Example**: A single cell in a spreadsheet containing a specific value.
    Cell,

    /// Represents a hyperlink to a resource.
    /// - **Purpose**: To provide a navigational link to another document or location.
    /// - **When to use**: For text or images that, when clicked, navigate to another resource.
    /// - **Example**: A clickable link on a web page.
    Link,

    /// Represents a help balloon or pop-up.
    /// - **Purpose**: To provide more detailed help information than a standard tooltip.
    /// - **When to use**: For a pop-up that offers extended help text, often initiated by a help
    ///   button.
    /// - **Example**: A pop-up balloon with a paragraph of help text that appears when a user
    ///   clicks a help icon.
    HelpBalloon,

    /// Represents an animated, character-like graphic object.
    /// - **Purpose**: To provide an animated agent for user assistance or entertainment.
    /// - **When to use**: For animated characters or avatars that provide help or guidance.
    /// - **Example**: An animated paperclip that offers tips in a word processor (e.g.,
    ///   Microsoft's Clippy).
    Character,

    /// Represents a list of items.
    /// - **Purpose**: To contain a set of `ListItem` objects.
    /// - **When to use**: For list boxes or similar controls that present a list of selectable
    ///   items.
    /// - **Example**: The list of files in a file selection dialog.
    List,

    /// Represents an individual item within a list.
    /// - **Purpose**: To represent a single, selectable item within a `List`.
    /// - **When to use**: For each individual item in a list box or combo box.
    /// - **Example**: A single file name in a list of files.
    ListItem,

    /// Represents an outline or tree structure.
    /// - **Purpose**: To display a hierarchical view of data.
    /// - **When to use**: For tree-view controls that show nested items.
    /// - **Example**: A file explorer's folder tree view.
    Outline,

    /// Represents an individual item within an outline or tree.
    /// - **Purpose**: To represent a single node (which can be a leaf or a branch) in an
    ///   `Outline`.
    /// - **When to use**: For each node in a tree view.
    /// - **Example**: A single folder in a file explorer's tree view.
    OutlineItem,

    /// Represents a single tab in a tabbed interface.
    /// - **Purpose**: To provide a control for switching between different `PropertyPage` views.
    /// - **When to use**: For the individual tabs that the user can click to switch pages.
    /// - **Example**: The "General" and "Security" tabs in a file properties dialog.
    PageTab,

    /// Represents the content of a page in a property sheet.
    /// - **Purpose**: To serve as a container for the controls displayed when a `PageTab` is
    ///   selected.
    /// - **When to use**: For the content area associated with a specific tab.
    /// - **Example**: The set of options displayed when the "Security" tab is active.
    PropertyPage,

    /// Represents a visual indicator, like a slider thumb.
    /// - **Purpose**: To visually indicate the current value or position of another control.
    /// - **When to use**: For a sub-element that indicates status, like the thumb of a scrollbar.
    /// - **Example**: The draggable thumb of a scrollbar that indicates the current scroll
    ///   position.
    Indicator,

    /// Represents a picture or graphical image.
    /// - **Purpose**: To display a non-interactive image.
    /// - **When to use**: For images and icons that are purely decorative or informational.
    /// - **Example**: A company logo displayed in an application's "About" dialog.
    Graphic,

    /// Represents read-only text.
    /// - **Purpose**: To provide a non-editable text label for another control or for displaying
    ///   information.
    /// - **When to use**: For text that the user cannot edit.
    /// - **Example**: The label "Username:" next to a text input field.
    StaticText,

    /// Represents editable text or a text area.
    /// - **Purpose**: To allow for user text input or selection.
    /// - **When to use**: For text input fields where the user can type.
    /// - **Example**: A text box for entering a username or password.
    Text,

    /// Represents a standard push button.
    /// - **Purpose**: To initiate an immediate action.
    /// - **When to use**: For standard buttons that perform an action when clicked.
    /// - **Example**: An "OK" or "Cancel" button in a dialog.
    PushButton,

    /// Represents a check box control.
    /// - **Purpose**: To allow the user to make a binary choice (checked or unchecked).
    /// - **When to use**: For options that can be toggled on or off independently.
    /// - **Example**: A "Remember me" checkbox on a login form.
    CheckButton,

    /// Represents a radio button.
    /// - **Purpose**: To allow the user to select one option from a mutually exclusive group.
    /// - **When to use**: For a choice where only one option from a `Grouping` can be selected.
    /// - **Example**: "Male" and "Female" radio buttons for selecting gender.
    RadioButton,

    /// Represents a combination of a text field and a drop-down list.
    /// - **Purpose**: To allow the user to either type a value or select one from a list.
    /// - **When to use**: For controls that offer a list of suggestions but also allow custom
    ///   input.
    /// - **Example**: A font selector that allows you to type a font name or choose one from a
    ///   list.
    ComboBox,

    /// Represents a drop-down list box.
    /// - **Purpose**: To allow the user to select an item from a non-editable list that drops
    ///   down.
    /// - **When to use**: For selecting a single item from a predefined list of options.
    /// - **Example**: A country selection drop-down menu.
    DropList,

    /// Represents a progress bar.
    /// - **Purpose**: To indicate the progress of a lengthy operation.
    /// - **When to use**: To provide feedback for tasks like file downloads or installations.
    /// - **Example**: The bar that fills up to show the progress of a file copy operation.
    ProgressBar,

    /// Represents a dial or knob.
    /// - **Purpose**: To allow selecting a value from a continuous or discrete range, often
    ///   circularly.
    /// - **When to use**: For controls that resemble real-world dials, like a volume knob.
    /// - **Example**: A volume control knob in a media player application.
    Dial,

    /// Represents a control for entering a keyboard shortcut.
    /// - **Purpose**: To capture a key combination from the user.
    /// - **When to use**: In settings where users can define their own keyboard shortcuts.
    /// - **Example**: A text field in a settings dialog where a user can press a key combination
    ///   to assign it to a command.
    HotkeyField,

    /// Represents a slider for selecting a value within a range.
    /// - **Purpose**: To allow the user to adjust a setting along a continuous or discrete range.
    /// - **When to use**: For adjusting values like volume, brightness, or zoom level.
    /// - **Example**: A slider to control the volume of a video.
    Slider,

    /// Represents a spin button (up/down arrows) for incrementing or decrementing a value.
    /// - **Purpose**: To provide fine-tuned adjustment of a value, typically numeric.
    /// - **When to use**: For controls that allow stepping through a range of values.
    /// - **Example**: The up and down arrows next to a number input for setting the font size.
    SpinButton,

    /// Represents a diagram or flowchart.
    /// - **Purpose**: To represent data or relationships in a schematic form.
    /// - **When to use**: For visual representations of structures that are not charts, like a
    ///   database schema diagram.
    /// - **Example**: A flowchart illustrating a business process.
    Diagram,

    /// Represents an animation control.
    /// - **Purpose**: To display a sequence of images or indicate an ongoing process.
    /// - **When to use**: For animations that show that an operation is in progress.
    /// - **Example**: The animation that plays while files are being copied.
    Animation,

    /// Represents a mathematical equation.
    /// - **Purpose**: To display a mathematical formula in the correct format.
    /// - **When to use**: For displaying mathematical equations.
    /// - **Example**: A rendered mathematical equation in a scientific document editor.
    Equation,

    /// Represents a button that drops down a list of items.
    /// - **Purpose**: To combine a default action button with a list of alternative actions.
    /// - **When to use**: For buttons that have a primary action and a secondary list of options.
    /// - **Example**: A "Send" button with a dropdown arrow that reveals "Send and Archive."
    ButtonDropdown,

    /// Represents a button that drops down a full menu.
    /// - **Purpose**: To provide a button that opens a menu of choices rather than performing a
    ///   single action.
    /// - **When to use**: When a button's primary purpose is to reveal a menu.
    /// - **Example**: A "Tools" button that opens a menu with various tool options.
    ButtonMenu,

    /// Represents a button that drops down a grid for selection.
    /// - **Purpose**: To allow selection from a two-dimensional grid of options.
    /// - **When to use**: For buttons that open a grid-based selection UI.
    /// - **Example**: A color picker button that opens a grid of color swatches.
    ButtonDropdownGrid,

    /// Represents blank space between other objects.
    /// - **Purpose**: To represent significant empty areas in a UI that are part of the layout.
    /// - **When to use**: Sparingly, to signify that a large area is intentionally blank.
    /// - **Example**: A large empty panel in a complex layout might use this role.
    Whitespace,

    /// Represents the container for a set of tabs.
    /// - **Purpose**: To group a set of `PageTab` elements.
    /// - **When to use**: To act as the parent container for a row or column of tabs.
    /// - **Example**: The entire row of tabs at the top of a properties dialog.
    PageTabList,

    /// Represents a clock control.
    /// - **Purpose**: To display the current time.
    /// - **When to use**: For any UI element that displays time.
    /// - **Example**: The clock in the system tray of the operating system.
    Clock,

    /// Represents a button with two parts: a default action and a dropdown.
    /// - **Purpose**: To combine a frequently used action with a set of related, less-used
    ///   actions.
    /// - **When to use**: When a button has a default action and other related actions available
    ///   in a dropdown.
    /// - **Example**: A "Save" split button where the primary part saves, and the dropdown offers
    ///   "Save As."
    SplitButton,

    /// Represents a control for entering an IP address.
    /// - **Purpose**: To provide a specialized input field for IP addresses, often with formatting
    ///   and validation.
    /// - **When to use**: For dedicated IP address input fields.
    /// - **Example**: A network configuration dialog with a field for entering a static IP
    ///   address.
    IpAddress,

    /// Represents an element with no specific role.
    /// - **Purpose**: To indicate an element that has no semantic meaning for accessibility.
    /// - **When to use**: Should be used sparingly for purely decorative elements that should be
    ///   ignored by assistive technologies.
    /// - **Example**: A decorative graphical flourish that has no function or information to
    ///   convey.
    Nothing,

    /// Unknown or unspecified role.
    /// - **Purpose**: Default fallback when no specific role is assigned.
    /// - **When to use**: As a default value or when role information is unavailable.
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
    /// - **Purpose**: To indicate that a control is disabled or grayed out.
    /// - **When to use**: For disabled buttons, non-interactive menu items, or any control that is
    ///   temporarily non-functional.
    /// - **Example**: A "Save" button that is disabled until the user makes changes to a document.
    Unavailable,

    /// The element is selected.
    /// - **Purpose**: To indicate that an item is currently chosen or highlighted. This is
    ///   distinct from having focus.
    /// - **When to use**: For selected items in a list, highlighted text, or the currently active
    ///   tab in a tab list.
    /// - **Example**: A file highlighted in a file explorer, or multiple selected emails in an
    ///   inbox.
    Selected,

    /// The element has the keyboard focus.
    /// - **Purpose**: To identify the single element that will receive keyboard input.
    /// - **When to use**: For the control that is currently active and ready to be manipulated by
    ///   the keyboard.
    /// - **Example**: A text box with a blinking cursor, or a button with a dotted outline around
    ///   it.
    Focused,

    /// The element is checked, toggled, or in an "on" state.
    /// - **Purpose**: To represent checked checkboxes, selected radio buttons, and active toggles.
    /// - **Example**: A checked "I agree" checkbox, a selected "Yes" radio button.
    CheckedTrue,
    /// The element is unchecked, untoggled, or in an "off" state.
    /// - **Purpose**: To explicitly represent an unchecked checkbox or unselected radio button.
    /// - **Example**: An unchecked checkbox that the user has not yet ticked.
    CheckedFalse,

    /// The element's content cannot be edited by the user.
    /// - **Purpose**: To indicate that the element's value can be viewed and copied, but not
    ///   modified.
    /// - **When to use**: For display-only text fields or documents.
    /// - **Example**: A text box displaying a license agreement that the user can scroll through
    ///   but cannot edit.
    Readonly,

    /// The element is the default action in a dialog or form.
    /// - **Purpose**: To identify the button that will be activated if the user presses the Enter
    ///   key.
    /// - **When to use**: For the primary confirmation button in a dialog.
    /// - **Example**: The "OK" button in a dialog box, which often has a thicker or colored
    ///   border.
    Default,

    /// The element is expanded, showing its child items.
    /// - **Purpose**: To indicate that a collapsible element is currently open and its contents
    ///   are visible.
    /// - **When to use**: For tree view nodes, combo boxes with their lists open, or expanded
    ///   accordion panels.
    /// - **Example**: A folder in a file explorer's tree view that has been clicked to show its
    ///   subfolders.
    Expanded,

    /// The element is collapsed, hiding its child items.
    /// - **Purpose**: To indicate that a collapsible element is closed and its contents are
    ///   hidden.
    /// - **When to use**: The counterpart to `Expanded` for any collapsible UI element.
    /// - **Example**: A closed folder in a file explorer's tree view, hiding its contents.
    Collapsed,

    /// The element is busy and cannot respond to user interaction.
    /// - **Purpose**: To indicate that the element or application is performing an operation and
    ///   is temporarily unresponsive.
    /// - **When to use**: When an application is loading, processing data, or otherwise occupied.
    /// - **Example**: A window that is grayed out and shows a spinning cursor while saving a large
    ///   file.
    Busy,

    /// The element is not currently visible on the screen.
    /// - **Purpose**: To indicate that an element exists but is currently scrolled out of the
    ///   visible area.
    /// - **When to use**: For items in a long list or a large document that are not within the
    ///   current viewport.
    /// - **Example**: A list item in a long dropdown that you would have to scroll down to see.
    Offscreen,

    /// The element can accept keyboard focus.
    /// - **Purpose**: To indicate that the user can navigate to this element using the keyboard
    ///   (e.g., with the Tab key).
    /// - **When to use**: On all interactive elements like buttons, links, and input fields,
    ///   whether they currently have focus or not.
    /// - **Example**: A button that can receive focus, even if it is not the currently focused
    ///   element.
    Focusable,

    /// The element is a container whose children can be selected.
    /// - **Purpose**: To indicate that the element contains items that can be chosen.
    /// - **When to use**: On container controls like list boxes, tree views, or text spans where
    ///   text can be highlighted.
    /// - **Example**: A list box control is `Selectable`, while its individual list items have the
    ///   `Selected` state when chosen.
    Selectable,

    /// The element is a hyperlink.
    /// - **Purpose**: To identify an object that navigates to another resource or location when
    ///   activated.
    /// - **When to use**: On any object that functions as a hyperlink.
    /// - **Example**: Text or an image that, when clicked, opens a web page.
    Linked,

    /// The element is a hyperlink that has been visited.
    /// - **Purpose**: To indicate that a hyperlink has already been followed by the user.
    /// - **When to use**: On a `Linked` object that the user has previously activated.
    /// - **Example**: A hyperlink on a web page that has changed color to show it has been
    ///   visited.
    Traversed,

    /// The element allows multiple of its children to be selected at once.
    /// - **Purpose**: To indicate that a container control supports multi-selection.
    /// - **When to use**: On container controls like list boxes or file explorers that support
    ///   multiple selections (e.g., with Ctrl-click).
    /// - **Example**: A file list that allows the user to select several files at once for a copy
    ///   operation.
    Multiselectable,

    /// The element contains protected content that should not be read aloud.
    /// - **Purpose**: To prevent assistive technologies from speaking the content of a sensitive
    ///   field.
    /// - **When to use**: Primarily for password input fields.
    /// - **Example**: A password text box where typed characters are masked with asterisks or
    ///   dots.
    Protected,
}

impl_option!(
    AccessibilityState,
    OptionAccessibilityState,
    [Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash]
);

impl_vec!(AccessibilityState, AccessibilityStateVec, AccessibilityStateVecDestructor, AccessibilityStateVecDestructorType, AccessibilityStateVecSlice, OptionAccessibilityState);
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
    pub fn label<S: Into<AzString>>(text: S) -> Self {
        Self {
            label: OptionString::Some(text.into()),
            role: OptionAccessibilityRole::None,
            description: OptionString::None,
        }
    }

    #[must_use] pub const fn with_role(mut self, role: AccessibilityRole) -> Self {
        self.role = OptionAccessibilityRole::Some(role);
        self
    }

    #[must_use] pub fn with_description<S: Into<AzString>>(mut self, desc: S) -> Self {
        self.description = OptionString::Some(desc.into());
        self
    }

    /// Convert to full `AccessibilityInfo`
    #[must_use] pub fn to_full_info(&self) -> AccessibilityInfo {
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
    #[must_use] pub const fn create(label: AzString) -> Self {
        Self {
            label: OptionString::Some(label),
            current_value: OptionF32::None,
            max: OptionF32::None,
            indeterminate: false,
            description: OptionString::None,
        }
    }

    /// Returns a copy with the given current value.
    #[must_use] pub const fn with_current_value(mut self, value: f32) -> Self {
        self.current_value = OptionF32::Some(value);
        self
    }

    /// Returns a copy with the given maximum value.
    #[must_use] pub const fn with_max(mut self, max: f32) -> Self {
        self.max = OptionF32::Some(max);
        self
    }

    /// Returns a copy with the indeterminate flag set.
    #[must_use] pub const fn with_indeterminate(mut self, indeterminate: bool) -> Self {
        self.indeterminate = indeterminate;
        self
    }

    /// Returns a copy with the given description.
    #[must_use] pub fn with_description(mut self, desc: AzString) -> Self {
        self.description = OptionString::Some(desc);
        self
    }

    /// Convert to full `AccessibilityInfo` so the value can be installed on a node.
    #[must_use] pub fn to_full_info(&self) -> AccessibilityInfo {
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
    #[must_use] pub const fn create(label: AzString, current_value: f32, min: f32, max: f32) -> Self {
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
    #[must_use] pub const fn with_low(mut self, low: f32) -> Self {
        self.low = OptionF32::Some(low);
        self
    }

    /// Returns a copy with the given high threshold.
    #[must_use] pub const fn with_high(mut self, high: f32) -> Self {
        self.high = OptionF32::Some(high);
        self
    }

    /// Returns a copy with the given optimum value.
    #[must_use] pub const fn with_optimum(mut self, optimum: f32) -> Self {
        self.optimum = OptionF32::Some(optimum);
        self
    }

    /// Returns a copy with the given description.
    #[must_use] pub fn with_description(mut self, desc: AzString) -> Self {
        self.description = OptionString::Some(desc);
        self
    }

    /// Convert to full `AccessibilityInfo` so the value can be installed on a node.
    #[must_use] pub fn to_full_info(&self) -> AccessibilityInfo {
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
    #[must_use] pub const fn create(label: AzString) -> Self {
        Self {
            label: OptionString::Some(label),
            modal: false,
            described_by: OptionString::None,
            role: AccessibilityRole::Dialog,
            description: OptionString::None,
        }
    }

    /// Returns a copy with the given modality flag.
    #[must_use] pub const fn with_modal(mut self, modal: bool) -> Self {
        self.modal = modal;
        self
    }

    /// Returns a copy with `aria-describedby` pointing at the given node ID.
    #[must_use] pub fn with_described_by(mut self, described_by: AzString) -> Self {
        self.described_by = OptionString::Some(described_by);
        self
    }

    /// Returns a copy with the given role (defaults to `Dialog`).
    #[must_use] pub const fn with_role(mut self, role: AccessibilityRole) -> Self {
        self.role = role;
        self
    }

    /// Returns a copy with the given inline description.
    #[must_use] pub fn with_description(mut self, desc: AzString) -> Self {
        self.description = OptionString::Some(desc);
        self
    }

    /// Convert to full `AccessibilityInfo` so the value can be installed on a node.
    #[must_use] pub fn to_full_info(&self) -> AccessibilityInfo {
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
mod autotest_generated {
    use super::*;
    use alloc::string::String;

    // ---- small helpers to reach into the FFI-style option wrappers ----

    fn name_str(o: &OptionString) -> Option<&str> {
        o.as_ref().map(|s| s.as_str())
    }

    fn f32_of(o: &OptionF32) -> Option<f32> {
        o.as_ref().copied()
    }

    /// A battery of adversarial strings: empty, embedded NUL, control chars,
    /// combining unicode, emoji, RTL, and a very large allocation.
    fn adversarial_strings() -> Vec<String> {
        vec![
            String::new(),
            String::from(" "),
            String::from("\0"),
            String::from("a\0b\0c"),
            String::from("\t\r\n\x1b[0m"),
            String::from("日本語のテキスト"),
            String::from("🎉👨‍👩‍👧‍👦🇺🇳"),
            String::from("e\u{0301}\u{0301}\u{0301}"), // combining accents
            String::from("\u{202e}reversed\u{202d}"),  // RTL override
            String::from("\u{FFFD}\u{10FFFF}"),        // replacement + max scalar
            "x".repeat(100_000),                        // huge
        ]
    }

    /// Numeric edge values for f32 fields.
    fn adversarial_f32() -> Vec<f32> {
        vec![
            0.0,
            -0.0,
            1.0,
            -1.0,
            f32::MIN,
            f32::MAX,
            f32::MIN_POSITIVE,
            -f32::MIN_POSITIVE,
            f32::EPSILON,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ]
    }

    // =====================================================================
    // 1. SmallAriaInfo::label — no_panic_smoke
    // =====================================================================

    #[test]
    fn small_label_no_panic_smoke() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let info = SmallAriaInfo::label(s);
            // The label must round-trip verbatim and the other fields default to None.
            assert_eq!(name_str(&info.label), Some(expected.as_str()));
            assert!(info.role.is_none());
            assert!(info.description.is_none());
            // to_full_info must not panic even for pathological labels.
            let full = info.to_full_info();
            assert_eq!(name_str(&full.accessibility_name), Some(expected.as_str()));
        }
        // `&str` input path as well.
        let info = SmallAriaInfo::label("hello");
        assert_eq!(name_str(&info.label), Some("hello"));
    }

    // =====================================================================
    // 2. SmallAriaInfo::with_role — no_panic + invariants
    // =====================================================================

    fn representative_roles() -> Vec<AccessibilityRole> {
        vec![
            AccessibilityRole::TitleBar, // first variant
            AccessibilityRole::PushButton,
            AccessibilityRole::CheckButton,
            AccessibilityRole::Slider,
            AccessibilityRole::Link,
            AccessibilityRole::Nothing,
            AccessibilityRole::Unknown, // last variant
        ]
    }

    #[test]
    fn small_with_role_invariants() {
        for role in representative_roles() {
            let info = SmallAriaInfo::label("base").with_role(role);
            // Only the role field changes; label preserved, description untouched.
            assert_eq!(info.role, OptionAccessibilityRole::Some(role));
            assert_eq!(name_str(&info.label), Some("base"));
            assert!(info.description.is_none());
        }
        // Last-write-wins when applied twice.
        let info = SmallAriaInfo::label("x")
            .with_role(AccessibilityRole::Link)
            .with_role(AccessibilityRole::Slider);
        assert_eq!(info.role, OptionAccessibilityRole::Some(AccessibilityRole::Slider));
    }

    // =====================================================================
    // 3. SmallAriaInfo::with_description — no_panic + invariants
    // =====================================================================

    #[test]
    fn small_with_description_invariants() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let info = SmallAriaInfo::label("base").with_description(s);
            assert_eq!(name_str(&info.description), Some(expected.as_str()));
            // label untouched, role still None.
            assert_eq!(name_str(&info.label), Some("base"));
            assert!(info.role.is_none());
        }
    }

    // =====================================================================
    // 4. SmallAriaInfo::to_full_info — basic + edge
    // =====================================================================

    #[test]
    fn small_to_full_info_basic() {
        let info = SmallAriaInfo::label("Submit")
            .with_role(AccessibilityRole::PushButton)
            .with_description("primary action")
            .to_full_info();
        assert_eq!(name_str(&info.accessibility_name), Some("Submit"));
        assert_eq!(info.role, AccessibilityRole::PushButton);
        assert_eq!(name_str(&info.description), Some("primary action"));
        assert!(info.accessibility_value.is_none());
        assert_eq!(info.states.len(), 0);
        assert_eq!(info.supported_actions.len(), 0);
        assert!(!info.is_live_region);
        assert!(info.labelled_by.is_none());
        assert!(info.described_by.is_none());
    }

    #[test]
    fn small_to_full_info_edge_missing_role_maps_to_unknown() {
        // No role set => full info must fall back to `Unknown`, never panic.
        let info = SmallAriaInfo::label("").to_full_info();
        assert_eq!(info.role, AccessibilityRole::Unknown);
        assert_eq!(name_str(&info.accessibility_name), Some(""));
        assert!(info.description.is_none());
    }

    // =====================================================================
    // 5. ProgressAriaInfo::create — no_panic_smoke
    // =====================================================================

    #[test]
    fn progress_create_no_panic_smoke() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let p = ProgressAriaInfo::create(s.into());
            assert_eq!(name_str(&p.label), Some(expected.as_str()));
            // Documented defaults.
            assert!(p.current_value.is_none());
            assert!(p.max.is_none());
            assert!(!p.indeterminate);
            assert!(p.description.is_none());
        }
    }

    // =====================================================================
    // 6. ProgressAriaInfo::with_current_value — no_panic + invariants (numeric)
    // =====================================================================

    #[test]
    fn progress_with_current_value_numeric() {
        for v in adversarial_f32() {
            let p = ProgressAriaInfo::create("p".into()).with_current_value(v);
            match f32_of(&p.current_value) {
                Some(got) if v.is_nan() => assert!(got.is_nan()),
                Some(got) => assert_eq!(got, v),
                None => panic!("current_value should be Some after with_current_value"),
            }
            // to_full_info must not panic for any float, and (since determinate)
            // must emit a value string.
            let full = p.to_full_info();
            assert!(full.accessibility_value.is_some());
        }
    }

    // =====================================================================
    // 7. ProgressAriaInfo::with_max — no_panic + invariants (numeric)
    // =====================================================================

    #[test]
    fn progress_with_max_numeric() {
        for v in adversarial_f32() {
            let p = ProgressAriaInfo::create("p".into()).with_max(v);
            match f32_of(&p.max) {
                Some(got) if v.is_nan() => assert!(got.is_nan()),
                Some(got) => assert_eq!(got, v),
                None => panic!("max should be Some after with_max"),
            }
            // max does not influence the value string; current_value stays None.
            assert!(p.current_value.is_none());
        }
    }

    // =====================================================================
    // 8. ProgressAriaInfo::with_indeterminate — no_panic + invariants
    // =====================================================================

    #[test]
    fn progress_with_indeterminate_invariants() {
        for flag in [true, false] {
            let p = ProgressAriaInfo::create("p".into()).with_indeterminate(flag);
            assert_eq!(p.indeterminate, flag);
        }
        // indeterminate must override a present current_value in to_full_info.
        let p = ProgressAriaInfo::create("p".into())
            .with_current_value(0.5)
            .with_indeterminate(true);
        assert!(p.to_full_info().accessibility_value.is_none());
    }

    // =====================================================================
    // 9. ProgressAriaInfo::with_description — no_panic + invariants
    // =====================================================================

    #[test]
    fn progress_with_description_invariants() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let p = ProgressAriaInfo::create("p".into()).with_description(s.into());
            assert_eq!(name_str(&p.description), Some(expected.as_str()));
            assert_eq!(name_str(&p.label), Some("p"));
        }
    }

    // =====================================================================
    // 10. ProgressAriaInfo::to_full_info — basic + edge
    // =====================================================================

    #[test]
    fn progress_to_full_info_basic() {
        let info = ProgressAriaInfo::create("Loading".into())
            .with_current_value(0.5)
            .to_full_info();
        assert_eq!(name_str(&info.accessibility_name), Some("Loading"));
        assert_eq!(info.role, AccessibilityRole::ProgressBar);
        assert_eq!(name_str(&info.accessibility_value), Some("0.5"));
        assert_eq!(info.states.len(), 0);
        assert_eq!(info.supported_actions.len(), 0);
    }

    #[test]
    fn progress_to_full_info_edge() {
        // No current value => value string is None.
        let info = ProgressAriaInfo::create("x".into()).to_full_info();
        assert!(info.accessibility_value.is_none());
        assert_eq!(info.role, AccessibilityRole::ProgressBar);

        // NaN / inf current values format to defined strings, no panic.
        assert_eq!(
            name_str(
                &ProgressAriaInfo::create("x".into())
                    .with_current_value(f32::NAN)
                    .to_full_info()
                    .accessibility_value
            ),
            Some("NaN")
        );
        assert_eq!(
            name_str(
                &ProgressAriaInfo::create("x".into())
                    .with_current_value(f32::INFINITY)
                    .to_full_info()
                    .accessibility_value
            ),
            Some("inf")
        );
        assert_eq!(
            name_str(
                &ProgressAriaInfo::create("x".into())
                    .with_current_value(f32::NEG_INFINITY)
                    .to_full_info()
                    .accessibility_value
            ),
            Some("-inf")
        );
    }

    // =====================================================================
    // 11. MeterAriaInfo::create — numeric (zero / min_max / negative / nan_inf)
    // =====================================================================

    #[test]
    fn meter_create_zero() {
        let m = MeterAriaInfo::create("z".into(), 0.0, 0.0, 0.0);
        assert_eq!(m.current_value, 0.0);
        assert_eq!(m.min, 0.0);
        assert_eq!(m.max, 0.0);
        assert_eq!(name_str(&m.to_full_info().accessibility_value), Some("0"));
    }

    #[test]
    fn meter_create_min_max() {
        let m = MeterAriaInfo::create("mm".into(), f32::MAX, f32::MIN, f32::MAX);
        assert_eq!(m.current_value, f32::MAX);
        assert_eq!(m.min, f32::MIN);
        assert_eq!(m.max, f32::MAX);
        // Formatting an extreme (but finite) float must not panic.
        assert!(m.to_full_info().accessibility_value.is_some());
    }

    #[test]
    fn meter_create_negative() {
        let m = MeterAriaInfo::create("neg".into(), -5.0, -10.0, -1.0);
        assert_eq!(m.current_value, -5.0);
        assert_eq!(m.min, -10.0);
        assert_eq!(m.max, -1.0);
        assert_eq!(name_str(&m.to_full_info().accessibility_value), Some("-5"));
        // Inverted range (min > max) is accepted verbatim; no panic, no clamping.
        let inv = MeterAriaInfo::create("inv".into(), 5.0, 100.0, 0.0);
        assert_eq!(inv.min, 100.0);
        assert_eq!(inv.max, 0.0);
        assert!(inv.to_full_info().accessibility_value.is_some());
    }

    #[test]
    fn meter_create_overflow_saturates_to_inf() {
        // f32 arithmetic saturates rather than panicking; feed the saturated
        // result straight in and confirm formatting stays defined.
        let over = f32::MAX * 2.0; // == +inf
        assert!(over.is_infinite());
        let m = MeterAriaInfo::create("o".into(), over, -over, over);
        assert_eq!(name_str(&m.to_full_info().accessibility_value), Some("inf"));
    }

    #[test]
    fn meter_create_nan_inf() {
        // NaN preserved as NaN, no panic constructing or formatting.
        let m = MeterAriaInfo::create("n".into(), f32::NAN, 0.0, 1.0);
        assert!(m.current_value.is_nan());
        assert_eq!(name_str(&m.to_full_info().accessibility_value), Some("NaN"));

        let pos = MeterAriaInfo::create("n".into(), f32::INFINITY, 0.0, 1.0);
        assert_eq!(name_str(&pos.to_full_info().accessibility_value), Some("inf"));

        let neg = MeterAriaInfo::create("n".into(), f32::NEG_INFINITY, 0.0, 1.0);
        assert_eq!(name_str(&neg.to_full_info().accessibility_value), Some("-inf"));

        // Non-finite bounds must not panic either.
        let bounds = MeterAriaInfo::create("n".into(), 0.5, f32::NEG_INFINITY, f32::INFINITY);
        assert!(bounds.min.is_infinite());
        assert!(bounds.max.is_infinite());
        assert!(bounds.to_full_info().accessibility_value.is_some());
    }

    // =====================================================================
    // 12-14. MeterAriaInfo::with_low / with_high / with_optimum — numeric invariants
    // =====================================================================

    #[test]
    fn meter_with_low_high_optimum_numeric() {
        for v in adversarial_f32() {
            let m = MeterAriaInfo::create("m".into(), 0.5, 0.0, 1.0)
                .with_low(v)
                .with_high(v)
                .with_optimum(v);
            for opt in [&m.low, &m.high, &m.optimum] {
                match f32_of(opt) {
                    Some(got) if v.is_nan() => assert!(got.is_nan()),
                    Some(got) => assert_eq!(got, v),
                    None => panic!("threshold should be Some after builder"),
                }
            }
            // Core value/range untouched by the threshold builders.
            assert_eq!(m.current_value, 0.5);
            assert_eq!(m.min, 0.0);
            assert_eq!(m.max, 1.0);
        }
    }

    // =====================================================================
    // 15. MeterAriaInfo::with_description — no_panic + invariants
    // =====================================================================

    #[test]
    fn meter_with_description_invariants() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let m = MeterAriaInfo::create("m".into(), 1.0, 0.0, 2.0).with_description(s.into());
            assert_eq!(name_str(&m.description), Some(expected.as_str()));
            assert_eq!(m.current_value, 1.0);
        }
    }

    // =====================================================================
    // 16. MeterAriaInfo::to_full_info — basic + edge
    // =====================================================================

    #[test]
    fn meter_to_full_info_basic() {
        let info = MeterAriaInfo::create("Disk".into(), 42.0, 0.0, 100.0)
            .with_description("usage".into())
            .to_full_info();
        assert_eq!(name_str(&info.accessibility_name), Some("Disk"));
        assert_eq!(info.role, AccessibilityRole::Indicator);
        assert_eq!(name_str(&info.accessibility_value), Some("42"));
        assert_eq!(name_str(&info.description), Some("usage"));
        assert_eq!(info.states.len(), 0);
        assert_eq!(info.supported_actions.len(), 0);
    }

    #[test]
    fn meter_to_full_info_edge() {
        // Meter always emits a value string (unlike progress). Even for an
        // extreme/empty-label instance it must not panic.
        let info = MeterAriaInfo::create("".into(), f32::MIN, f32::MIN, f32::MAX).to_full_info();
        assert!(info.accessibility_value.is_some());
        assert_eq!(info.role, AccessibilityRole::Indicator);
        assert_eq!(name_str(&info.accessibility_name), Some(""));
    }

    // =====================================================================
    // 17. DialogAriaInfo::create — no_panic_smoke
    // =====================================================================

    #[test]
    fn dialog_create_no_panic_smoke() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let d = DialogAriaInfo::create(s.into());
            assert_eq!(name_str(&d.label), Some(expected.as_str()));
            // Documented defaults: non-modal, role Dialog, no describers.
            assert!(!d.modal);
            assert_eq!(d.role, AccessibilityRole::Dialog);
            assert!(d.described_by.is_none());
            assert!(d.description.is_none());
        }
    }

    // =====================================================================
    // 18. DialogAriaInfo::with_modal — no_panic + invariants
    // =====================================================================

    #[test]
    fn dialog_with_modal_invariants() {
        for flag in [true, false] {
            let d = DialogAriaInfo::create("t".into()).with_modal(flag);
            assert_eq!(d.modal, flag);
            // Unrelated fields keep their defaults.
            assert_eq!(d.role, AccessibilityRole::Dialog);
            assert_eq!(name_str(&d.label), Some("t"));
        }
    }

    // =====================================================================
    // 19. DialogAriaInfo::with_described_by — no_panic + invariants
    // =====================================================================

    #[test]
    fn dialog_with_described_by_invariants() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let d = DialogAriaInfo::create("t".into()).with_described_by(s.into());
            assert_eq!(name_str(&d.described_by), Some(expected.as_str()));
            assert_eq!(name_str(&d.label), Some("t"));
        }
    }

    // =====================================================================
    // 20. DialogAriaInfo::with_role — no_panic + invariants
    // =====================================================================

    #[test]
    fn dialog_with_role_invariants() {
        for role in representative_roles() {
            let d = DialogAriaInfo::create("t".into()).with_role(role);
            assert_eq!(d.role, role);
            assert!(!d.modal);
        }
        // to_full_info propagates the overridden role verbatim.
        let info = DialogAriaInfo::create("t".into())
            .with_role(AccessibilityRole::Alert)
            .to_full_info();
        assert_eq!(info.role, AccessibilityRole::Alert);
    }

    // =====================================================================
    // 21. DialogAriaInfo::with_description — no_panic + invariants
    // =====================================================================

    #[test]
    fn dialog_with_description_invariants() {
        for s in adversarial_strings() {
            let expected = s.clone();
            let d = DialogAriaInfo::create("t".into()).with_description(s.into());
            assert_eq!(name_str(&d.description), Some(expected.as_str()));
            assert_eq!(name_str(&d.label), Some("t"));
        }
    }

    // =====================================================================
    // 22. DialogAriaInfo::to_full_info — basic + edge
    // =====================================================================

    #[test]
    fn dialog_to_full_info_basic() {
        let info = DialogAriaInfo::create("Confirm".into())
            .with_modal(true)
            .with_role(AccessibilityRole::Alert)
            .with_described_by("body-node".into())
            .with_description("Are you sure?".into())
            .to_full_info();
        assert_eq!(name_str(&info.accessibility_name), Some("Confirm"));
        assert_eq!(info.role, AccessibilityRole::Alert);
        assert_eq!(name_str(&info.description), Some("Are you sure?"));
        // The string `described_by` node-ref is NOT propagated into the DomNodeId field.
        assert!(info.described_by.is_none());
        assert!(info.labelled_by.is_none());
        assert!(info.accessibility_value.is_none());
        assert_eq!(info.states.len(), 0);
        assert_eq!(info.supported_actions.len(), 0);
    }

    #[test]
    fn dialog_to_full_info_edge() {
        // Default (non-modal, empty) instance must convert without panic.
        let info = DialogAriaInfo::create("".into()).to_full_info();
        assert_eq!(info.role, AccessibilityRole::Dialog);
        assert_eq!(name_str(&info.accessibility_name), Some(""));
        assert!(info.description.is_none());
    }

    // #####################################################################
    // Appended: round-trip, total-order and FFI-vec coverage.
    //
    // The block above exercises the 22 listed builder/getter fns. What it
    // does NOT cover is the machinery those fns feed into: the FFI vec/option
    // wrappers, and the `Eq`/`Ord`/`Hash` impls that `AccessibilityInfo`
    // derives *through* f32-carrying payloads (`LogicalPosition`,
    // `FloatValue`). Those derives are where a total-order contract can
    // silently break, so they get the adversarial treatment here.
    // #####################################################################

    use core::hash::{Hash, Hasher};

    use crate::{
        dom::{DomId, DomNodeId},
        styled_dom::NodeHierarchyItemId,
    };

    /// FNV-1a. Hand-rolled rather than `DefaultHasher` so these tests still
    /// build when azul-core is compiled `--no-default-features` (i.e. `no_std`,
    /// where `std::collections::hash_map` does not exist).
    struct Fnv(u64);

    impl Default for Fnv {
        fn default() -> Self {
            Self(0xcbf2_9ce4_8422_2325) // offset basis
        }
    }

    impl Hasher for Fnv {
        fn finish(&self) -> u64 {
            self.0
        }
        fn write(&mut self, bytes: &[u8]) {
            for b in bytes {
                self.0 ^= u64::from(*b);
                self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3); // FNV prime
            }
        }
    }

    fn hash_of<T: Hash>(t: &T) -> u64 {
        let mut h = Fnv::default();
        t.hash(&mut h);
        h.finish()
    }

    /// Every `AccessibilityRole`, in declaration order.
    ///
    /// `Ord` is derived, so declaration order *is* the sort order — the tests
    /// below pin that. Kept in sync with the enum by `role_exhaustiveness_canary`.
    fn all_roles() -> Vec<AccessibilityRole> {
        use AccessibilityRole::*;
        vec![
            TitleBar, MenuBar, ScrollBar, Grip, Sound, Cursor, Caret, Alert, Window, Client,
            MenuPopup, MenuItem, Tooltip, Application, Document, Pane, Chart, Dialog, Border,
            Grouping, Separator, Toolbar, StatusBar, Table, ColumnHeader, RowHeader, Column, Row,
            Cell, Link, HelpBalloon, Character, List, ListItem, Outline, OutlineItem, PageTab,
            PropertyPage, Indicator, Graphic, StaticText, Text, PushButton, CheckButton,
            RadioButton, ComboBox, DropList, ProgressBar, Dial, HotkeyField, Slider, SpinButton,
            Diagram, Animation, Equation, ButtonDropdown, ButtonMenu, ButtonDropdownGrid,
            Whitespace, PageTabList, Clock, SplitButton, IpAddress, Nothing, Unknown,
        ]
    }

    /// Every `AccessibilityState`, in declaration order.
    fn all_states() -> Vec<AccessibilityState> {
        use AccessibilityState::*;
        vec![
            Unavailable, Selected, Focused, CheckedTrue, CheckedFalse, Readonly, Default, Expanded,
            Collapsed, Busy, Offscreen, Focusable, Selectable, Linked, Traversed, Multiselectable,
            Protected,
        ]
    }

    /// Exhaustive `match`es: if a variant is added upstream without being added
    /// to `all_roles()` / `all_states()`, this stops compiling. That is the
    /// point — it keeps the ordering tests below honest instead of letting them
    /// silently degrade into partial coverage.
    #[test]
    fn role_exhaustiveness_canary() {
        use AccessibilityRole::*;
        for r in all_roles() {
            let known = match r {
                TitleBar | MenuBar | ScrollBar | Grip | Sound | Cursor | Caret | Alert | Window
                | Client | MenuPopup | MenuItem | Tooltip | Application | Document | Pane | Chart
                | Dialog | Border | Grouping | Separator | Toolbar | StatusBar | Table
                | ColumnHeader | RowHeader | Column | Row | Cell | Link | HelpBalloon | Character
                | List | ListItem | Outline | OutlineItem | PageTab | PropertyPage | Indicator
                | Graphic | StaticText | Text | PushButton | CheckButton | RadioButton | ComboBox
                | DropList | ProgressBar | Dial | HotkeyField | Slider | SpinButton | Diagram
                | Animation | Equation | ButtonDropdown | ButtonMenu | ButtonDropdownGrid
                | Whitespace | PageTabList | Clock | SplitButton | IpAddress | Nothing | Unknown => true,
            };
            assert!(known);
        }

        use AccessibilityState::*;
        for s in all_states() {
            let known = match s {
                Unavailable | Selected | Focused | CheckedTrue | CheckedFalse | Readonly
                | Default | Expanded | Collapsed | Busy | Offscreen | Focusable | Selectable
                | Linked | Traversed | Multiselectable | Protected => true,
            };
            assert!(known);
        }
    }

    // =====================================================================
    // Total-order / Eq / Hash contracts on the plain C-like enums
    // =====================================================================

    #[test]
    fn role_ord_is_strict_declaration_order() {
        let roles = all_roles();
        // Strictly increasing => derived Ord follows declaration order AND the
        // list has no duplicates.
        for pair in roles.windows(2) {
            assert!(
                pair[0] < pair[1],
                "roles must sort in declaration order: {:?} !< {:?}",
                pair[0],
                pair[1]
            );
        }
        // Trichotomy: for every ordered pair exactly one of <, ==, > holds.
        for a in &roles {
            for b in &roles {
                let lt = a < b;
                let eq = a == b;
                let gt = a > b;
                assert_eq!(
                    u8::from(lt) + u8::from(eq) + u8::from(gt),
                    1,
                    "trichotomy violated for {a:?} vs {b:?}"
                );
            }
        }
        // Reflexivity + the documented endpoints.
        assert_eq!(roles[0], AccessibilityRole::TitleBar);
        assert_eq!(*roles.last().unwrap(), AccessibilityRole::Unknown);
        assert!(AccessibilityRole::TitleBar < AccessibilityRole::Unknown);
    }

    #[test]
    fn state_ord_is_strict_declaration_order() {
        let states = all_states();
        for pair in states.windows(2) {
            assert!(pair[0] < pair[1], "{:?} !< {:?}", pair[0], pair[1]);
        }
        // CheckedTrue / CheckedFalse are adjacent but must never compare equal —
        // aliasing them would make a checked and unchecked box indistinguishable.
        assert_ne!(AccessibilityState::CheckedTrue, AccessibilityState::CheckedFalse);
        assert_ne!(
            hash_of(&AccessibilityState::CheckedTrue),
            hash_of(&AccessibilityState::CheckedFalse)
        );
    }

    #[test]
    fn role_and_state_hash_agrees_with_eq() {
        // Eq => equal hashes (the direction the Hash contract actually requires),
        // and Hash is deterministic across calls.
        for r in all_roles() {
            let copy = r;
            assert_eq!(hash_of(&r), hash_of(&copy));
        }
        for s in all_states() {
            let copy = s;
            assert_eq!(hash_of(&s), hash_of(&copy));
        }

        // Stronger: no two distinct variants may collide. A collision here would
        // let two different roles/states alias as the same HashMap key. The
        // derive hashes the (necessarily distinct) discriminant, and FNV-1a's
        // multiply step is invertible mod 2^64, so distinctness is guaranteed —
        // this pins that no variant is ever given a duplicate discriminant.
        let role_hashes: Vec<u64> = all_roles().iter().map(hash_of).collect();
        for (i, a) in role_hashes.iter().enumerate() {
            for (j, b) in role_hashes.iter().enumerate() {
                assert_eq!(i == j, a == b, "role hash collision at {i}/{j}");
            }
        }

        let state_hashes: Vec<u64> = all_states().iter().map(hash_of).collect();
        for (i, a) in state_hashes.iter().enumerate() {
            for (j, b) in state_hashes.iter().enumerate() {
                assert_eq!(i == j, a == b, "state hash collision at {i}/{j}");
            }
        }
    }

    // =====================================================================
    // AccessibilityStateVec — FFI vec round-trip
    // =====================================================================

    #[test]
    fn state_vec_round_trips_through_ffi_wrapper() {
        let cases: Vec<Vec<AccessibilityState>> = vec![
            Vec::new(),
            vec![AccessibilityState::Focused],
            all_states(),
            // duplicates must survive verbatim (this is a Vec, not a Set)
            vec![
                AccessibilityState::Busy,
                AccessibilityState::Busy,
                AccessibilityState::Busy,
            ],
            // large allocation: the FFI wrapper owns the buffer, so this is the
            // shape most likely to trip a bad len/cap or double-free.
            core::iter::repeat(AccessibilityState::Selected).take(10_000).collect(),
        ];

        for original in cases {
            let wrapped: AccessibilityStateVec = original.clone().into();

            // len / is_empty stay consistent with the source Vec.
            assert_eq!(wrapped.len(), original.len());
            assert_eq!(wrapped.is_empty(), original.is_empty());
            assert_eq!(wrapped.as_slice(), original.as_slice());
            assert_eq!(wrapped.iter().count(), original.len());

            // Clone must deep-copy: equal content, and dropping the clone must
            // not invalidate the original (both are dropped at end of scope).
            let cloned = wrapped.clone();
            assert_eq!(cloned.as_slice(), original.as_slice());
            assert_eq!(cloned, wrapped);
            assert_eq!(hash_of(&cloned), hash_of(&wrapped));
            drop(cloned);
            assert_eq!(wrapped.as_slice(), original.as_slice());

            // Round-trip back out: decode(encode(x)) == x.
            let back = wrapped.into_library_owned_vec();
            assert_eq!(back, original);
        }
    }

    #[test]
    fn state_vec_indexing_is_bounds_safe() {
        let v: AccessibilityStateVec = all_states().into();
        let len = v.len();

        for (i, expected) in all_states().into_iter().enumerate() {
            assert_eq!(v.get(i), Some(&expected));
        }
        // One-past-the-end and the pathological index must return None, not panic.
        assert_eq!(v.get(len), None);
        assert_eq!(v.get(len + 1), None);
        assert_eq!(v.get(usize::MAX), None);
        assert!(v.c_get(usize::MAX).is_none());
        assert!(v.c_get(len).is_none());
        assert!(v.c_get(0).is_some());

        // The empty vec has no valid index at all.
        let empty = AccessibilityStateVec::new();
        assert!(empty.is_empty());
        assert_eq!(empty.get(0), None);
        assert_eq!(empty.get(usize::MAX), None);
        assert!(empty.c_get(0).is_none());
    }

    #[test]
    fn state_vec_from_vec_preserves_order_len_and_lookup() {
        // The C-ABI vec is built from a Rust Vec and is then read-only — it has
        // no push/pop. Assert the round-trip is lossless and lookups agree.
        let empty = AccessibilityStateVec::new();
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
        assert_eq!(empty.get(0), None);

        let states = all_states();
        let v = AccessibilityStateVec::from_vec(states.clone());
        assert_eq!(v.len(), states.len());
        assert!(!v.is_empty());
        assert!(v.capacity() >= v.len(), "capacity must never trail len");

        // Order is preserved and every index is reachable.
        assert_eq!(v.as_slice(), states.as_slice());
        for (i, s) in states.iter().enumerate() {
            assert_eq!(v.get(i), Some(s));
        }
        assert_eq!(
            v.get(states.len()),
            None,
            "out-of-bounds must be None, not a panic"
        );
        assert!(v.iter().eq(states.iter()));
    }

    // =====================================================================
    // AccessibilityAction — payload-carrying variants
    // =====================================================================

    /// One instance of every `AccessibilityAction` variant, in declaration order.
    fn all_actions() -> Vec<AccessibilityAction> {
        use AccessibilityAction::*;
        vec![
            Default,
            Focus,
            Blur,
            Collapse,
            Expand,
            ScrollIntoView,
            Increment,
            Decrement,
            ShowContextMenu,
            HideTooltip,
            ShowTooltip,
            ScrollUp,
            ScrollDown,
            ScrollLeft,
            ScrollRight,
            ReplaceSelectedText("replacement".into()),
            ScrollToPoint(LogicalPosition::new(1.0, 2.0)),
            SetScrollOffset(LogicalPosition::new(-3.0, 4.0)),
            SetTextSelection(TextSelectionStartEnd {
                selection_start: 0,
                selection_end: 5,
            }),
            SetSequentialFocusNavigationStartingPoint,
            SetValue("value".into()),
            SetNumericValue(FloatValue::new(1.5)),
            CustomAction(42),
        ]
    }

    #[test]
    fn action_vec_round_trips_with_payloads() {
        let original = all_actions();
        let wrapped: AccessibilityActionVec = original.clone().into();

        assert_eq!(wrapped.len(), original.len());
        assert_eq!(wrapped.as_slice(), original.as_slice());

        // The payload variants own heap data (AzString). Cloning must deep-copy;
        // dropping the clone must leave the original intact (no double-free).
        let cloned = wrapped.clone();
        assert_eq!(cloned, wrapped);
        assert_eq!(hash_of(&cloned), hash_of(&wrapped));
        drop(cloned);
        assert_eq!(wrapped.as_slice(), original.as_slice());

        let back = wrapped.into_library_owned_vec();
        assert_eq!(back, original);

        // Variant order dominates payload in the derived Ord.
        for pair in original.windows(2) {
            assert!(pair[0] < pair[1], "{:?} !< {:?}", pair[0], pair[1]);
        }
    }

    #[test]
    fn action_string_payloads_survive_adversarial_strings() {
        for s in adversarial_strings() {
            let expected = s.clone();

            let replace = AccessibilityAction::ReplaceSelectedText(s.clone().into());
            let set = AccessibilityAction::SetValue(s.into());

            // Payload preserved verbatim — including interior NUL and lone
            // combining marks, which a C-string round-trip would truncate.
            match &replace {
                AccessibilityAction::ReplaceSelectedText(got) => {
                    assert_eq!(got.as_str(), expected.as_str());
                    assert_eq!(got.as_str().len(), expected.len());
                }
                other => panic!("wrong variant: {other:?}"),
            }
            match &set {
                AccessibilityAction::SetValue(got) => assert_eq!(got.as_str(), expected.as_str()),
                other => panic!("wrong variant: {other:?}"),
            }

            // Clone/Eq/Hash agree even for the pathological payloads.
            assert_eq!(replace.clone(), replace);
            assert_eq!(hash_of(&replace.clone()), hash_of(&replace));
            // Different variants with the *same* payload must never alias.
            assert_ne!(replace, set);
        }
    }

    #[test]
    fn action_custom_action_i32_limits() {
        let min = AccessibilityAction::CustomAction(i32::MIN);
        let zero = AccessibilityAction::CustomAction(0);
        let max = AccessibilityAction::CustomAction(i32::MAX);

        // Signed ordering, not a bit-pattern/unsigned ordering.
        assert!(min < zero, "i32::MIN must sort below 0");
        assert!(zero < max);
        assert!(min < max);

        assert_eq!(min, AccessibilityAction::CustomAction(i32::MIN));
        assert_ne!(min, max);
        assert_eq!(hash_of(&min), hash_of(&AccessibilityAction::CustomAction(i32::MIN)));

        // -1 must not alias u32::MAX-style onto anything.
        assert_ne!(
            AccessibilityAction::CustomAction(-1),
            AccessibilityAction::CustomAction(i32::MAX)
        );
    }

    #[test]
    fn text_selection_start_end_limits() {
        // usize::MAX bounds: constructing and comparing must not overflow.
        let huge = TextSelectionStartEnd {
            selection_start: usize::MAX,
            selection_end: usize::MAX,
        };
        assert_eq!(huge.selection_start, usize::MAX);
        assert_eq!(huge.selection_end, usize::MAX);
        assert_eq!(huge, huge);

        // Inverted range (start > end) is accepted verbatim — the type does not
        // normalise or clamp, so downstream consumers must not assume start<=end.
        let inverted = TextSelectionStartEnd {
            selection_start: 10,
            selection_end: 0,
        };
        assert_eq!(inverted.selection_start, 10);
        assert_eq!(inverted.selection_end, 0);
        assert_ne!(
            inverted,
            TextSelectionStartEnd {
                selection_start: 0,
                selection_end: 10,
            }
        );

        // Collapsed (zero-length) selection is distinct from an empty-at-zero one.
        let collapsed = TextSelectionStartEnd {
            selection_start: 7,
            selection_end: 7,
        };
        assert_ne!(
            collapsed,
            TextSelectionStartEnd {
                selection_start: 0,
                selection_end: 0,
            }
        );

        // Ord is lexicographic (start, then end).
        let a = TextSelectionStartEnd {
            selection_start: 1,
            selection_end: 99,
        };
        let b = TextSelectionStartEnd {
            selection_start: 2,
            selection_end: 0,
        };
        assert!(a < b, "selection_start must dominate the ordering");

        // Wrapped in the action, the same invariants hold.
        let action = AccessibilityAction::SetTextSelection(huge);
        assert_eq!(action.clone(), action);
        assert_eq!(hash_of(&action.clone()), hash_of(&action));
    }

    // =====================================================================
    // f32-carrying payloads: the Eq/Ord/Hash total-order contract
    //
    // `AccessibilityAction` *derives* Eq + Ord + Hash while carrying
    // `LogicalPosition` (two f32s) and `FloatValue`. f32 is not Eq/Ord, so
    // those inner types must supply total impls. These tests pin the actual
    // behaviour at NaN / inf / overflow, where a naive impl breaks the
    // reflexivity (a == a) that HashMap and BTreeMap rely on.
    // =====================================================================

    #[test]
    fn scroll_to_point_nan_is_reflexive_and_totally_ordered() {
        let nan = AccessibilityAction::ScrollToPoint(LogicalPosition::new(f32::NAN, f32::NAN));
        let origin = AccessibilityAction::ScrollToPoint(LogicalPosition::new(0.0, 0.0));

        // Reflexivity: `Eq` promises a == a. Raw f32 PartialEq would return
        // false here and quietly corrupt any HashMap keyed on this action.
        assert_eq!(nan, nan.clone());
        assert_eq!(hash_of(&nan), hash_of(&nan.clone()));
        assert_eq!(nan.cmp(&nan.clone()), core::cmp::Ordering::Equal);

        // NaN must NOT alias onto the origin (LogicalPosition::quantize maps NaN
        // to a dedicated i64::MIN sentinel precisely to avoid that collision).
        assert_ne!(nan, origin);
        assert_ne!(hash_of(&nan), hash_of(&origin));
        assert!(nan < origin, "NaN sorts below every real coordinate");

        // Ord is total: every pair of these is comparable and antisymmetric.
        let neg = AccessibilityAction::ScrollToPoint(LogicalPosition::new(-1.0, -1.0));
        let pos = AccessibilityAction::ScrollToPoint(LogicalPosition::new(1.0, 1.0));
        let mut sorted = vec![pos.clone(), origin.clone(), nan.clone(), neg.clone()];
        sorted.sort();
        assert_eq!(sorted, vec![nan, neg, origin, pos]);
    }

    #[test]
    fn scroll_to_point_infinite_coords_saturate_without_panic() {
        let inf = AccessibilityAction::SetScrollOffset(LogicalPosition::new(
            f32::INFINITY,
            f32::NEG_INFINITY,
        ));
        let finite = AccessibilityAction::SetScrollOffset(LogicalPosition::new(1.0, 1.0));

        // Defined, reflexive, no panic on the fixed-point conversion.
        assert_eq!(inf, inf.clone());
        assert_eq!(hash_of(&inf), hash_of(&inf.clone()));
        assert!(inf > finite, "+inf x-coordinate must sort above a finite one");

        // Documented saturation: the fixed-point quantisation clamps, so
        // f32::MAX and +inf land in the same bucket. Asserted so a future
        // change to the quantiser has to consciously break this.
        let max = AccessibilityAction::SetScrollOffset(LogicalPosition::new(f32::MAX, f32::MAX));
        let plus_inf =
            AccessibilityAction::SetScrollOffset(LogicalPosition::new(f32::INFINITY, f32::INFINITY));
        assert_eq!(
            max, plus_inf,
            "f32::MAX and +inf both saturate to the same quantised coordinate"
        );
    }

    #[test]
    fn set_numeric_value_float_edges_are_defined() {
        // Representable-under-quantisation values round-trip exactly
        // (FloatValue is fixed-point with a 1/1000 quantum).
        for v in [0.0_f32, 1.5, -1.5, 2.25, -3.75, 1000.0] {
            let f = FloatValue::new(v);
            assert_eq!(f.get(), v, "FloatValue must round-trip {v}");
            let action = AccessibilityAction::SetNumericValue(f);
            assert_eq!(action.clone(), action);
        }

        // Non-finite input must not panic. `as isize` saturates, so:
        assert_eq!(FloatValue::new(f32::INFINITY).number(), isize::MAX);
        assert_eq!(FloatValue::new(f32::NEG_INFINITY).number(), isize::MIN);

        // NOTE (reported, not a weakened assertion): FloatValue::new maps NaN to
        // 0 via a raw `as isize` cast, so a NaN numeric value is INDISTINGUISHABLE
        // from 0.0. LogicalPosition::quantize explicitly fixed this same aliasing
        // (NaN -> i64::MIN sentinel); FloatValue still has it. Pinning the current
        // behaviour so the aliasing is visible and a fix has to update this test.
        assert_eq!(FloatValue::new(f32::NAN).number(), 0);
        assert_eq!(
            AccessibilityAction::SetNumericValue(FloatValue::new(f32::NAN)),
            AccessibilityAction::SetNumericValue(FloatValue::new(0.0)),
            "KNOWN ALIASING: NaN numeric value collides with 0.0"
        );

        // Reflexivity still holds for the NaN case (it is Eq-safe, just aliased).
        let nan_action = AccessibilityAction::SetNumericValue(FloatValue::new(f32::NAN));
        assert_eq!(hash_of(&nan_action), hash_of(&nan_action.clone()));

        // f32::MAX overflows the fixed-point scale and saturates rather than wrapping.
        assert_eq!(FloatValue::new(f32::MAX).number(), isize::MAX);
        assert_eq!(FloatValue::new(f32::MIN).number(), isize::MIN);
    }

    // =====================================================================
    // Float -> value-string encoding: format/parse round-trip
    // =====================================================================

    #[test]
    fn progress_value_string_round_trips_through_parse() {
        for v in adversarial_f32() {
            let full = ProgressAriaInfo::create("p".into())
                .with_current_value(v)
                .to_full_info();
            let s = name_str(&full.accessibility_value).expect("determinate => Some");

            if v.is_nan() {
                assert_eq!(s, "NaN");
                assert!(s.parse::<f32>().unwrap().is_nan());
            } else if v.is_infinite() {
                assert_eq!(s, if v > 0.0 { "inf" } else { "-inf" });
            } else {
                // Display for f32 is shortest-round-trip: decode(encode(v)) == v.
                let parsed: f32 = s.parse().expect("emitted value string must re-parse");
                assert_eq!(parsed, v, "round-trip failed for {v} via {s:?}");
                if v != 0.0 {
                    // Bit-exact for everything except +0.0/-0.0, which compare
                    // equal under `==` by definition.
                    assert_eq!(parsed.to_bits(), v.to_bits(), "lossy round-trip for {v}");
                }
            }
        }
    }

    #[test]
    fn meter_value_string_round_trips_through_parse() {
        for v in adversarial_f32() {
            let full = MeterAriaInfo::create("m".into(), v, 0.0, 1.0).to_full_info();
            // Meter ALWAYS emits a value string (unlike progress).
            let s = name_str(&full.accessibility_value).expect("meter always emits a value");

            if v.is_nan() {
                assert_eq!(s, "NaN");
            } else if v.is_infinite() {
                assert_eq!(s, if v > 0.0 { "inf" } else { "-inf" });
            } else {
                let parsed: f32 = s.parse().expect("emitted value string must re-parse");
                assert_eq!(parsed, v);
                if v != 0.0 {
                    assert_eq!(parsed.to_bits(), v.to_bits());
                }
            }
        }
    }

    // =====================================================================
    // Builder algebra: purity, idempotence, last-write-wins, order-independence
    // =====================================================================

    #[test]
    fn to_full_info_is_pure_and_idempotent() {
        let small = SmallAriaInfo::label("s")
            .with_role(AccessibilityRole::Slider)
            .with_description("d");
        let progress = ProgressAriaInfo::create("p".into())
            .with_current_value(0.25)
            .with_max(10.0);
        let meter = MeterAriaInfo::create("m".into(), 1.0, 0.0, 2.0).with_low(0.5);
        let dialog = DialogAriaInfo::create("d".into()).with_modal(true);

        // &self getters must not mutate the receiver, and must be deterministic:
        // f(x) == f(x) for repeated calls.
        let (s0, p0, m0, d0) = (small.clone(), progress.clone(), meter.clone(), dialog.clone());

        assert_eq!(small.to_full_info(), small.to_full_info());
        assert_eq!(progress.to_full_info(), progress.to_full_info());
        assert_eq!(meter.to_full_info(), meter.to_full_info());
        assert_eq!(dialog.to_full_info(), dialog.to_full_info());

        assert_eq!(small, s0, "to_full_info must not mutate SmallAriaInfo");
        assert_eq!(progress, p0, "to_full_info must not mutate ProgressAriaInfo");
        assert_eq!(meter, m0, "to_full_info must not mutate MeterAriaInfo");
        assert_eq!(dialog, d0, "to_full_info must not mutate DialogAriaInfo");

        // Idempotent even when the value is NaN — the AccessibilityInfo carries a
        // *string* ("NaN"), which is Eq-comparable, so this holds where a raw f32
        // comparison would not.
        let nan_meter = MeterAriaInfo::create("m".into(), f32::NAN, 0.0, 1.0);
        assert_eq!(nan_meter.to_full_info(), nan_meter.to_full_info());
    }

    #[test]
    fn progress_max_is_never_surfaced_in_full_info() {
        // `max` has no representation in AccessibilityInfo, so setting it to
        // anything at all — including inf/NaN — must not perturb the conversion.
        let baseline = ProgressAriaInfo::create("p".into())
            .with_current_value(0.5)
            .to_full_info();

        for v in adversarial_f32() {
            let with_max = ProgressAriaInfo::create("p".into())
                .with_current_value(0.5)
                .with_max(v)
                .to_full_info();
            assert_eq!(with_max, baseline, "with_max({v}) leaked into to_full_info");
        }
    }

    #[test]
    fn meter_threshold_builders_are_order_independent_and_last_write_wins() {
        // Order-independence: the three threshold setters touch disjoint fields.
        let a = MeterAriaInfo::create("m".into(), 0.5, 0.0, 1.0)
            .with_low(0.1)
            .with_high(0.9)
            .with_optimum(0.7);
        let b = MeterAriaInfo::create("m".into(), 0.5, 0.0, 1.0)
            .with_optimum(0.7)
            .with_high(0.9)
            .with_low(0.1);
        assert_eq!(a, b);

        // Last-write-wins, including when the second write is a non-finite value.
        let m = MeterAriaInfo::create("m".into(), 0.5, 0.0, 1.0)
            .with_low(0.1)
            .with_low(f32::INFINITY);
        assert_eq!(f32_of(&m.low), Some(f32::INFINITY));

        // Nonsensical-but-accepted config: low > high, optimum outside [min,max].
        // The type performs no validation; assert it stores them verbatim rather
        // than silently clamping (downstream code must do its own validation).
        let weird = MeterAriaInfo::create("m".into(), 5.0, 0.0, 1.0)
            .with_low(100.0)
            .with_high(-100.0)
            .with_optimum(-1.0);
        assert_eq!(f32_of(&weird.low), Some(100.0));
        assert_eq!(f32_of(&weird.high), Some(-100.0));
        assert_eq!(f32_of(&weird.optimum), Some(-1.0));
        assert_eq!(weird.current_value, 5.0); // out of [min,max], not clamped
        assert!(weird.to_full_info().accessibility_value.is_some());
    }

    #[test]
    fn progress_and_dialog_builders_last_write_wins() {
        let p = ProgressAriaInfo::create("p".into())
            .with_current_value(1.0)
            .with_current_value(2.0)
            .with_indeterminate(true)
            .with_indeterminate(false)
            .with_description("a".into())
            .with_description("b".into());
        assert_eq!(f32_of(&p.current_value), Some(2.0));
        assert!(!p.indeterminate);
        assert_eq!(name_str(&p.description), Some("b"));
        // Not indeterminate => the (last) current value is surfaced.
        assert_eq!(name_str(&p.to_full_info().accessibility_value), Some("2"));

        let d = DialogAriaInfo::create("d".into())
            .with_modal(true)
            .with_modal(false)
            .with_role(AccessibilityRole::Alert)
            .with_role(AccessibilityRole::Dialog)
            .with_described_by("x".into())
            .with_described_by("y".into());
        assert!(!d.modal);
        assert_eq!(d.role, AccessibilityRole::Dialog);
        assert_eq!(name_str(&d.described_by), Some("y"));
    }

    // =====================================================================
    // AccessibilityInfo — the fully-populated aggregate
    // =====================================================================

    fn full_info_fixture() -> AccessibilityInfo {
        AccessibilityInfo {
            accessibility_name: OptionString::Some("name".into()),
            accessibility_value: OptionString::Some("value".into()),
            description: OptionString::Some("desc".into()),
            accelerator: OptionVirtualKeyCodeCombo::None,
            default_action: OptionString::Some("activate".into()),
            states: all_states().into(),
            supported_actions: all_actions().into(),
            labelled_by: OptionDomNodeId::Some(DomNodeId::ROOT),
            described_by: OptionDomNodeId::Some(DomNodeId {
                dom: DomId { inner: 3 },
                node: NodeHierarchyItemId::from_raw(7),
            }),
            role: AccessibilityRole::PushButton,
            is_live_region: true,
        }
    }

    #[test]
    fn full_info_clone_eq_hash_ord_are_consistent() {
        let a = full_info_fixture();
        let b = a.clone();

        // Deep clone: equal, equally hashed, mutually Equal under Ord.
        assert_eq!(a, b);
        assert_eq!(hash_of(&a), hash_of(&b));
        assert_eq!(a.cmp(&b), core::cmp::Ordering::Equal);

        // The clone owns its own heap buffers — dropping it must leave `a` intact.
        drop(b);
        assert_eq!(a.states.len(), all_states().len());
        assert_eq!(a.supported_actions.len(), all_actions().len());
        assert_eq!(name_str(&a.accessibility_name), Some("name"));

        // Perturbing any single field must break equality (no field is ignored
        // by the derived PartialEq — a field silently dropped from the derive
        // would let two different a11y nodes compare equal).
        let mut differs = a.clone();
        differs.is_live_region = false;
        assert_ne!(a, differs);

        let mut differs = a.clone();
        differs.role = AccessibilityRole::Unknown;
        assert_ne!(a, differs);

        let mut differs = a.clone();
        differs.labelled_by = OptionDomNodeId::None;
        assert_ne!(a, differs);

        let mut differs = a.clone();
        differs.states = Vec::new().into();
        assert_ne!(a, differs);

        let mut differs = a.clone();
        differs.supported_actions = Vec::new().into();
        assert_ne!(a, differs);

        let mut differs = a.clone();
        differs.default_action = OptionString::None;
        assert_ne!(a, differs);
    }

    // =====================================================================
    // Option<T> FFI wrappers — Some/None round-trip
    // =====================================================================

    #[test]
    fn option_wrappers_round_trip() {
        // Copy payloads.
        for r in all_roles() {
            let opt = OptionAccessibilityRole::Some(r);
            assert!(opt.is_some());
            assert!(!opt.is_none());
            assert_eq!(opt.as_ref(), Some(&r));
            assert_eq!(opt.into_option(), Some(r));
        }
        assert!(OptionAccessibilityRole::None.is_none());
        assert_eq!(OptionAccessibilityRole::None.into_option(), None);

        for s in all_states() {
            assert_eq!(OptionAccessibilityState::Some(s).into_option(), Some(s));
        }
        assert_eq!(OptionAccessibilityState::None.into_option(), None);

        // Non-Copy payloads (heap-owning) must round-trip without a double-free.
        for a in all_actions() {
            let opt = OptionAccessibilityAction::Some(a.clone());
            assert!(opt.is_some());
            assert_eq!(opt.into_option(), Some(a));
        }
        assert!(OptionAccessibilityAction::None.is_none());

        let small = SmallAriaInfo::label("s").with_role(AccessibilityRole::Link);
        assert_eq!(
            OptionSmallAriaInfo::Some(small.clone()).into_option(),
            Some(small)
        );
        assert!(OptionSmallAriaInfo::None.is_none());

        let progress = ProgressAriaInfo::create("p".into()).with_current_value(0.5);
        assert_eq!(
            OptionProgressAriaInfo::Some(progress.clone()).into_option(),
            Some(progress)
        );

        let meter = MeterAriaInfo::create("m".into(), 1.0, 0.0, 2.0);
        assert_eq!(
            OptionMeterAriaInfo::Some(meter.clone()).into_option(),
            Some(meter)
        );

        let dialog = DialogAriaInfo::create("d".into()).with_modal(true);
        assert_eq!(
            OptionDialogAriaInfo::Some(dialog.clone()).into_option(),
            Some(dialog)
        );

        // The big aggregate, which owns two FFI vecs.
        let info = full_info_fixture();
        assert_eq!(
            OptionAccessibilityInfo::Some(info.clone()).into_option(),
            Some(info)
        );
        assert!(OptionAccessibilityInfo::None.is_none());
    }
}
