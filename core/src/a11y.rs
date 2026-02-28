//! Accessibility types for screen reader support.
//!
//! Extracted from `dom.rs` to keep that module focused on DOM structure.

use azul_css::{
    AzString, OptionString,
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
    /// Get an enumerated value representing what this IAccessible is used for,
    /// for example is it a link, static text, editable text, a checkbox, or a table cell, etc.
    pub role: AccessibilityRole,
    /// For live regions that update automatically (e.g., chat messages, timers).
    /// Maps to accesskit's `Live` property.
    pub is_live_region: bool,
}

/// Actions that can be performed on an accessible element.
/// This is a simplified version of accesskit::Action to avoid direct dependency in core.
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

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
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
/// like screen readers about the function of a UI element. Each variant corresponds to a
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

    /// The element is checked, toggled, or in a mixed state.
    /// - **Purpose**: To represent the state of controls like checkboxes, radio buttons, and
    ///   toggle buttons.
    /// - **When to use**: For checkboxes that are ticked, selected radio buttons, or toggle
    ///   buttons that are "on."
    /// - **Example**: A checked "I agree" checkbox, a selected "Yes" radio button, or an active
    ///   "Bold" button in a toolbar.
    Checked,

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
    /// - **When to use**: When an application is loading, processing refany, or otherwise occupied.
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

    pub fn with_role(mut self, role: AccessibilityRole) -> Self {
        self.role = OptionAccessibilityRole::Some(role);
        self
    }

    pub fn with_description<S: Into<AzString>>(mut self, desc: S) -> Self {
        self.description = OptionString::Some(desc.into());
        self
    }

    /// Convert to full `AccessibilityInfo`
    pub fn to_full_info(&self) -> AccessibilityInfo {
        AccessibilityInfo {
            accessibility_name: self.label.clone(),
            accessibility_value: OptionString::None,
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
