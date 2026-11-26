# AzReview: the flow

The product picture of AzReview as the user described it (2026-11-25). The code follows it; when the code changes, this
file changes with it. The user's words are kept verbatim in section 1; sections 2 to 5 are the picture made specific.

## 1. The user's words (verbatim)
"good, and then the settings make it configurable which model to use for exploration and editing? i.e. "claude -p" vs API or Gemini for rewriting comments? Then just on the layout: the 1 / 2 / 3 ... numbers for the pages should be
vertically on the side (so when the sidebar with the file tree appears, it slides in from the left). No scrollbar there. Then the upper buttons for the different pen types should move down a bit so they're in line with the macos
traffic lights, and the comment button itself should disappear. I'm not sure what the "marker" button does, or if we need it. At the bottom will then be the bar with the agents and on the right side of the screen there should be a
bar where I can quickly open / close a whiteboard section (and resize it). So the idea is that I can circle some code, then drag it to the sidebar and it'll keep a CPU-rendered cutout of my shape there, and internally keep this
source code location there - this way I can very "naturally" draw arrows in the "whiteboard sidebar" and then double click any image to jump back to that file / line where my cutout was - this is to explore the architecture of the
codebase. That also needs to be saved. Finally, I want a "presenter" options, so that I can record my camera / voice (see AzMeet code) and save to disk and preview myself, so I can record a video of me reviewing the codebase or
explaining it like in a video call. So we have four borders of the screen with a small edge each: top showing pens 1-5 (cycle-able via double click on the pen or 1-5 on the keyboard) and the save / options gear icon, left the sidebar
(if open, showing files in green / yellow in case there are still unresolved agents in one file) + the vertical page jump navigation, bottom the working agents, so I can review + dictate, and go on while the agent asynchronously
starts working, keeping the page breaks stable as I told you, and then in the bottom bar I see when it turns to yellow, and then on the right the architecture of the entire workspace where I can drag code locations in and draw
arrows between them and so on. For the file tree, remove the heading / title, its useless and takes up space. The file tree should take up the entire height and shift the entire layout (pages) to the left on slide in / out. Note this
down somewhere so you get the "flow", then start refactoring / building."

## 2. The screen: four edges around the pages
```
 +----------------------------------------------------------------------------------+
 | (o)(o)(o)  [1][2][3][4][5]   pens, vertically in line with the traffic lights   [rec] [save] [gear] |  TOP edge
 +----+---------------------------------------------------------------+--+----------+
 | 1  |                                                               |  | WHITE-   |
 | 2  |   the PAGES of the current file (ink, the page's own           |R | BOARD    |  RIGHT edge:
 | 3  |   sidebar column with its conversations)                       |  | (open /  |  a thin bar, click or
 | .. |                                                               |  |  close,  |  drag to open, resizable)
 |LEFT|                                                               |  |  resize) |
 +----+---------------------------------------------------------------+--+----------+
 | [blue spinner: agent working] [yellow: suggestion waiting] [green: done]  ...     |  BOTTOM edge
 +----------------------------------------------------------------------------------+
 LEFT edge: the vertical page-jump numbers (always there, no scrollbar); the FILE TREE slides in from the left over the full
 height (no heading), and PUSHES the pages (the whole layout shifts by the tree's width; it does not cover them).
```

## 3. Edge by edge
TOP (the title-bar row, vertically centred on the macOS traffic lights)
- Pens 1-5 (the five meanings: scope, issue, question, duplicate, praise; colours as today). Selected by the keys 1-5 or a click; a DOUBLE CLICK on a pen (or the same key again) cycles that pen's nib style
  (the nibs today: marker = broad highlighter that always means "scope", pen = the selected meaning, audio pen = a pen that also records/dictates).
- The "Marker" button (the nib selector: it shows and cycles marker -> pen -> audio pen on a click) goes away: the nib is a property of the pen (see double click). The comment button goes away.
- At the right end: the presenter toggle (a record/camera icon), save, and the gear (options/settings: language, theme, dwell, agents and models, recorder devices ...).
LEFT
- The vertical page-jump strip: 1 / 2 / 3 ... one chip per page, vertical, NO scrollbar (many pages: it scrolls by the wheel/drag without a visible bar, or compacts); the current page highlighted.
- The file tree: slides in from the left edge (animated), FULL height, no heading or title, shifts the pages layout by its width on slide in/out (INTERPRETATION: the pages move right, to make room; the user wrote "to the
  left", which can only mean the layout makes room at the left: ask in the plan if unsure). Each file row shows its state: green = nothing unresolved, yellow = unresolved agent suggestions in that file, blue spinner = an agent is
  working on it (same colours as the bottom bar). Opening a folder, the last folder, an empty state with a big "Open folder" button; Cmd+O; the current folder's name is visible.
BOTTOM
- The agents bar of the CURRENT FILE: one chip per conversation: blue macOS-style spinner = the agent is working, yellow = it suggested something (additional lines, words greyed out for removal, yellow for in-line
  replacement), green = finished. Click = go there. The user reviews and dictates and goes on while the agent works asynchronously; the bar tells when a chip turns yellow.
RIGHT
- A thin bar: click or drag opens/closes the WHITEBOARD section; it is resizable by dragging its inner edge; its open/closed state and width are remembered.

## 4. The flows
Review and dictate: draw on the page with a pen; a pen at rest (or the audio nib) starts a conversation anchored at that spot; the transcript shows its LAST THREE LINES live in the page's own sidebar column; the user goes on
drawing; the agent works asynchronously (blue spinner in the bottom bar); when it has a suggestion the chip turns YELLOW; accepting records a diff (green).
Deletion: a quick, mostly horizontal slash across a word deletes it at once (no wait): the diff is recorded in memory (+ session archive), the word is drawn in the BACKGROUND colour, its space stays: nothing reflows.
Additions by the agent: separate YELLOW lines (GREEN once accepted) with a checkmark in the sidebar; removals: words greyed out until accepted; in-line replacements: old greyed, new yellow.
PAGE BREAKS ARE STABLE: the initial page breaks never move; a page that receives inserted lines GROWS vertically and overflows; nothing else moves (page count and the page of every other line unchanged).
No extra git worktrees: edits are in-memory diffs + the session archive; the source files are touched only by an explicit export. What the agent gets when a transcription starts: the stroke as an SVG shape with pressure and time
(recorded per sample: x, y, pressure, time, tilt), the transcript so far, and the context (file, the lines around, the words under the stroke).
Whiteboard: circle some code (a lasso with the pen), DRAG the circled part to the right bar: the whiteboard keeps a CPU-RENDERED CUTOUT (an image of the shape's area, rendered by azul's CPU renderer at the window's device
scale) AND internally the source location (file, line range, the stroke's shape); the user draws ARROWS between cutouts naturally (arrows attach to cutouts and follow them), moves and resizes cutouts, adds a short label;
a DOUBLE CLICK on a cutout jumps to its file and line (and flashes the range). It is the architecture of the whole WORKSPACE (not one file), it is SAVED with the workspace (data root, atomic write, survives a restart; a
cutout whose file changed keeps its image and says so).
Presenter: records camera + voice (see examples/azul-meet and examples/azul-videocut for the capture and encode code) to a file on disk, with a live preview of the user (a small picture-in-picture that can be dragged and
hidden), start/stop/pause, a clear "recording" indicator, a list of the recordings with Reveal in Finder; the point: a video of the user reviewing the codebase or explaining it, as in a video call.
Agents and models (the gear): which model does EXPLORATION (read-only questions and analysis) and which does EDITING (proposals: rewriting comments, replacing or inserting lines): each chooses among `claude -p` (the Claude Code CLI,
print mode; no key needed), the Anthropic API, Gemini (the `gemini` CLI or the API), and a MOCK (scripted, for the e2e). Per task: backend, model name, timeout, a reference to the key (the NAME of an environment variable or a
keychain item, never the key in the settings file). Everything mockable: agent, speech, camera, microphone, clock.

## 5. Invariants for every slice
- Real-window verification at 2x with screenshots (never only headless); the user's real cursor is never moved by a test without "main" having the user's OK; the CPU renderer is the default, `HwAcceleration::Enabled` = GPU with
  automatic fallback to the CPU (azul already does this): AzReview asks for it like mapedit3.
- HiDPI: every image AzReview draws (ink, cutouts, spinners, markers) is rasterised at the window's device pixel ratio.
- Page-break stability, no worktrees, mockability, input caps with diagnostics (never a crash), no global state, tests in adjacent `*_test.rs` (RED first), en + de strings, accessibility names on every control, no attribution lines.
- Colours: blue = working, yellow = needs the user, green = done: the same in the bottom bar, the file tree and the page.
- Frame times of the real window stay good with 50 conversations and a full whiteboard (numbers in each report).

## 6. What is built (kept true with each landing)

| Part | State |
|------|-------|
| Pages of the open file, ink with the mouse, trackpad or pen, five meanings (keys 1 to 5) | built |
| Ink rasterised at the window's device pixel ratio, drawn by the GPU with an automatic fallback to the CPU renderer | built |
| A stroke ends with any release, a move that finds the pointer not pressed, the pointer leaving, the focus lost; the last 200 pointer events can be saved (Session menu, `AZREVIEW_EVENTS`, the debug op `azreview_pointer_log`) | built |
| A tap by a mouse or a trackpad changes no nib and selects no audio pen; a pen's tap still cycles the nib | built |
| Open folder (button in the top bar, Mod+O), the folder named in the top bar, the last folder remembered, an empty page with a large button | built |
| Dictation by the pen at rest (a note in the side column, a marker on the page) | built |
| The file tree: the first column, the whole height of the window, no heading, pushing the pages to the right (covering them below 700 px, Escape folds it then), sliding in 180 ms, its right edge dragged to resize, shown with Mod+B or the switch next to the traffic lights, open or closed and its width remembered; a pick of a file leaves it as it is | built |
| The vertical page strip at the left of the pages, without a scrollbar | built |
| The top bar in the title-bar row: the folder at the left, the five pens centred (a click takes a pen, a double click or its key again takes its next nib; each pen keeps its own nib and the pen in hand shows it; the scope pen has the marker, the others the pen and the audio pen), record, save and the gear at the right; no marker button, no comment button | built |
| The right bar and its panel (Mod+Shift+B, its left edge dragged, open or closed and the width remembered), a placeholder until the whiteboard is mounted in it | built |
| The bottom bar: a chip per conversation of the open file (blue spinner working, yellow a suggestion waits, green done; a click goes to its page; many chips scroll without a bar); the tree marks each file the same way; mock conversations until the agents answer | built |
| The page's code column and measures shared with the whiteboard's cutouts (`ui::page_code_column`, `PAGE_W`, `GUTTER_W`, `LINE_H`, `PAGE_PAD`) | built |
| Slash deletion, conversations in the page's sidebar, suggestions with accept, stable page breaks, stroke samples to the agent | planned |
| Whiteboard, presenter, agents and models panels | other modules, plugged in by the layout |

## 6b. The places other modules plug into (`ui.rs`, "The places other modules plug into")
`right_panel_content` (the whiteboard's panel, `whiteboard_ui::panel_dom`), `presenter_slot` (the recorder's key in the top bar) and `overlay_slot`
(the camera's picture in picture over the pages), `settings_sections` (the settings page after the dictation's: the agents and their models, the recorder's
devices, the speech recogniser's chain), the bottom bar's conversations (`AppState::chips`: the agent pool's messages set their states), and the window's
debug ops (`azreview_pointer_log`, `azreview_mock_chips`, `azreview_state`).

## 7. Interpretations (open until the user corrects them)
1. The tree pushes the pages to the right (not left). 2. The nib folds into the pens (double click cycles). 3. Files: yellow = unresolved suggestions, blue spinner = working, green = clean. 4. The whiteboard is per workspace
(the opened folder), one board. 5. The presenter records camera + voice only (the screen is not recorded by AzReview).
