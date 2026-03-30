# Session 8B: Widget API Cleanup + Finder Clone + Data Visualization

**Date**: 2026-03-30
**Branch**: `layout-debug-clean`
**Status**: Planning complete, ready for implementation

---

## 1. Browser HTTP Bug Fix (CRITICAL — blocks browser.c)

### Root Cause

`browser.c:420` has a **union access bug**: checks `http_result.Ok.tag` instead of
`http_result.Err.tag`. In C, both variants of the union share the same tag offset,
but the code reads the Ok variant when the actual data is Err. This causes the
request to always appear to fail.

### Fix

```c
// WRONG (current):
if (http_result.Ok.tag == AzResultHttpResponseHttpError_Tag_Err) {

// CORRECT:
if (http_result.Err.tag == AzResultHttpResponseHttpError_Tag_Err) {
```

Also add error detail extraction using `AzHttpError_matchRef*()` functions:
- `AzHttpError_matchRefTlsError()` — TLS certificate errors
- `AzHttpError_matchRefConnectionFailed()` — network unreachable
- `AzHttpError_matchRefOther()` — generic ureq errors
- `AzHttpError_matchRefHttpStatus()` — 4xx/5xx responses

**Files**: `examples/c/browser.c` (lines 413-425)

---

## 2. Widget API Completeness

### 2.1 Widgets Exposed to C (via api.json)

| Widget | Status | Notes |
|--------|--------|-------|
| Button | ✅ Done | |
| CheckBox | ✅ Done | |
| TextInput | ✅ Done | |
| ProgressBar | ✅ Done | |
| FileInput | ✅ Done | |
| ColorInput | ✅ Done | |
| NumberInput | ✅ Done | |
| TabHeader | ✅ Done | |
| Frame | ✅ Done | |
| Label | ✅ Done | |
| DropDown | ✅ Done (this session) | |
| ListView | ✅ Done | Needs row push API |
| **TreeView** | ❌ Not exposed | Needs dynamic children API |
| **Ribbon** | ❌ Not exposed | Hardcoded tabs, needs generalization |
| **NodeGraph** | ❌ Not exposed | Complex, defer |

### 2.2 TreeView Enhancement (needed for Finder sidebar)

Current TreeView is too simple: `struct TreeView { root: AzString }` with hardcoded structure.

**New API needed:**

```rust
pub struct TreeView {
    pub root: TreeViewNode,
    pub on_node_click: OptionTreeViewOnNodeClick,
    pub on_node_expand: OptionTreeViewOnNodeExpand,
}

pub struct TreeViewNode {
    pub label: AzString,
    pub icon: OptionImageRef,       // File/folder icon
    pub children: TreeViewNodeVec,  // Nested children
    pub is_expanded: bool,          // Expand/collapse state
    pub is_selected: bool,
    pub node_data: OptionRefAny,    // User payload (file path, etc.)
}
```

**C usage:**
```c
AzTreeViewNode root = AzTreeViewNode_create(az("Favorites"));
AzTreeViewNode_addChild(&root, AzTreeViewNode_create(az("Desktop")));
AzTreeViewNode_addChild(&root, AzTreeViewNode_create(az("Documents")));
AzTreeView tree = AzTreeView_create(root);
AzTreeView_setOnNodeClick(&tree, data, on_node_click);
AzDom_addChild(&sidebar, AzTreeView_dom(tree));
```

### 2.3 Ribbon Generalization

Current Ribbon has hardcoded spreadsheet tabs (FILE, HOME, INSERT...).

**New API needed:**

```rust
pub struct Ribbon {
    pub tabs: RibbonTabVec,
    pub active_tab: usize,
    pub on_tab_click: OptionRibbonOnTabClick,
}

pub struct RibbonTab {
    pub label: AzString,
    pub sections: RibbonSectionVec,
}

pub struct RibbonSection {
    pub title: AzString,
    pub items: Dom,  // User provides the section content
}
```

### 2.4 ListView Enhancement

ListView already works for tabular data. Enhancements needed:

- `AzListViewRowVec_push()` or `AzListView_addRow()` — add rows incrementally
- Column width hints
- Sort direction indicators
- Selection state tracking

---

## 3. Finder Clone (`finder.c`)

### Architecture

```
┌─────────────────────────────────────────────────────────┐
│  Native Menu Bar (File, Edit, View, Go, Window, Help)   │
├─────────────────────────────────────────────────────────┤
│  Toolbar: [← →] [View Mode ▼] [  Search...  ]          │
├──────────┬──────────────────────────────────────────────┤
│ Sidebar  │  Main Content (ListView)                     │
│          │                                              │
│ Favorites│  Name          Size     Date Modified        │
│  Desktop │  ─────────────────────────────────────       │
│  Docs    │  Documents/    —        Mar 28, 2026         │
│  Downloads│  report.pdf   2.4 MB   Mar 27, 2026         │
│          │  photo.jpg    1.1 MB   Mar 26, 2026         │
│ Locations│  notes.txt    4 KB     Mar 25, 2026         │
│  Macintosh│                                             │
│  Network │                                              │
├──────────┴──────────────────────────────────────────────┤
│  Status: 42 items, 1.2 GB available                     │
└─────────────────────────────────────────────────────────┘
```

### Implementation Steps

1. **Native menu bar** — Use `AzDom_setMenuBar()` with full File/Edit/View/Go hierarchy
2. **Toolbar** — Row of Buttons + DropDown (view mode) + TextInput (search)
3. **Sidebar** — Enhanced TreeView with section headers (Favorites, Locations, Tags)
4. **Main content** — ListView with columns: Name, Date Modified, Size, Kind
5. **Status bar** — Simple div with item count
6. **Layout** — Flexbox: `flex-direction: row` for sidebar + content split

### Data Model

```c
typedef struct {
    char current_path[1024];      // Current directory
    TreeViewNode sidebar_tree;     // Sidebar hierarchy
    FileEntry* files;             // Current directory listing
    size_t file_count;
    size_t selected_index;
    ViewMode view_mode;           // List, Icon, Column, Gallery
} FinderData;
```

### File System Integration (C-side)

```c
// Platform-specific directory listing
FileEntry* list_directory(const char* path, size_t* count);
const char* get_file_icon_name(const char* filename);
const char* format_file_size(uint64_t bytes);
const char* format_date(time_t timestamp);
```

---

## 4. Data Visualization API

### Current State

`chart.c` creates charts using:
- R8 image masks (`AzImageMask`) for clip-path shapes (bars, pie wedges)
- Manual pixel buffer manipulation for mask generation
- Gradient backgrounds for coloring
- DOM layering for chart elements

### Proposed DataFrame + Chart API

#### 4.1 DataFrame (Rust widget, exposed to C)

```rust
pub struct DataFrame {
    pub columns: DataColumnVec,
}

pub struct DataColumn {
    pub name: AzString,
    pub values: DataColumnValues,
}

pub enum DataColumnValues {
    Float(FloatVec),
    Integer(IntVec),
    String(StringVec),
}

impl DataFrame {
    pub fn new() -> Self;
    pub fn add_float_column(&mut self, name: AzString, values: FloatVec);
    pub fn add_string_column(&mut self, name: AzString, values: StringVec);
    pub fn row_count(&self) -> usize;
    pub fn column_count(&self) -> usize;
}
```

#### 4.2 Chart Widgets

Each chart type is a widget that takes a DataFrame reference and configuration:

```rust
pub struct BarChart {
    pub data: DataFrame,
    pub x_column: AzString,       // Category column name
    pub y_column: AzString,       // Value column name
    pub bar_color: ColorU,
    pub show_labels: bool,
    pub show_grid: bool,
    pub on_bar_click: OptionBarChartOnClick,
}

pub struct LineChart {
    pub data: DataFrame,
    pub x_column: AzString,
    pub y_columns: StringVec,     // Multiple series support
    pub colors: ColorUVec,
    pub show_points: bool,
    pub show_grid: bool,
    pub line_width: f32,
}

pub struct PieChart {
    pub data: DataFrame,
    pub label_column: AzString,
    pub value_column: AzString,
    pub colors: ColorUVec,
    pub show_labels: bool,
    pub show_percentages: bool,
}
```

**C usage:**
```c
AzDataFrame df = AzDataFrame_create();
float revenues[] = { 100, 150, 200, 180, 250 };
AzDataFrame_addFloatColumn(&df, az("Revenue"),
    AzFloatVec_copyFromPtr(revenues, 5));

AzBarChart chart = AzBarChart_create(df, az("Quarter"), az("Revenue"));
chart.bar_color = AzColorU_rgb(66, 133, 244);
chart.show_grid = true;
AzDom_addChild(&body, AzBarChart_dom(chart));
```

#### 4.3 Rendering Strategy

Charts render using the existing DOM + clip mask approach:
- **Bars**: Div with background color, height set via CSS percentage
- **Lines**: SVG path rendered as background-image or clip-mask
- **Pie**: Conic gradient backgrounds (already supported) or clip-mask wedges
- **Axes**: Text nodes positioned with CSS flex/grid
- **Grid**: Border-based grid lines on container divs
- **Labels**: Text nodes positioned at data points

This approach is accessible because each bar/slice/point is a real DOM node
with appropriate a11y attributes (role, label, value).

#### 4.4 Accessibility

Each chart element gets:
- `role="img"` on chart container with `aria-label`
- Individual bars/points as focusable nodes with data values
- Screen readers can navigate through data points
- Tab navigation through chart elements

---

## 5. Implementation Priority

| # | Task | Effort | Blocks |
|---|------|--------|--------|
| 1 | Fix browser.c HTTP bug | 30 min | browser demo |
| 2 | Enhance TreeView API + expose to C | 2-3 hrs | Finder sidebar |
| 3 | Finder.c skeleton (menu + layout) | 2-3 hrs | — |
| 4 | Generalize Ribbon API | 1-2 hrs | — |
| 5 | DataFrame struct + C API | 2-3 hrs | Charts |
| 6 | BarChart widget | 2-3 hrs | — |
| 7 | LineChart widget | 2-3 hrs | — |
| 8 | PieChart widget | 1-2 hrs | — |
| 9 | Finder.c complete (file system) | 3-4 hrs | — |

---

## 6. Key Files

| Component | File |
|-----------|------|
| TreeView widget | `layout/src/widgets/tree_view.rs` |
| ListView widget | `layout/src/widgets/list_view.rs` |
| Ribbon widget | `layout/src/widgets/ribbon.rs` |
| API definition | `api.json` |
| Chart example | `examples/c/chart.c` |
| Browser example | `examples/c/browser.c` |
| Widget showcase | `examples/c/widgets.c` |
| Menu types | `core/src/menu.rs` |
| Menu rendering | `dll/src/desktop/menu_renderer.rs` |
| Clip mask API | via `AzDom_withClipMask` in header |
| SVG paths | `layout/src/xml/svg.rs` |
