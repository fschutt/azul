/**
 * Azul Debugger — DOM inspector, CSS editor, and E2E test runner.
 *
 * Architecture:
 *   app.config   — connection, mock mode
 *   app.state    — all runtime state (selection, tests, overrides, etc.)
 *   app.schema   — command definitions with params, desc, example
 *   app.api      — HTTP communication (+ mock fallbacks)
 *   app.ui       — pure rendering functions (DOM tree, node detail, tests, etc.)
 *   app.handlers — user-interaction callbacks + persistence
 *   app.runner   — E2E test execution (client & server side)
 *   app.resizer  — panel resize logic
 *   app.json     — collapsible JSON tree widget (for App State)
 */
const app = {
    /* ================================================================
     * CONFIG
     * ================================================================ */
    config: {
        apiUrl: window.location.origin || 'http://localhost:8765',
        isMock: false,
    },

    /* ================================================================
     * STATE
     * ================================================================ */
    state: {
        currentView: 'inspector',   // 'inspector' | 'testing' | 'components'
        activePanel: 'terminal',    // 'terminal' | 'debug'
        activeTestId: null,
        tests: [],
        executionStatus: 'idle',
        currentStepIndex: -1,
        // DOM tree
        hierarchy: null,
        hierarchyRoot: -1,
        selectedNodeId: null,
        collapsedNodes: new Set(),
        contextMenuNodeId: null,
        // CSS overrides (node_id -> { prop: value })
        cssOverrides: {},
        // Menubar open state
        openMenu: null,
        // App state JSON (last loaded)
        appStateJson: null,
        // Saved app state snapshots { alias: jsonValue }
        snapshots: {},
        // Resolved symbol cache { address: resolvedInfo }
        resolvedSymbols: {},
        // Last loaded component registry data
        componentData: null,
        // Currently selected library name in components view
        selectedLibrary: null,
        // Library list cache
        libraryList: null,
        // JSON tree collapsed paths
        jsonCollapsed: new Set(),
        // JSON tree grouped ranges that are collapsed
        jsonGroupCollapsed: new Set(),
        // JSON tree collapsed paths for read-only views
        jsonReadOnlyCollapsed: new Set(),
        jsonReadOnlyGroupCollapsed: new Set(),
        // Node dataset state
        datasetJson: null,
        datasetNodeId: null,
        // Preview environment overrides (H4)
        previewOs: null,       // null = native, "windows", "mac", "linux"
        previewTheme: null,    // null = native, "light", "dark"
        previewLang: null,     // null = native, e.g. "en", "de", "fr"
    },

    /* ================================================================
     * SCHEMA — every debug API command with params, description, example
     * ================================================================ */
    schema: {
        commands: {
            // ── Queries ──
            'get_state':            { desc: 'Get debug server state',              examples: ['/get_state'],                              params: [] },
            'get_dom':              { desc: 'Get raw DOM structure',                examples: ['/get_dom'],                                params: [] },
            'get_html_string':      { desc: 'Get DOM as HTML string',              examples: ['/get_html_string'],                        params: [] },
            'get_dom_tree':         { desc: 'Get detailed DOM tree',               examples: ['/get_dom_tree'],                            params: [] },
            'get_node_hierarchy':   { desc: 'Get raw node hierarchy',              examples: ['/get_node_hierarchy'],                      params: [] },
            'get_layout_tree':      { desc: 'Get layout tree (debug)',             examples: ['/get_layout_tree'],                         params: [] },
            'get_display_list':     { desc: 'Get display list items',              examples: ['/get_display_list'],                        params: [] },
            'get_all_nodes_layout': { desc: 'Get all nodes with layout',           examples: ['/get_all_nodes_layout'],                    params: [] },
            'get_logs':             { desc: 'Get server logs',                     examples: ['/get_logs', '/get_logs since_request_id 5'], params: [{ name: 'since_request_id', type: 'number', placeholder: '0' }] },

            // ── Mouse ──
            'mouse_move':     { desc: 'Move mouse to (x, y)',                      examples: ['/mouse_move x 100 y 200'],                  params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }] },
            'mouse_down':     { desc: 'Mouse button press at (x, y)',              examples: ['/mouse_down x 100 y 200 button left'],      params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }, { name: 'button', type: 'text', placeholder: 'left' }] },
            'mouse_up':       { desc: 'Mouse button release at (x, y)',            examples: ['/mouse_up x 100 y 200 button left'],        params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }, { name: 'button', type: 'text', placeholder: 'left' }] },
            'click':          { desc: 'Click (by selector, text, coords, or ID)',  examples: ['/click selector .btn', '/click text Submit', '/click node_id 42', '/click x 100 y 200'], params: [{ name: 'selector', type: 'text', placeholder: '.btn' }, { name: 'text', type: 'text', placeholder: 'Label' }, { name: 'node_id', type: 'number', placeholder: '42' }, { name: 'x', type: 'number', placeholder: '100' }, { name: 'y', type: 'number', placeholder: '200' }], variants: ['selector', 'text', 'node_id', 'x+y'] },
            'double_click':   { desc: 'Double-click at (x, y)',                    examples: ['/double_click x 100 y 200'],                params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }, { name: 'button', type: 'text', placeholder: 'left' }] },
            'click_node':     { desc: 'Click on node by ID (deprecated)',          examples: ['/click_node node_id 5'],                    params: [{ name: 'node_id', type: 'number', value: 0 }, { name: 'button', type: 'text', placeholder: 'left' }] },
            'scroll':         { desc: 'Scroll at (x, y) by delta',                examples: ['/scroll x 100 y 200 delta_x 0 delta_y -50'], params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }, { name: 'delta_x', type: 'number', value: 0 }, { name: 'delta_y', type: 'number', value: 50 }] },
            'hit_test':       { desc: 'Find node at (x, y)',                       examples: ['/hit_test x 100 y 200'],                    params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }] },

            // ── Keyboard ──
            'key_down':       { desc: 'Key press event',                           examples: ['/key_down key Enter'],                      params: [{ name: 'key', type: 'text', placeholder: 'Enter' }] },
            'key_up':         { desc: 'Key release event',                         examples: ['/key_up key Enter'],                        params: [{ name: 'key', type: 'text', placeholder: 'Enter' }] },
            'text_input':     { desc: 'Type text string',                          examples: ['/text_input text Hello'],                    params: [{ name: 'text', type: 'text', placeholder: 'Hello' }] },

            // ── Window ──
            'resize':         { desc: 'Resize window',                             examples: ['/resize width 800 height 600'],              params: [{ name: 'width', type: 'number', value: 800 }, { name: 'height', type: 'number', value: 600 }] },
            'move':           { desc: 'Move window to (x, y)',                     examples: ['/move x 100 y 100'],                        params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }] },
            'focus':          { desc: 'Focus the window',                          examples: ['/focus'],                                    params: [] },
            'blur':           { desc: 'Blur (unfocus) the window',                 examples: ['/blur'],                                     params: [] },
            'close':          { desc: 'Close the window',                          examples: ['/close'],                                    params: [] },
            'dpi_changed':    { desc: 'Simulate DPI change',                       examples: ['/dpi_changed dpi 2'],                        params: [{ name: 'dpi', type: 'number', value: 1 }] },

            // ── DOM Inspection ──
            'get_node_css_properties': { desc: 'Get computed CSS for a node',      examples: ['/get_node_css_properties node_id 3', '/get_node_css_properties selector .item'], params: [{ name: 'node_id', type: 'number', placeholder: '0' }, { name: 'selector', type: 'text', placeholder: '.item' }], variants: ['node_id', 'selector'] },
            'get_node_layout':         { desc: 'Get position/size of a node',      examples: ['/get_node_layout node_id 3', '/get_node_layout selector .item', '/get_node_layout text Submit'], params: [{ name: 'node_id', type: 'number', placeholder: '0' }, { name: 'selector', type: 'text', placeholder: '.item' }, { name: 'text', type: 'text', placeholder: '' }], variants: ['node_id', 'selector', 'text'] },
            'find_node_by_text':       { desc: 'Find node by text content',        examples: ['/find_node_by_text text "Hello"'],            params: [{ name: 'text', type: 'text', placeholder: 'Hello' }] },

            // ── Scrolling ──
            'get_scroll_states':    { desc: 'Get all scroll positions',            examples: ['/get_scroll_states'],                        params: [] },
            'get_scrollable_nodes': { desc: 'List scrollable nodes',               examples: ['/get_scrollable_nodes'],                     params: [] },
            'scroll_node_by':       { desc: 'Scroll a node by delta',              examples: ['/scroll_node_by node_id 5 delta_x 0 delta_y -50', '/scroll_node_by selector .scrollable delta_y 100'], params: [{ name: 'node_id', type: 'number', placeholder: '5' }, { name: 'selector', type: 'text', placeholder: '.scrollable' }, { name: 'delta_x', type: 'number', value: 0 }, { name: 'delta_y', type: 'number', value: 0 }], variants: ['node_id', 'selector'] },
            'scroll_node_to':       { desc: 'Scroll a node to position',           examples: ['/scroll_node_to node_id 5 x 0 y 100', '/scroll_node_to selector .content x 0 y 500'], params: [{ name: 'node_id', type: 'number', placeholder: '5' }, { name: 'selector', type: 'text', placeholder: '.content' }, { name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }], variants: ['node_id', 'selector'] },
            'scroll_into_view':     { desc: 'Scroll node into view (W3C)',         examples: ['/scroll_into_view selector .item'],           params: [{ name: 'selector', type: 'text', placeholder: '.item' }, { name: 'block', type: 'text', placeholder: 'center' }] },
            'get_scrollbar_info':   { desc: 'Get scrollbar geometry for node',     examples: ['/get_scrollbar_info node_id 5', '/get_scrollbar_info selector .scrollable'], params: [{ name: 'node_id', type: 'number', placeholder: '5' }, { name: 'selector', type: 'text', placeholder: '.scrollable' }, { name: 'orientation', type: 'text', placeholder: 'both' }], variants: ['node_id', 'selector'] },

            // ── Selection / Drag ──
            'get_selection_state':    { desc: 'Get text selection state',           examples: ['/get_selection_state'],                      params: [] },
            'dump_selection_manager': { desc: 'Dump selection manager (debug)',     examples: ['/dump_selection_manager'],                    params: [] },
            'get_drag_state':         { desc: 'Get drag state',                    examples: ['/get_drag_state'],                            params: [] },
            'get_drag_context':       { desc: 'Get drag context (debug)',           examples: ['/get_drag_context'],                          params: [] },
            'get_focus_state':        { desc: 'Get current focus node',            examples: ['/get_focus_state'],                            params: [] },
            'get_cursor_state':       { desc: 'Get cursor position/blink',         examples: ['/get_cursor_state'],                           params: [] },

            // ── Control ──
            'relayout':        { desc: 'Force re-layout',                          examples: ['/relayout'],                                  params: [] },
            'redraw':          { desc: 'Force redraw',                             examples: ['/redraw'],                                    params: [] },
            'wait':            { desc: 'Wait milliseconds',                        examples: ['/wait ms 500'],                               params: [{ name: 'ms', type: 'number', value: 500 }] },
            'wait_frame':      { desc: 'Wait for next frame',                      examples: ['/wait_frame'],                                params: [] },

            // ── Screenshots ──
            'take_screenshot':        { desc: 'Take screenshot (SW render)',       examples: ['/take_screenshot'],                           params: [] },
            'take_native_screenshot': { desc: 'Take native OS screenshot',         examples: ['/take_native_screenshot'],                    params: [] },

            // ── App State ──
            'get_app_state':   { desc: 'Get global app state as JSON',             examples: ['/get_app_state'],                             params: [] },
            'set_app_state':   { desc: 'Set global app state from JSON',           examples: ['/set_app_state state {"counter":0}'],          params: [{ name: 'state', type: 'text', placeholder: '{"counter": 0}' }] },
            'get_node_dataset': { desc: 'Get node dataset RefAny as JSON',         examples: ['/get_node_dataset node_id 3'],                params: [{ name: 'node_id', type: 'number', value: 0 }] },

            // ── DOM Mutation ──
            'insert_node':     { desc: 'Insert child node',                        examples: ['/insert_node parent_id 0 node_type div'],     params: [{ name: 'parent_id', type: 'number', value: 0 }, { name: 'node_type', type: 'text', placeholder: 'div' }, { name: 'position', type: 'number', placeholder: '' }] },
            'delete_node':     { desc: 'Delete a node',                            examples: ['/delete_node node_id 5'],                     params: [{ name: 'node_id', type: 'number', value: 0 }] },
            'set_node_text':   { desc: 'Set text content of a node',               examples: ['/set_node_text node_id 3 text "Hello"'],      params: [{ name: 'node_id', type: 'number', value: 0 }, { name: 'text', type: 'text', placeholder: 'Hello' }] },
            'set_node_classes':{ desc: 'Set CSS classes on a node',                examples: ['/set_node_classes node_id 3 classes btn primary'], params: [{ name: 'node_id', type: 'number', value: 0 }, { name: 'classes', type: 'text', placeholder: 'btn primary' }] },
            'set_node_css_override': { desc: 'Override a CSS property on a node',  examples: ['/set_node_css_override node_id 3 property width value 100px'], params: [{ name: 'node_id', type: 'number', value: 0 }, { name: 'property', type: 'text', placeholder: 'width' }, { name: 'value', type: 'text', placeholder: '100px' }] },

            // ── Debugging ──
            'resolve_function_pointers': { desc: 'Resolve fn ptrs to symbols',    examples: ['/resolve_function_pointers addresses 0x1234'],  params: [{ name: 'addresses', type: 'text', placeholder: '0x1234,0x5678' }] },
            'get_component_registry':    { desc: 'Get registered components',      examples: ['/get_component_registry'],                     params: [] },
            'get_libraries':             { desc: 'List registered libraries',      examples: ['/get_libraries'],                              params: [] },
            'get_library_components':    { desc: 'Get components in library',      examples: ['/get_library_components library builtin'],      params: [{ name: 'library', type: 'text', placeholder: 'builtin' }] },
            'import_component_library':  { desc: 'Import component library (JSON)', examples: ['/import_component_library'],                    params: [{ name: 'library', type: 'text', placeholder: '{"name":"mylib","version":"1.0","components":[]}' }] },
            'export_component_library':  { desc: 'Export component library (JSON)', examples: ['/export_component_library library mylib'],       params: [{ name: 'library', type: 'text', placeholder: 'mylib' }] },
            'export_code':               { desc: 'Export app code for language',     examples: ['/export_code language rust'],                    params: [{ name: 'language', type: 'text', placeholder: 'rust' }] },
            'export_code_zip':           { desc: 'Export code as ZIP download',     examples: ['/export_code_zip language rust'],                 params: [{ name: 'language', type: 'text', placeholder: 'rust' }, { name: 'library', type: 'text', placeholder: 'mylib' }] },
            'create_library':            { desc: 'Create new component library',    examples: ['/create_library name mylib'],                    params: [{ name: 'name', type: 'text', placeholder: 'mylib' }] },
            'delete_library':            { desc: 'Delete a component library',      examples: ['/delete_library name mylib'],                    params: [{ name: 'name', type: 'text', placeholder: 'mylib' }] },
            'create_component':          { desc: 'Create component in library',     examples: ['/create_component library mylib name mycomp'],   params: [{ name: 'library', type: 'text', placeholder: 'mylib' }, { name: 'name', type: 'text', placeholder: 'mycomp' }] },
            'delete_component':          { desc: 'Delete component from library',   examples: ['/delete_component library mylib name mycomp'],   params: [{ name: 'library', type: 'text', placeholder: 'mylib' }, { name: 'name', type: 'text', placeholder: 'mycomp' }] },
            'update_component':          { desc: 'Update component properties',     examples: ['/update_component library mylib name mycomp'],   params: [{ name: 'library', type: 'text', placeholder: 'mylib' }, { name: 'name', type: 'text', placeholder: 'mycomp' }] },
            'get_component_preview':      { desc: 'Render component to PNG image',  examples: ['/get_component_preview library builtin name button', '/get_component_preview library mylib name card width 400 height 300', '/get_component_preview library mylib name card width 400 height 300 dpi 2 background #ffffff override_os mac override_theme dark override_lang en'], params: [{ name: 'library', type: 'text', placeholder: 'builtin' }, { name: 'name', type: 'text', placeholder: 'button' }, { name: 'width', type: 'number', placeholder: '400', optional: true }, { name: 'height', type: 'number', placeholder: '300', optional: true }, { name: 'dpi', type: 'number', placeholder: '1', optional: true }, { name: 'background', type: 'text', placeholder: '#ffffff', optional: true }, { name: 'css_override', type: 'text', placeholder: '.root { color: red; }', optional: true }, { name: 'args', type: 'text', placeholder: '{"label":"Click"}', optional: true }, { name: 'override_os', type: 'text', placeholder: 'mac', optional: true }, { name: 'override_theme', type: 'text', placeholder: 'dark', optional: true }, { name: 'override_lang', type: 'text', placeholder: 'en', optional: true }], variants: ['library+name', 'library+name+size', 'library+name+all'] },
            'get_component_render_tree': { desc: 'Get component render output tree', examples: ['/get_component_render_tree library mylib name mycomp'], params: [{ name: 'library', type: 'text', placeholder: 'mylib' }, { name: 'name', type: 'text', placeholder: 'mycomp' }] },
            'get_component_source':      { desc: 'Get component source code',       examples: ['/get_component_source library mylib name mycomp source_type render_fn', '/get_component_source library mylib name mycomp source_type compile_fn language rust'], params: [{ name: 'library', type: 'text', placeholder: 'mylib' }, { name: 'name', type: 'text', placeholder: 'mycomp' }, { name: 'source_type', type: 'text', placeholder: 'render_fn' }, { name: 'language', type: 'text', placeholder: 'rust', optional: true }] },
            'update_component_render_fn': { desc: 'Update component render_fn',     examples: ['/update_component_render_fn library mylib name mycomp source "fn render..."'], params: [{ name: 'library', type: 'text', placeholder: 'mylib' }, { name: 'name', type: 'text', placeholder: 'mycomp' }, { name: 'source', type: 'text', placeholder: '...' }] },
            'update_component_compile_fn': { desc: 'Update component compile_fn',   examples: ['/update_component_compile_fn library mylib name mycomp language rust source "fn compile..."'], params: [{ name: 'library', type: 'text', placeholder: 'mylib' }, { name: 'name', type: 'text', placeholder: 'mycomp' }, { name: 'source', type: 'text', placeholder: '...' }, { name: 'language', type: 'text', placeholder: 'rust' }] },

            // ── File ──
            'open_file':      { desc: 'Open source file in editor',             examples: ['/open_file file /path/to/file.rs', '/open_file file /path/to/file.rs line 42'], params: [{ name: 'file', type: 'text', placeholder: '/path/to/file.rs' }, { name: 'line', type: 'number', placeholder: '0', optional: true }] },

            // ── Snapshots ──
            'restore_snapshot': { desc: 'Restore saved app state snapshot',       examples: ['/restore_snapshot alias initial-state'],         params: [{ name: 'alias', type: 'text', placeholder: 'initial-state' }] },

            // ── E2E ──
            'run_e2e_tests':  { desc: 'Run E2E test suite on server',              examples: ['/run_e2e_tests'],                              params: [] },

            // ── Assertions (virtual, used in E2E steps) ──
            'assert_text':       { desc: 'Assert node text equals expected',       examples: ['/assert_text selector .label expected Hello'],  params: [{ name: 'selector', type: 'text', placeholder: '.label' }, { name: 'expected', type: 'text', placeholder: 'Hello' }] },
            'assert_exists':     { desc: 'Assert element exists',                  examples: ['/assert_exists selector .element'],             params: [{ name: 'selector', type: 'text', placeholder: '.element' }] },
            'assert_not_exists': { desc: 'Assert element does NOT exist',          examples: ['/assert_not_exists selector .gone'],            params: [{ name: 'selector', type: 'text', placeholder: '.gone' }] },
            'assert_node_count': { desc: 'Assert selector matches N nodes',        examples: ['/assert_node_count selector li expected 5'],    params: [{ name: 'selector', type: 'text', placeholder: 'li' }, { name: 'expected', type: 'number', value: 5 }] },
            'assert_layout':     { desc: 'Assert layout property value',           examples: ['/assert_layout selector .box property width expected 100 tolerance 1'], params: [{ name: 'selector', type: 'text', placeholder: '.box' }, { name: 'property', type: 'text', placeholder: 'width' }, { name: 'expected', type: 'number', value: 100 }, { name: 'tolerance', type: 'number', value: 1 }] },
            'assert_app_state':  { desc: 'Assert app state path value',            examples: ['/assert_app_state path counter expected 42'],   params: [{ name: 'path', type: 'text', placeholder: 'counter' }, { name: 'expected', type: 'text', placeholder: '42' }] },
        }
    },

    /* ================================================================
     * INIT
     * ================================================================ */
    init: async function() {
        console.log('[dbg] init, apiUrl =', this.config.apiUrl);
        if (window.location.port) this.config.apiUrl = window.location.origin;

        // Load saved state
        var saved = localStorage.getItem('azul_debugger');
        if (saved) {
            try {
                var s = JSON.parse(saved);
                if (s.tests) this.state.tests = s.tests;
                if (s.cssOverrides) this.state.cssOverrides = s.cssOverrides;
                if (s.snapshots) this.state.snapshots = s.snapshots;
                // Restore extra persistent state
                if (s.currentView) this.state.currentView = s.currentView;
                if (s.activeTestId) this.state.activeTestId = s.activeTestId;
                if (s.selectedLibrary) this.state.selectedLibrary = s.selectedLibrary;
                if (s.selectedNodeId != null) this.state.selectedNodeId = s.selectedNodeId;
                if (s.selectedComponentIdx != null) this.state.selectedComponentIdx = s.selectedComponentIdx;
                if (s.collapsedNodes && Array.isArray(s.collapsedNodes)) this.state.collapsedNodes = new Set(s.collapsedNodes);
                if (s.previewOs !== undefined) this.state.previewOs = s.previewOs;
                if (s.previewTheme !== undefined) this.state.previewTheme = s.previewTheme;
                if (s.previewLang !== undefined) this.state.previewLang = s.previewLang;
            } catch(e) { console.warn('[dbg] bad localStorage:', e); }
        }
        if (!this.state.tests.length) this.handlers.newTest();

        this._initMenubar();
        this.resizer.init();

        // Global click to close context menu
        document.addEventListener('click', function() {
            document.getElementById('context-menu').classList.add('hidden');
        });

        // Connect
        try {
            await this.api.post({ op: 'get_state' });
            document.getElementById('connection-status').innerText = 'Connected';
            document.getElementById('connection-status').style.color = 'var(--success)';
            this.log('Connected to ' + this.config.apiUrl, 'info');
        } catch(e) {
            this.config.isMock = true;
            document.getElementById('connection-status').innerText = 'Mock';
            this.log('Connection failed — Mock Mode', 'warning');
        }

        this.ui.renderTestList();
        this.handlers.refreshSidebar();
        this.handlers.loadAppState();
        this.handlers.loadWindowState();

        // Restore view from persisted state
        if (this.state.currentView && this.state.currentView !== 'inspector') {
            this.ui.switchView(this.state.currentView);
        }
    },

    _initMenubar: function() {
        var menubar = document.getElementById('menubar');
        var items = menubar.querySelectorAll('.menu-item[data-menu]');
        var self = this;
        items.forEach(function(mi) {
            mi.addEventListener('click', function(e) {
                e.stopPropagation();
                var wasOpen = mi.classList.contains('open');
                items.forEach(function(x) { x.classList.remove('open'); });
                if (!wasOpen) {
                    mi.classList.add('open');
                    self.state.openMenu = mi.dataset.menu;
                } else {
                    self.state.openMenu = null;
                }
            });
            mi.addEventListener('mouseenter', function() {
                if (self.state.openMenu && self.state.openMenu !== mi.dataset.menu) {
                    items.forEach(function(x) { x.classList.remove('open'); });
                    mi.classList.add('open');
                    self.state.openMenu = mi.dataset.menu;
                }
            });
        });
        document.addEventListener('click', function() {
            items.forEach(function(x) { x.classList.remove('open'); });
            self.state.openMenu = null;
        });
        menubar.querySelectorAll('.menu-dropdown-item').forEach(function(di) {
            di.addEventListener('click', function(e) {
                e.stopPropagation();
                items.forEach(function(x) { x.classList.remove('open'); });
                self.state.openMenu = null;
            });
        });
    },

    /* ================================================================
     * LOGGING
     * ================================================================ */
    log: function(msg, type) {
        type = type || 'info';
        this._appendLog(document.getElementById('panel-terminal'), msg, type);
    },
    debugLog: function(msg, type) {
        type = type || 'info';
        this._appendLog(document.getElementById('panel-debug'), msg, type);
    },
    _appendLog: function(panel, msg, type) {
        if (!panel) return;
        var div = document.createElement('div');
        div.className = 'log-entry ' + type;
        var time = new Date().toLocaleTimeString('en', { hour12: false });

        // Check for base64 image data in the message (screenshots)
        var imgData = null;
        if (typeof msg === 'object') {
            // Look for screenshot data in response
            imgData = _extractBase64Image(msg);
        } else if (typeof msg === 'string') {
            try {
                var parsed = JSON.parse(msg);
                imgData = _extractBase64Image(parsed);
            } catch(e) { /* not JSON, that's fine */ }
        }

        if (imgData) {
            var text = typeof msg === 'object' ? JSON.stringify(msg).substring(0, 100) + '...' : msg.substring(0, 100) + '...';
            div.innerHTML = '[' + time + '] Screenshot received';
            panel.appendChild(div);
            var imgDiv = document.createElement('div');
            imgDiv.className = 'log-entry-image';
            var img = document.createElement('img');
            img.src = imgData.startsWith('data:') ? imgData : 'data:image/png;base64,' + imgData;
            img.style.cssText = 'max-width:100%;max-height:300px;cursor:pointer;border:1px solid var(--border);border-radius:3px;margin:4px 0;';
            img.onclick = function() { app.ui.showScreenshot(img.src); };
            imgDiv.appendChild(img);
            panel.appendChild(imgDiv);
        } else {
            var text = typeof msg === 'object' ? JSON.stringify(msg) : msg;
            div.textContent = '[' + time + '] ' + text;
            panel.appendChild(div);
        }
        panel.scrollTop = panel.scrollHeight;
    },

    /* ================================================================
     * AUTOCOMPLETE (slash commands in terminal)
     * ================================================================ */
    _acSelectedIndex: -1,

    _buildSlashTemplate: function(cmd) {
        var schema = this.schema.commands[cmd];
        if (!schema) return '/' + cmd;
        var parts = ['/' + cmd];
        var params = schema.params || [];
        params.forEach(function(p) {
            var val = (p.value != null) ? String(p.value) : (p.placeholder || '');
            parts.push(p.name);
            parts.push(val);
        });
        return parts.join(' ');
    },

    _showAutocomplete: function(filter) {
        var existing = document.getElementById('autocomplete-popup');
        if (existing) existing.remove();
        this._acSelectedIndex = -1;

        var cmdNames = Object.keys(this.schema.commands);
        var matches = filter
            ? cmdNames.filter(function(c) { return c.indexOf(filter) !== -1; })
            : cmdNames;
        if (!matches.length) return;

        var menu = document.createElement('div');
        menu.id = 'autocomplete-popup';
        menu.className = 'autocomplete-menu';

        var self = this;
        matches.forEach(function(cmd, idx) {
            var schema = self.schema.commands[cmd];
            var item = document.createElement('div');
            item.className = 'autocomplete-item';
            item.dataset.idx = idx;
            item.dataset.cmd = cmd;
            var examplesArr = schema.examples || (schema.example ? [schema.example] : []);
            var extraExamples = examplesArr.slice(0);

            // Build template string with all params (including optional)
            var template = self._buildSlashTemplate(cmd);

            var html = '<div class="autocomplete-main">' +
                '<span class="autocomplete-cmd">/' + esc(cmd) + '</span>' +
                '<span class="autocomplete-desc">' + esc(schema.desc || '') + '</span>' +
                '</div>';

            // Clickable examples — each one pastes into terminal on click
            if (extraExamples.length) {
                html += '<div class="autocomplete-examples">';
                extraExamples.forEach(function(ex) {
                    html += '<div class="autocomplete-example-item" data-example="' + esc(ex) + '" title="Click to paste into terminal">' + esc(ex) + '</div>';
                });
                html += '</div>';
            }

            // Show optional params hint
            var optParams = (schema.params || []).filter(function(p) { return p.optional; });
            if (optParams.length) {
                var optNames = optParams.map(function(p) { return p.name; }).join(', ');
                html += '<div class="autocomplete-optional">optional: ' + esc(optNames) + '</div>';
            }

            item.innerHTML = html;

            // Clicking the main item pastes the full template (with all params)
            item.addEventListener('mousedown', function(e) {
                // If clicking directly on an example line, paste that example instead
                var exEl = e.target.closest('.autocomplete-example-item');
                if (exEl) {
                    e.preventDefault();
                    var input = document.getElementById('terminal-cmd');
                    input.value = exEl.dataset.example;
                    input.focus();
                    // Place cursor at end
                    input.setSelectionRange(input.value.length, input.value.length);
                    self._hideAutocomplete();
                    return;
                }
                e.preventDefault();
                var input = document.getElementById('terminal-cmd');
                input.value = template;
                input.focus();
                input.setSelectionRange(input.value.length, input.value.length);
                self._hideAutocomplete();
            });
            menu.appendChild(item);
        });

        // Position above the terminal input
        var inputRow = document.querySelector('.terminal-input-row');
        var rect = inputRow.getBoundingClientRect();
        menu.style.bottom = (window.innerHeight - rect.top + 2) + 'px';
        menu.style.left = rect.left + 'px';
        menu.style.width = rect.width + 'px';
        document.body.appendChild(menu);
    },

    _hideAutocomplete: function() {
        var el = document.getElementById('autocomplete-popup');
        if (el) el.remove();
        this._acSelectedIndex = -1;
    },

    _acNavigate: function(direction) {
        var menu = document.getElementById('autocomplete-popup');
        if (!menu) return false;
        var items = menu.querySelectorAll('.autocomplete-item');
        if (!items.length) return false;

        // Remove current highlight
        if (this._acSelectedIndex >= 0 && this._acSelectedIndex < items.length) {
            items[this._acSelectedIndex].classList.remove('ac-selected');
        }

        if (direction === 'up') {
            this._acSelectedIndex = this._acSelectedIndex <= 0 ? items.length - 1 : this._acSelectedIndex - 1;
        } else {
            this._acSelectedIndex = this._acSelectedIndex >= items.length - 1 ? 0 : this._acSelectedIndex + 1;
        }

        items[this._acSelectedIndex].classList.add('ac-selected');
        items[this._acSelectedIndex].scrollIntoView({ block: 'nearest' });
        return true;
    },

    _acAccept: function() {
        var menu = document.getElementById('autocomplete-popup');
        if (!menu) return false;
        var items = menu.querySelectorAll('.autocomplete-item');
        if (this._acSelectedIndex < 0 || this._acSelectedIndex >= items.length) return false;
        var item = items[this._acSelectedIndex];
        var cmd = item.dataset.cmd;
        var template = this._buildSlashTemplate(cmd);
        var input = document.getElementById('terminal-cmd');
        input.value = template;
        input.focus();
        input.setSelectionRange(input.value.length, input.value.length);
        this._hideAutocomplete();
        return true;
    },

    /* ================================================================
     * API
     * ================================================================ */
    api: {
        post: async function(payload) {
            if (app.config.isMock) return app.api.mockResponse(payload);
            var t0 = performance.now();
            app.debugLog(payload, 'request');
            var res = await fetch(app.config.apiUrl, { method: 'POST', body: JSON.stringify(payload) });
            var json = await res.json();
            var ms = Math.round(performance.now() - t0);
            app.debugLog('[' + ms + 'ms] ' + JSON.stringify(json).substring(0, 400), 'response');
            return json;
        },

        postE2e: async function(tests) {
            var testArr = Array.isArray(tests) ? tests : [tests];
            if (app.config.isMock) return app.api.mockE2eResponse(tests);
            var payload = { op: 'run_e2e_tests', tests: testArr, timeout_secs: 300 };
            if (Object.keys(app.state.snapshots).length > 0) payload.snapshots = app.state.snapshots;
            app.debugLog(payload, 'request');
            var res = await fetch(app.config.apiUrl, { method: 'POST', body: JSON.stringify(payload) });
            var json = await res.json();
            app.debugLog(json, 'response');
            if (json.data && json.data.type === 'e2e_results' && json.data.value) {
                return { status: json.status, results: json.data.value.results };
            }
            if (json.data && json.data.E2eResults) {
                return { status: json.status, results: json.data.E2eResults.results };
            }
            return json;
        },

        mockResponse: async function(payload) {
            await new Promise(function(r) { setTimeout(r, 50); });
            if (payload.op === 'get_node_hierarchy') {
                return { status: 'ok', data: { type: 'node_hierarchy', value: {
                    root: 0, node_count: 3, nodes: [
                        { index: 0, type: 'Html', tag: 'Html', children: [1], parent: -1, events: [], rect: { x: 0, y: 0, width: 400, height: 300 }, contenteditable: false },
                        { index: 1, type: 'Body', tag: 'Body', children: [2], parent: 0, events: [], rect: { x: 0, y: 0, width: 400, height: 300 }, contenteditable: false },
                        { index: 2, type: 'Text', tag: 'Text', text: 'Hello World', children: [], parent: 1, events: [], rect: { x: 10, y: 10, width: 80, height: 16 }, contenteditable: false },
                    ]
                }}};
            }
            if (payload.op === 'get_node_css_properties') {
                return { status: 'ok', data: { type: 'node_css_properties', value: { node_id: payload.node_id, property_count: 2, properties: ['display: Block', 'font-size: 13px'] }}};
            }
            if (payload.op === 'get_node_layout') {
                return { status: 'ok', data: { type: 'node_layout', value: {
                    node_id: payload.node_id,
                    size: { width: 100, height: 50 }, position: { x: 0, y: 0 },
                    rect: { x: 0, y: 0, width: 100, height: 50 },
                    margin: { top: 8, right: 0, bottom: 8, left: 0 },
                    border: { top: 1, right: 1, bottom: 1, left: 1 },
                    padding: { top: 4, right: 8, bottom: 4, left: 8 },
                }}};
            }
            if (payload.op === 'get_app_state') {
                return { status: 'ok', data: { type: 'app_state', value: { counter: 0, items: ['a', 'b'], nested: { flag: true } } }};
            }
            if (payload.op === 'get_node_dataset') {
                return { status: 'ok', data: { type: 'node_dataset', value: { node_id: payload.node_id, metadata: { type_id: 0, type_name: 'MockDataset', can_serialize: true, can_deserialize: false, ref_count: 1 }, dataset: { key: 'mock_value' }, error: null }}};
            }
            if (payload.op === 'get_component_registry') {
                return { status: 'ok', data: { type: 'component_registry', value: { libraries: [] }}};
            }
            if (payload.op === 'get_libraries') {
                return { status: 'ok', data: { type: 'library_list', value: { libraries: [{ name: 'builtin', version: '1.0.0', description: 'Built-in HTML elements', exportable: false, component_count: 52 }] }}};
            }
            if (payload.op === 'get_library_components') {
                return { status: 'ok', data: { type: 'library_components', value: { library: payload.library || 'builtin', components: [] }}};
            }
            return { status: 'ok', data: {} };
        },

        mockE2eResponse: async function(tests) {
            await new Promise(function(r) { setTimeout(r, 300); });
            var arr = Array.isArray(tests) ? tests : [tests];
            var results = arr.map(function(t) {
                return {
                    name: t.name || 'Test', status: 'pass', duration_ms: 42,
                    step_count: (t.steps || []).length,
                    steps_passed: (t.steps || []).length, steps_failed: 0,
                    steps: (t.steps || []).map(function(s, i) {
                        return { step_index: i, op: s.op, status: 'pass', duration_ms: 5, logs: [], error: null, response: null };
                    })
                };
            });
            return { status: 'ok', results: results };
        }
    },

    /* ================================================================
     * UI — pure rendering
     * ================================================================ */
    ui: {
        switchView: function(view) {
            app.state.currentView = view;
            app.handlers.save();
            document.querySelectorAll('.activity-icon').forEach(function(el) {
                el.classList.toggle('active', el.dataset.view === view);
            });
            // Sidebar panels
            var panels = ['inspector', 'testing', 'components'];
            panels.forEach(function(p) {
                var sb = document.getElementById('sidebar-' + p);
                if (sb) sb.classList.toggle('hidden', view !== p);
                var v = document.getElementById('view-' + p);
                if (v) v.classList.toggle('hidden', view !== p);
            });
            var titles = { inspector: 'DOM Explorer', testing: 'Test Explorer', components: 'Component Libraries' };
            document.getElementById('sidebar-title').innerText = titles[view] || view;
            var tabTitles = { inspector: 'Inspector', testing: 'runner.e2e', components: 'Component Details' };
            // For testing view, use active test name if available
            if (view === 'testing' && app.state.activeTestId) {
                var activeTest = app.state.tests.find(function(t) { return t.id === app.state.activeTestId; });
                if (activeTest) tabTitles.testing = activeTest.name;
            }
            document.getElementById('tab-title').innerText = tabTitles[view] || view;
            // Hide App State panel and bottom console when in components view
            var appstatePanel = document.getElementById('appstate-panel');
            var appstateResizer = appstatePanel ? appstatePanel.nextElementSibling : null;
            if (appstatePanel) appstatePanel.classList.toggle('hidden', view === 'components');
            if (appstateResizer && appstateResizer.classList.contains('resizer')) appstateResizer.classList.toggle('hidden', view === 'components');
            var bottomPanel = document.getElementById('bottom-panel');
            if (bottomPanel) bottomPanel.classList.toggle('hidden', view === 'components');
            // Load libraries when switching to components view
            if (view === 'components') {
                app.handlers.loadLibraries();
            }
        },

        switchPanel: function(panel) {
            app.state.activePanel = panel;
            document.querySelectorAll('.panel-tab').forEach(function(el) {
                el.classList.toggle('active', el.dataset.panel === panel);
            });
            document.getElementById('panel-terminal').classList.toggle('hidden', panel !== 'terminal');
            document.getElementById('panel-debug').classList.toggle('hidden', panel !== 'debug');
        },

        togglePalette: function() {
            var list = document.getElementById('palette-component-list');
            var icon = document.getElementById('palette-toggle-icon');
            if (!list) return;
            if (list.style.display === 'none') {
                list.style.display = '';
                if (icon) icon.textContent = 'expand_less';
            } else {
                list.style.display = 'none';
                if (icon) icon.textContent = 'expand_more';
            }
        },

        /* ── DOM Tree rendering ── */
        renderDomTree: function(hierarchy, root) {
            var container = document.getElementById('dom-tree-container');
            container.innerHTML = '';
            if (!hierarchy || !hierarchy.length) {
                container.innerHTML = '<div class="placeholder-text">No DOM data.</div>';
                return;
            }
            app.state.hierarchy = hierarchy;
            app.state.hierarchyRoot = root;

            var byIndex = {};
            hierarchy.forEach(function(n) { byIndex[n.index] = n; });
            var rootNode = byIndex[root] || hierarchy[0];
            this._renderTreeNode(container, rootNode, byIndex, 0, false);
        },

        _renderTreeNode: function(container, node, byIndex, depth, insideComponent) {
            if (!node) return;
            var hasChildren = node.children && node.children.length > 0;
            var isCollapsed = app.state.collapsedNodes.has(node.index);
            var isSelected = app.state.selectedNodeId === node.index;

            var isComponentRoot = !insideComponent && node.component && node.component.component_id;
            var isComponentChild = insideComponent;

            var row = document.createElement('div');
            row.className = 'tree-row' + (isSelected ? ' selected' : '') + (isComponentChild ? ' component-internal' : '') + (isComponentRoot ? ' component-root' : '');
            row.dataset.nodeId = node.index;
            row.dataset.type = node.type === 'Text' ? 'text' : 'element';

            // Indent
            var indent = document.createElement('span');
            indent.className = 'tree-indent';
            indent.style.width = (depth * 16 + 4) + 'px';
            row.appendChild(indent);

            // Toggle arrow
            var toggle = document.createElement('span');
            toggle.className = 'tree-toggle';
            if (hasChildren) {
                toggle.textContent = isCollapsed ? '▶' : '▼';
                toggle.addEventListener('click', function(e) {
                    e.stopPropagation();
                    if (app.state.collapsedNodes.has(node.index)) {
                        app.state.collapsedNodes.delete(node.index);
                    } else {
                        app.state.collapsedNodes.add(node.index);
                    }
                    app.ui.renderDomTree(app.state.hierarchy, app.state.hierarchyRoot);
                });
            } else {
                toggle.innerHTML = '&nbsp;';
            }
            row.appendChild(toggle);

            // Label — NO angle brackets, just the lowercase tag name
            var label = document.createElement('span');
            label.className = 'tree-label';

            if (node.type === 'Text') {
                var textSpan = document.createElement('span');
                textSpan.className = 'tree-text-content';
                textSpan.textContent = '"' + (node.text || '') + '"';
                label.appendChild(textSpan);
            } else {
                // Tag name — plain, no angle brackets
                var tagSpan = document.createElement('span');
                tagSpan.className = 'tree-tag';
                tagSpan.textContent = (node.tag || node.type).toLowerCase();
                label.appendChild(tagSpan);

                // ID
                if (node.id) {
                    var idSpan = document.createElement('span');
                    idSpan.className = 'tree-id';
                    idSpan.textContent = ' #' + node.id;
                    label.appendChild(idSpan);
                }

                // Classes
                if (node.classes && node.classes.length) {
                    var clsSpan = document.createElement('span');
                    clsSpan.className = 'tree-class';
                    clsSpan.textContent = ' .' + node.classes.join('.');
                    label.appendChild(clsSpan);
                }
            }

            // Event badges
            if (node.events && node.events.length) {
                var badge = document.createElement('span');
                badge.className = 'tree-event-badge';
                badge.textContent = '⚡' + node.events.length;
                badge.title = node.events.map(function(e) { return e.event; }).join(', ');
                label.appendChild(badge);
            }

            // Component origin badge
            if (node.component && node.component.component_id) {
                var compBadge = document.createElement('span');
                compBadge.className = 'tree-component-badge';
                var shortName = node.component.component_id;
                var lastColon = shortName.lastIndexOf(':');
                if (lastColon >= 0) shortName = shortName.substring(lastColon + 1);
                compBadge.textContent = shortName;
                var dm = node.component.data_model;
                if (dm != null && typeof dm === 'object') {
                    compBadge.title = JSON.stringify(dm);
                } else if (dm != null) {
                    compBadge.title = String(dm);
                }
                label.appendChild(compBadge);
            }

            // Dataset badge
            if (node.has_dataset) {
                var dsBadge = document.createElement('span');
                dsBadge.className = 'tree-dataset-badge';
                dsBadge.textContent = 'D';
                dsBadge.title = 'Has dataset';
                label.appendChild(dsBadge);
            }

            row.appendChild(label);

            // Click to select
            row.addEventListener('click', function(e) {
                e.stopPropagation();
                app.handlers.nodeSelected(node.index);
            });

            // Right-click context menu
            row.addEventListener('contextmenu', function(e) {
                e.preventDefault();
                e.stopPropagation();
                app.state.contextMenuNodeId = node.index;
                var menu = document.getElementById('context-menu');
                menu.classList.remove('hidden');
                menu.style.left = e.clientX + 'px';
                menu.style.top = e.clientY + 'px';
            });

            container.appendChild(row);

            // Children
            if (hasChildren && !isCollapsed) {
                var self = this;
                var childInsideComponent = insideComponent || !!isComponentRoot;
                node.children.forEach(function(childIdx) {
                    self._renderTreeNode(container, byIndex[childIdx], byIndex, depth + 1, childInsideComponent);
                });
            }
        },

        /* ── Node detail panel (main editor area) ── */
        renderNodeDetail: function(node) {
            var panel = document.getElementById('node-detail-panel');
            if (!node) {
                panel.innerHTML = '<div class="placeholder-text">Select a node in the DOM Explorer to inspect it.</div>';
                return;
            }

            var html = '';

            // Top section: Node info + Screenshot side by side
            html += '<div class="detail-top-row">';

            // Left: Node info
            html += '<div class="detail-node-info">';
            html += '<div class="detail-section-header">Node #' + node.index + '</div>';
            html += '<div class="detail-row"><span class="detail-key">type</span><span class="detail-value">' + esc(node.type) + '</span></div>';
            if (node.tag) html += '<div class="detail-row"><span class="detail-key">tag</span><span class="detail-value">' + esc(node.tag) + '</span></div>';
            if (node.id) html += '<div class="detail-row"><span class="detail-key">id</span><span class="detail-value">' + esc(node.id) + '</span></div>';
            if (node.classes && node.classes.length) html += '<div class="detail-row"><span class="detail-key">classes</span><span class="detail-value">' + esc(node.classes.join(' ')) + '</span></div>';
            if (node.text) html += '<div class="detail-row"><span class="detail-key">text</span><span class="detail-value">' + esc(node.text) + '</span></div>';

            // Component origin
            if (node.component && node.component.component_id) {
                html += '<div class="detail-row"><span class="detail-key">component</span><span class="detail-value detail-component-id">' + esc(node.component.component_id) + '</span></div>';
                var dm = node.component.data_model;
                if (dm != null) {
                    if (typeof dm === 'object' && !Array.isArray(dm)) {
                        // Object: show key-value rows
                        Object.keys(dm).forEach(function(k) {
                            var val = typeof dm[k] === 'object' ? JSON.stringify(dm[k]) : String(dm[k]);
                            html += '<div class="detail-row" style="padding-left:12px"><span class="detail-key">' + esc(k) + '</span><span class="detail-value">' + esc(val) + '</span></div>';
                        });
                    } else {
                        // Primitive or array: show as JSON string
                        html += '<div class="detail-row" style="padding-left:12px"><span class="detail-key">value</span><span class="detail-value">' + esc(JSON.stringify(dm)) + '</span></div>';
                    }
                }
            }

            // Dataset indicator
            if (node.has_dataset) {
                html += '<div class="detail-row"><span class="detail-key">dataset</span><span class="detail-value" style="color:var(--warning)">present</span></div>';
            }

            html += '</div>';

            // Right: Native screenshot — placeholder, filled by async fetch
            html += '<div id="node-screenshot" class="detail-screenshot-col">';
            html += '<div class="detail-section-header">Screenshot</div>';
            html += '<div class="placeholder-text" style="text-align:center">Loading...</div>';
            html += '</div>';

            // Box Model (Chrome-style) — next to screenshot, filled by async fetch
            html += '<div id="node-box-model" class="detail-layout-col">';
            html += '<div class="detail-section-header">Layout</div>';
            html += '<div class="placeholder-text">Loading...</div>';
            html += '</div>';

            html += '</div>'; // detail-top-row

            // Events
            if (node.events && node.events.length) {
                html += '<div class="detail-section">';
                html += '<div class="detail-section-header">Event Handlers (' + node.events.length + ')</div>';
                node.events.forEach(function(ev) {
                    html += '<div class="event-row">';
                    html += '<span class="event-type">' + esc(ev.event) + '</span>';
                    html += '<span class="event-ptr" title="Click to resolve" data-addr="' + esc(ev.callback_ptr) + '" onclick="app.handlers.resolvePtr(this)">' + esc(ev.callback_ptr) + '</span>';
                    html += '</div>';
                });
                html += '</div>';
            }

            // Unified CSS Properties section (merged: display + add override)
            html += '<div id="node-css-section" class="detail-section">';
            html += '<div class="detail-section-header">CSS Properties</div>';
            html += '<div class="placeholder-text">Loading...</div>';
            html += '</div>';

            // Accessibility section
            html += '<div id="node-a11y-section" class="detail-section">';
            html += '<div class="detail-section-header">Accessibility</div>';
            var hasA11y = false;
            if (node.tab_index != null) { html += '<div class="detail-row"><span class="detail-key">tabindex</span><span class="detail-value">' + node.tab_index + '</span></div>'; hasA11y = true; }
            if (node.contenteditable) { html += '<div class="detail-row"><span class="detail-key">contenteditable</span><span class="detail-value">true</span></div>'; hasA11y = true; }
            if (node.role) { html += '<div class="detail-row"><span class="detail-key">role</span><span class="detail-value">' + esc(node.role) + '</span></div>'; hasA11y = true; }
            if (node.aria_label) { html += '<div class="detail-row"><span class="detail-key">aria-label</span><span class="detail-value">' + esc(node.aria_label) + '</span></div>'; hasA11y = true; }
            if (node.focusable) { html += '<div class="detail-row"><span class="detail-key">focusable</span><span class="detail-value">true</span></div>'; hasA11y = true; }
            if (!hasA11y) html += '<div class="placeholder-text" style="padding:4px 0">No accessibility attributes set.</div>';
            html += '</div>';

            // Clip mask section (loaded async)
            html += '<div id="node-clip-section" class="detail-section">';
            html += '<div class="detail-section-header">Clip / Scroll Nesting</div>';
            html += '<div class="placeholder-text">Loading...</div>';
            html += '</div>';

            panel.innerHTML = html;

            // Fetch screenshot, layout (for box model) and CSS properties async
            app._loadNodeScreenshot();
            app._loadNodeBoxModel(node.index);
            app._loadNodeCssProperties(node.index);
            app._loadNodeClipInfo(node.index);
        },

        renderTestList: function() {
            var container = document.getElementById('test-list-container');
            container.innerHTML = '';
            app.state.tests.forEach(function(test) {
                var div = document.createElement('div');
                div.className = 'list-item' + (app.state.activeTestId === test.id ? ' selected' : '');
                div.onclick = function() { app.handlers.selectTest(test.id); };
                var icon = test._result ? (test._result.status === 'pass' ? 'check_circle' : 'cancel') : 'description';
                var iconColor = test._result ? (test._result.status === 'pass' ? 'var(--success)' : 'var(--error)') : 'inherit';

                var iconSpan = document.createElement('span');
                iconSpan.className = 'material-icons';
                iconSpan.style.cssText = 'font-size:14px;color:' + iconColor;
                iconSpan.textContent = icon;
                div.appendChild(iconSpan);

                var nameSpan = document.createElement('span');
                nameSpan.className = 'test-name-editable';
                nameSpan.textContent = test.name;

                var startEdit = function(e) {
                    e.stopPropagation();
                    var inp = document.createElement('input');
                    inp.type = 'text';
                    inp.value = test.name;
                    inp.className = 'test-name-input';
                    inp.onblur = function() {
                        test.name = inp.value.trim() || test.name;
                        app.handlers.save();
                        app.ui.renderTestList();
                        // Sync tab title if this is the active test
                        if (app.state.activeTestId === test.id) {
                            document.getElementById('tab-title').innerText = test.name;
                        }
                    };
                    inp.onkeydown = function(ev) {
                        if (ev.key === 'Enter') inp.blur();
                        if (ev.key === 'Escape') { app.ui.renderTestList(); }
                    };
                    nameSpan.innerHTML = '';
                    nameSpan.appendChild(inp);
                    inp.focus();
                    inp.select();
                };

                nameSpan.ondblclick = startEdit;
                div.appendChild(nameSpan);

                // Pencil icon for rename
                var pencil = document.createElement('span');
                pencil.className = 'edit-icon material-icons';
                pencil.textContent = 'edit';
                pencil.title = 'Rename test';
                pencil.onclick = startEdit;
                div.appendChild(pencil);

                container.appendChild(div);
            });
        },

        renderSteps: function() {
            var container = document.getElementById('steps-container');
            container.innerHTML = '';
            var activeTest = app.state.tests.find(function(t) { return t.id === app.state.activeTestId; });
            if (!activeTest) return;

            activeTest.steps.forEach(function(step, idx) {
                var div = document.createElement('div');
                var cls = '';
                if (step.status === 'pass') cls = 'pass';
                if (step.status === 'fail') cls = 'fail';
                if (app.state.currentStepIndex === idx) cls = 'active';
                div.className = 'step-item ' + cls;

                var thumbHtml = step.screenshot
                    ? '<img class="screenshot-thumb" src="' + step.screenshot + '" style="max-width:40px;max-height:24px;margin-left:5px;vertical-align:middle;cursor:pointer" onclick="app.ui.showScreenshot(this.src)">'
                    : '';

                // Compact param display: only show if params exist and have values
                var paramEntries = Object.entries(step.params || {}).filter(function(e) { return e[1] !== '' && e[1] !== undefined && e[1] !== null; });
                var paramStr = paramEntries.length ? paramEntries.map(function(e) { return e[0] + '=' + e[1]; }).join(' ') : '';
                var durationStr = step.duration_ms != null ? ' <span style="color:var(--text-muted)">' + step.duration_ms + 'ms</span>' : '';

                div.innerHTML =
                    '<div class="step-gutter"><div class="breakpoint ' + (step.breakpoint ? 'active' : '') + '" onclick="app.handlers.toggleBreakpoint(' + idx + ', event)"></div></div>' +
                    '<div class="step-content" onclick="app.ui.showStepDetails(' + idx + ')">' +
                    '<span class="step-title">' + esc(step.op) + '</span>' +
                    (paramStr ? ' <span class="step-params">' + esc(paramStr) + '</span>' : '') +
                    durationStr + thumbHtml +
                    (step.error ? '<div style="color:var(--error);font-size:10px">' + esc(step.error) + '</div>' : '') +
                    '</div>' +
                    '<div class="step-gutter"><span class="material-icons" style="font-size:14px;cursor:pointer" onclick="app.handlers.deleteStep(' + idx + ', event)">close</span></div>';
                container.appendChild(div);
            });
        },

        showScreenshot: function(src) {
            document.getElementById('screenshot-modal-img').src = src;
            document.getElementById('screenshot-modal').classList.add('active');
        },

        renderSnapshots: function() {
            var container = document.getElementById('snapshots-container');
            if (!container) return;
            var aliases = Object.keys(app.state.snapshots);
            if (!aliases.length) {
                container.innerHTML = '<div class="placeholder-text" style="padding:4px 8px;font-size:11px">No snapshots saved.</div>';
                return;
            }
            container.innerHTML = '';
            aliases.forEach(function(alias) {
                var row = document.createElement('div');
                row.className = 'snapshot-row';

                var nameSpan = document.createElement('span');
                nameSpan.className = 'snapshot-name';
                nameSpan.textContent = alias;

                var startRename = function(e) {
                    e.stopPropagation();
                    var inp = document.createElement('input');
                    inp.type = 'text';
                    inp.value = alias;
                    inp.className = 'test-name-input';
                    inp.style.cssText = 'width:80px;font-size:11px;';
                    inp.onblur = function() {
                        var newName = inp.value.trim();
                        if (newName && newName !== alias) {
                            app._renameSnapshot(alias, newName);
                        } else {
                            app.ui.renderSnapshots();
                        }
                    };
                    inp.onkeydown = function(ev) {
                        if (ev.key === 'Enter') inp.blur();
                        if (ev.key === 'Escape') app.ui.renderSnapshots();
                    };
                    nameSpan.innerHTML = '';
                    nameSpan.appendChild(inp);
                    inp.focus();
                    inp.select();
                };

                nameSpan.ondblclick = startRename;
                row.appendChild(nameSpan);

                // Pencil icon
                var pencil = document.createElement('span');
                pencil.className = 'edit-icon material-icons';
                pencil.textContent = 'edit';
                pencil.title = 'Rename';
                pencil.onclick = startRename;
                row.appendChild(pencil);

                // Restore button
                var restoreBtn = document.createElement('span');
                restoreBtn.className = 'material-icons clickable snapshot-action';
                restoreBtn.textContent = 'restore';
                restoreBtn.title = 'Restore this snapshot';
                restoreBtn.onclick = function(e) { e.stopPropagation(); app._restoreSnapshot(alias); };
                row.appendChild(restoreBtn);

                // Delete button
                var deleteBtn = document.createElement('span');
                deleteBtn.className = 'material-icons clickable snapshot-action';
                deleteBtn.textContent = 'delete';
                deleteBtn.title = 'Delete snapshot';
                deleteBtn.style.color = 'var(--error)';
                deleteBtn.onclick = function(e) { e.stopPropagation(); app._deleteSnapshot(alias); };
                row.appendChild(deleteBtn);

                container.appendChild(row);
            });
        },

        showAddStepInline: function() {
            var container = document.getElementById('details-content');
            var ops = '<select id="new-step-op" class="form-control" onchange="app.ui.updateStepParamsForm()">';
            for (var op in app.schema.commands) ops += '<option value="' + op + '">' + op + '</option>';
            ops += '</select>';
            container.innerHTML =
                '<div class="add-step-form">' +
                '<div class="form-group"><label class="form-label">Operation</label>' + ops + '</div>' +
                '<div id="step-variant-selector"></div>' +
                '<div id="step-params-container"></div>' +
                '<div style="padding:6px 10px"><button class="btn-sm" onclick="app.handlers.addStepFromForm()">Add Step</button></div>' +
                '</div>';
            this.updateStepParamsForm();
        },

        showAddStepForm: function() {
            this.showAddStepInline();
        },

        updateStepParamsForm: function() {
            var op = document.getElementById('new-step-op').value;
            var schema = app.schema.commands[op];
            var container = document.getElementById('step-params-container');
            var variantContainer = document.getElementById('step-variant-selector');
            var params = schema.params || [];
            var variants = schema.variants || [];

            // Variant selector (if command has variants)
            if (variants.length && variantContainer) {
                var activeVariant = variantContainer.dataset.active || variants[0];
                var vhtml = '<div class="form-group"><label class="form-label">Target by</label><div class="variant-tabs">';
                variants.forEach(function(v) {
                    vhtml += '<span class="variant-tab' + (v === activeVariant ? ' active' : '') + '" onclick="this.parentElement.parentElement.parentElement.dataset.active=\'' + v + '\'; app.ui.updateStepParamsForm()">' + esc(v) + '</span>';
                });
                vhtml += '</div></div>';
                variantContainer.innerHTML = vhtml;
                variantContainer.dataset.active = activeVariant;

                // Filter params: show only params that match the variant
                var variantFields = activeVariant.split('+');
                // Always show non-variant params too (those not in any variant group)
                var allVariantFields = [];
                variants.forEach(function(v) { v.split('+').forEach(function(f) { allVariantFields.push(f); }); });

                params = params.filter(function(p) {
                    // Show if it's a variant field in the active variant, or not a variant field at all
                    return variantFields.indexOf(p.name) >= 0 || allVariantFields.indexOf(p.name) < 0;
                });
            } else if (variantContainer) {
                variantContainer.innerHTML = '';
            }

            var html = '';
            if (params.length === 0) {
                html = '<div class="form-group"><span style="color:var(--text-muted);font-size:11px">No parameters required.</span></div>';
            } else {
                params.forEach(function(p) {
                    // Special handling: snapshot alias dropdown
                    if (op === 'restore_snapshot' && p.name === 'alias') {
                        var aliases = Object.keys(app.state.snapshots);
                        if (aliases.length === 0) {
                            html += '<div class="form-group"><label class="form-label">alias</label><span style="color:var(--text-muted);font-size:11px">No snapshots saved yet.</span></div>';
                        } else {
                            html += '<div class="form-group"><label class="form-label">alias</label><select class="form-control step-param-input" data-name="alias">';
                            aliases.forEach(function(a) { html += '<option value="' + esc(a) + '">' + esc(a) + '</option>'; });
                            html += '</select></div>';
                        }
                    } else {
                        html += '<div class="form-group"><label class="form-label">' + p.name + ' (' + p.type + ')</label>' +
                            '<input type="' + (p.type === 'number' ? 'number' : 'text') + '" class="form-control step-param-input" data-name="' + p.name + '" placeholder="' + (p.placeholder || '') + '" value="' + (p.value !== undefined ? p.value : '') + '"></div>';
                    }
                });
            }
            container.innerHTML = html;
        },

        showStepDetails: function(idx) {
            var test = app.state.tests.find(function(t) { return t.id === app.state.activeTestId; });
            if (!test) return;
            var step = test.steps[idx];
            var container = document.getElementById('details-content');
            var html = '<h3>Step ' + (idx + 1) + ': ' + esc(step.op) + '</h3>';
            html += '<div class="mono" style="font-size:11px;margin:10px 0;background:var(--bg-input);padding:8px;border-radius:3px;white-space:pre-wrap;">' + esc(JSON.stringify(step.params || {}, null, 2)) + '</div>';
            if (step.status) html += '<div style="margin-bottom:10px"><strong>Status:</strong> <span style="color:' + (step.status === 'pass' ? 'var(--success)' : 'var(--error)') + '">' + step.status.toUpperCase() + '</span></div>';
            if (step.error) html += '<div style="color:var(--error);margin-bottom:10px">' + esc(step.error) + '</div>';
            if (step.screenshot) html += '<div style="margin:10px 0"><img src="' + step.screenshot + '" style="max-width:100%;cursor:pointer;border:1px solid var(--border)" onclick="app.ui.showScreenshot(this.src)"></div>';
            html += '<h4>Response</h4>';
            html += '<div id="step-response-tree" style="max-height:300px;overflow:auto;"></div>';
            container.innerHTML = html;
            // Render response as collapsible JSON tree (read-only)
            if (step.lastResponse) {
                app.json.render('step-response-tree', step.lastResponse, true);
            } else {
                document.getElementById('step-response-tree').innerHTML = '<div class="placeholder-text">Not executed</div>';
            }
        },
    },

    /* ================================================================
     * NODE SCREENSHOT (native OS screenshot displayed in inspector)
     * ================================================================ */
    _loadNodeScreenshot: async function() {
        var section = document.getElementById('node-screenshot');
        if (!section) return;
        try {
            var res = await this.api.post({ op: 'take_native_screenshot' });
            if (res.status === 'ok' && res.data) {
                var d = res.data.value || res.data;
                var imgData = d.data || d.image || d.png || null;
                if (imgData) {
                    var html = '<div class="detail-section-header">Screenshot</div>';
                    html += '<img src="' + imgData + '" style="max-width:100%;max-height:300px;object-fit:contain;border:1px solid var(--border);border-radius:4px;cursor:pointer" ';
                    html += 'title="Click to enlarge" onclick="document.getElementById(\'screenshot-modal-img\').src=this.src;document.getElementById(\'screenshot-modal\').classList.add(\'active\')" />';
                    section.innerHTML = html;
                } else {
                    section.innerHTML = '<div class="detail-section-header">Screenshot</div><div class="placeholder-text">No image data</div>';
                }
            } else {
                section.innerHTML = '<div class="detail-section-header">Screenshot</div><div class="placeholder-text" style="color:var(--text-muted)">Screenshot not available</div>';
            }
        } catch(e) {
            section.innerHTML = '<div class="detail-section-header">Screenshot</div><div class="placeholder-text" style="color:var(--text-muted)">Screenshot not available</div>';
        }
    },

    /* ================================================================
     * BOX MODEL (Chrome-style visualization)
     * ================================================================ */
    _loadNodeBoxModel: async function(nodeId) {
        var section = document.getElementById('node-box-model');
        if (!section) return;
        try {
            var res = await this.api.post({ op: 'get_node_layout', node_id: nodeId });
            var d = (res.data && res.data.value) ? res.data.value : (res.data || {});
            var rect = d.rect || d.size || {};
            var w = round(rect.width || 0);
            var h = round(rect.height || 0);
            var mar = d.margin || { top: 0, right: 0, bottom: 0, left: 0 };
            var bdr = d.border || { top: 0, right: 0, bottom: 0, left: 0 };
            var pad = d.padding || { top: 0, right: 0, bottom: 0, left: 0 };

            var html = '<div class="detail-section-header">Layout</div>';
            html += '<div class="box-model">';
            // Margin box
            html += '<div class="bm-box bm-margin"><span class="bm-label">margin</span>';
            html += '<div class="bm-top"><span class="bm-val">' + round(mar.top) + '</span></div>';
            html += '<div class="bm-mid">';
            html += '<span class="bm-left bm-val">' + round(mar.left) + '</span>';
            // Border box
            html += '<div class="bm-center"><div class="bm-box bm-border"><span class="bm-label">border</span>';
            html += '<div class="bm-top"><span class="bm-val">' + round(bdr.top) + '</span></div>';
            html += '<div class="bm-mid">';
            html += '<span class="bm-left bm-val">' + round(bdr.left) + '</span>';
            // Padding box
            html += '<div class="bm-center"><div class="bm-box bm-padding"><span class="bm-label">padding</span>';
            html += '<div class="bm-top"><span class="bm-val">' + round(pad.top) + '</span></div>';
            html += '<div class="bm-mid">';
            html += '<span class="bm-left bm-val">' + round(pad.left) + '</span>';
            // Content box
            html += '<div class="bm-center"><div class="bm-box bm-content">' + w + ' × ' + h + '</div></div>';
            html += '<span class="bm-right bm-val">' + round(pad.right) + '</span>';
            html += '</div>'; // bm-mid
            html += '<div class="bm-bottom"><span class="bm-val">' + round(pad.bottom) + '</span></div>';
            html += '</div></div>'; // bm-padding, bm-center
            html += '<span class="bm-right bm-val">' + round(bdr.right) + '</span>';
            html += '</div>'; // bm-mid
            html += '<div class="bm-bottom"><span class="bm-val">' + round(bdr.bottom) + '</span></div>';
            html += '</div></div>'; // bm-border, bm-center
            html += '<span class="bm-right bm-val">' + round(mar.right) + '</span>';
            html += '</div>'; // bm-mid
            html += '<div class="bm-bottom"><span class="bm-val">' + round(mar.bottom) + '</span></div>';
            html += '</div>'; // bm-margin
            html += '</div>'; // box-model

            // Position info
            if (d.position || d.rect) {
                var pos = d.position || d.rect;
                html += '<div class="detail-row"><span class="detail-key">x</span><span class="detail-value">' + round(pos.x || 0) + 'px</span></div>';
                html += '<div class="detail-row"><span class="detail-key">y</span><span class="detail-value">' + round(pos.y || 0) + 'px</span></div>';
            }

            section.innerHTML = html;
        } catch(e) {
            section.innerHTML = '<div class="detail-section-header">Layout</div><div class="placeholder-text" style="color:var(--error)">Failed to load layout</div>';
        }
    },

    /* ================================================================
     * CSS PROPERTIES (unified: view + edit inline + add override)
     * ================================================================ */
    _loadNodeCssProperties: async function(nodeId) {
        var section = document.getElementById('node-css-section');
        if (!section) return;
        try {
            var res = await this.api.post({ op: 'get_node_css_properties', node_id: nodeId });
            var props = [];
            if (res.data) {
                var d = res.data.value || res.data;
                props = d.properties || [];
            }
            var overrides = this.state.cssOverrides[nodeId] || {};

            var html = '<div class="detail-section-header">CSS Properties (' + props.length + ')</div>';
            html += '<div class="css-props-scroll">';
            if (props.length === 0) {
                html += '<div class="placeholder-text">No CSS properties set.</div>';
            } else {
                props.forEach(function(propStr) {
                    var colonIdx = propStr.indexOf(':');
                    var name = colonIdx > 0 ? propStr.substring(0, colonIdx).trim() : propStr;
                    var value = colonIdx > 0 ? propStr.substring(colonIdx + 1).trim() : '';
                    var isOverridden = overrides[name] !== undefined;
                    var displayValue = isOverridden ? overrides[name] : value;
                    html += '<div class="css-prop-row">';
                    html += '<span class="css-prop-name">' + esc(name) + '</span>';
                    html += '<span class="css-prop-value' + (isOverridden ? ' overridden' : '') + '" title="Click to edit" onclick="app.handlers.editCssProp(' + nodeId + ', \'' + esc(name) + '\', this)">' + esc(displayValue) + '</span>';
                    html += '</div>';
                });
            }
            html += '</div>'; // css-props-scroll

            // Add new override row
            html += '<div class="css-add-row">';
            html += '<input type="text" class="form-control" id="css-add-name" placeholder="property" style="width:40%;font-size:11px;padding:2px 4px;font-family:monospace;">';
            html += '<input type="text" class="form-control" id="css-add-value" placeholder="value" style="flex:1;font-size:11px;padding:2px 4px;font-family:monospace;" onkeydown="if(event.key===\'Enter\') app.handlers.addCssOverride(' + nodeId + ')">';
            html += '<span class="material-icons clickable" style="font-size:14px" onclick="app.handlers.addCssOverride(' + nodeId + ')" title="Add override">add</span>';
            html += '</div>';

            section.innerHTML = html;
        } catch(e) {
            section.innerHTML = '<div class="detail-section-header">CSS Properties</div><div class="placeholder-text" style="color:var(--error)">Failed to load</div>';
        }
    },

    /* ================================================================
     * CLIP/SCROLL NESTING INFO
     * ================================================================ */
    _loadNodeClipInfo: async function(nodeId) {
        var section = document.getElementById('node-clip-section');
        if (!section) return;
        try {
            var res = await this.api.post({ op: 'get_display_list' });
            var data = (res.data && res.data.value) ? res.data.value : (res.data || {});
            var items = data.items || [];
            var analysis = data.clip_analysis || {};

            var html = '<div class="detail-section-header">Clip / Scroll Nesting</div>';

            // Find this node's clip/scroll depths
            var nodeItem = items.find(function(it) { return it.node_index === nodeId || it.index === nodeId; });
            if (nodeItem) {
                html += '<div class="detail-row"><span class="detail-key">clip_depth</span><span class="detail-value">' + (nodeItem.clip_depth || 0) + '</span></div>';
                html += '<div class="detail-row"><span class="detail-key">scroll_depth</span><span class="detail-value">' + (nodeItem.scroll_depth || 0) + '</span></div>';
                if (nodeItem.content_size) html += '<div class="detail-row"><span class="detail-key">content_size</span><span class="detail-value">' + round(nodeItem.content_size.width || 0) + ' × ' + round(nodeItem.content_size.height || 0) + '</span></div>';
                if (nodeItem.scroll_id) html += '<div class="detail-row"><span class="detail-key">scroll_id</span><span class="detail-value">' + nodeItem.scroll_id + '</span></div>';
            } else {
                html += '<div class="placeholder-text" style="padding:4px 0">No display list entry for this node.</div>';
            }

            // Show analysis summary
            if (analysis.balanced !== undefined) {
                html += '<div class="detail-row" style="margin-top:6px"><span class="detail-key">balanced</span><span class="detail-value" style="color:' + (analysis.balanced ? 'var(--success)' : 'var(--error)') + '">' + analysis.balanced + '</span></div>';
                html += '<div class="detail-row"><span class="detail-key">final_clip_depth</span><span class="detail-value">' + (analysis.final_clip_depth || 0) + '</span></div>';
                html += '<div class="detail-row"><span class="detail-key">final_scroll_depth</span><span class="detail-value">' + (analysis.final_scroll_depth || 0) + '</span></div>';
            }

            section.innerHTML = html;
        } catch(e) {
            section.innerHTML = '<div class="detail-section-header">Clip / Scroll Nesting</div><div class="placeholder-text" style="padding:4px 0">Not available.</div>';
        }
    },

    /* ================================================================
     * AUTO-RESOLVE FUNCTION POINTERS
     * ================================================================ */
    _autoResolvePointers: async function(node) {
        if (!node || !node.events || !node.events.length) return;
        // Collect addresses that need resolving (not already cached)
        var toResolve = [];
        node.events.forEach(function(ev) {
            if (!app.state.resolvedSymbols[ev.callback_ptr]) {
                toResolve.push(ev.callback_ptr);
            }
        });
        if (toResolve.length > 0) {
            try {
                var res = await app.api.post({ op: 'resolve_function_pointers', addresses: toResolve });
                var resolved = (res.data && res.data.value) ? res.data.value : res.data;
                if (resolved && resolved.resolved) {
                    resolved.resolved.forEach(function(info) {
                        if (info.symbol_name) {
                            app.state.resolvedSymbols[info.address] = info;
                        }
                    });
                }
            } catch(e) {
                // Silently fail — user can still click to resolve manually
            }
        }
        // Update the displayed pointers in the DOM
        var ptrs = document.querySelectorAll('.event-ptr[data-addr]');
        ptrs.forEach(function(el) {
            var addr = el.dataset.addr;
            var cached = app.state.resolvedSymbols[addr];
            if (cached) {
                app.handlers._displayResolvedSymbol(el, addr, cached);
            }
        });
    },

    /* ================================================================
     * SNAPSHOT MANAGEMENT
     * ================================================================ */
    _saveSnapshot: function(alias) {
        if (!alias || !app.state.appStateJson) return;
        app.state.snapshots[alias] = JSON.parse(JSON.stringify(app.state.appStateJson));
        app.handlers.save();
        app.ui.renderSnapshots();
        app.log('Saved snapshot: ' + alias, 'info');
    },

    _restoreSnapshot: async function(alias) {
        var snapshot = app.state.snapshots[alias];
        if (!snapshot) {
            app.log('Snapshot not found: ' + alias, 'error');
            return;
        }
        try {
            var res = await app.api.post({ op: 'set_app_state', state: snapshot });
            if (res.status === 'ok') {
                app.log('Restored snapshot: ' + alias, 'info');
                app.handlers.loadAppState();
                app.handlers.refreshSidebar();
            } else {
                app.log('Failed to restore snapshot: ' + (res.message || ''), 'error');
            }
        } catch(e) {
            app.log('Restore failed: ' + e.message, 'error');
        }
    },

    _deleteSnapshot: function(alias) {
        delete app.state.snapshots[alias];
        app.handlers.save();
        app.ui.renderSnapshots();
        app.log('Deleted snapshot: ' + alias, 'info');
    },

    _renameSnapshot: function(oldAlias, newAlias) {
        if (!newAlias || newAlias === oldAlias) return;
        if (app.state.snapshots[newAlias]) {
            app.log('Snapshot name already exists: ' + newAlias, 'error');
            return;
        }
        app.state.snapshots[newAlias] = app.state.snapshots[oldAlias];
        delete app.state.snapshots[oldAlias];
        app.handlers.save();
        app.ui.renderSnapshots();
    },

    /* ================================================================
     * HANDLERS
     * ================================================================ */
    handlers: {
        /* ── Import / Export ── */
        importProject: function() { document.getElementById('file-import-project').click(); },
        importE2eTests: function() { document.getElementById('file-import-e2e').click(); },

        handleProjectImport: function(input) {
            var file = input.files[0];
            if (!file) return;
            var reader = new FileReader();
            reader.onload = function(e) {
                try {
                    var project = JSON.parse(e.target.result);
                    if (!confirm('Replace current project with imported data?')) return;
                    if (project.tests) app.state.tests = project.tests;
                    if (project.cssOverrides) app.state.cssOverrides = project.cssOverrides;
                    if (project.snapshots) app.state.snapshots = project.snapshots;
                    if (project.resolvedSymbols) app.state.resolvedSymbols = project.resolvedSymbols;
                    app.handlers.save();
                    app.ui.renderTestList();
                    app.ui.renderSnapshots();
                    app.log('Imported project from ' + file.name, 'info');
                } catch(err) { alert('Invalid project JSON: ' + err.message); }
            };
            reader.readAsText(file);
            input.value = '';
        },

        handleE2eImport: function(input) {
            var file = input.files[0];
            if (!file) return;
            var reader = new FileReader();
            reader.onload = function(e) {
                try {
                    var imported = JSON.parse(e.target.result);
                    var arr = Array.isArray(imported) ? imported : [imported];
                    arr.forEach(function(t) {
                        app.state.tests.push({
                            id: Date.now() + Math.random(),
                            name: t.name || 'Imported Test',
                            steps: (t.steps || []).map(function(s) {
                                var op = s.op;
                                var params = Object.assign({}, s);
                                delete params.op;
                                return { op: op, params: params, breakpoint: false };
                            })
                        });
                    });
                    app.handlers.save();
                    app.ui.renderTestList();
                    app.log('Appended ' + arr.length + ' E2E test(s) from ' + file.name, 'info');
                } catch(err) { alert('Invalid test JSON: ' + err.message); }
            };
            reader.readAsText(file);
            input.value = '';
        },

        exportProject: function() {
            var cleanTests = app.state.tests.map(function(t) {
                return {
                    id: t.id, name: t.name,
                    steps: t.steps.map(function(s) {
                        return { op: s.op, params: s.params || {}, breakpoint: s.breakpoint || false };
                    }),
                };
            });
            var exportData = {
                version: 2, exported_at: new Date().toISOString(),
                tests: cleanTests,
                cssOverrides: app.state.cssOverrides,
                snapshots: app.state.snapshots,
                resolvedSymbols: app.state.resolvedSymbols,
                apiUrl: app.config.apiUrl,
            };
            // Include live metadata if available
            if (app.state.hierarchy) {
                exportData.htmlTree = { nodes: app.state.hierarchy, root: app.state.hierarchyRoot };
            }
            if (app.state.componentData) {
                exportData.componentRegistry = app.state.componentData;
            }
            _downloadJSON(exportData, 'azul-debugger-project.json');
            app.log('Exported project', 'info');
        },

        exportE2eTests: function() {
            var exported = app.state.tests.map(function(t) {
                return { name: t.name, steps: t.steps.map(function(s) { return Object.assign({ op: s.op }, s.params || {}); }) };
            });
            _downloadJSON(exported, 'azul_e2e_tests.json');
            app.log('Exported ' + exported.length + ' E2E test(s)', 'info');
        },

        /* ── Component Library Import / Export ── */
        importComponentLibrary: function() {
            document.getElementById('file-import-component-lib').click();
        },

        handleComponentLibraryImport: async function(input) {
            var file = input.files[0];
            if (!file) return;
            var reader = new FileReader();
            reader.onload = async function(e) {
                try {
                    var libJson = JSON.parse(e.target.result);
                    // Validate basic structure
                    if (!libJson.name || !Array.isArray(libJson.components)) {
                        alert('Invalid component library JSON: must have "name" and "components" array.');
                        return;
                    }
                    var res = await app.api.post({ op: 'import_component_library', library: libJson });
                    if (res.status === 'ok') {
                        var d = (res.data && res.data.value) ? res.data.value : (res.data || {});
                        var action = d.was_update ? 'Updated' : 'Imported';
                        app.log(action + ' library "' + d.library_name + '" with ' + d.component_count + ' component(s)', 'info');
                        // Refresh library list
                        await app.handlers.loadLibraries();
                        // Auto-select the imported library
                        app.handlers.selectLibrary(libJson.name);
                    } else {
                        app.log('Import failed: ' + (res.message || JSON.stringify(res)), 'error');
                    }
                } catch(err) {
                    alert('Invalid component library JSON: ' + err.message);
                }
            };
            reader.readAsText(file);
            input.value = '';
        },

        exportComponentLibrary: async function() {
            var libName = app.state.selectedLibrary;
            if (!libName) {
                app.log('No library selected. Select a library first in the Components panel.', 'warn');
                return;
            }
            try {
                var res = await app.api.post({ op: 'export_component_library', library: libName });
                if (res.status === 'ok') {
                    var data = (res.data && res.data.value) ? res.data.value : (res.data || {});
                    _downloadJSON(data, libName + '_components.json');
                    app.log('Exported library "' + libName + '"', 'info');
                } else {
                    app.log('Export failed: ' + (res.message || 'Library may not be exportable (builtin libraries cannot be exported)'), 'error');
                }
            } catch(e) {
                app.log('Export component library failed: ' + e.message, 'error');
            }
        },

        exportCode: async function(language) {
            try {
                // Try ZIP endpoint first for a proper downloadable archive
                var payload = { op: 'export_code_zip', language: language };
                // If a user library is selected, include it
                if (app.state.selectedLibrary) payload.library = app.state.selectedLibrary;

                var res;
                try {
                    // Attempt binary fetch for ZIP
                    var response = await fetch(app.config.apiUrl, {
                        method: 'POST',
                        body: JSON.stringify(payload),
                    });
                    var ct = response.headers.get('content-type') || '';
                    if (ct.indexOf('application/zip') !== -1 || ct.indexOf('application/octet-stream') !== -1) {
                        // Binary ZIP response — download directly
                        var blob = await response.blob();
                        var url = URL.createObjectURL(blob);
                        var a = document.createElement('a');
                        a.href = url;
                        a.download = 'azul_export_' + language + '.zip';
                        a.click();
                        URL.revokeObjectURL(url);
                        app.log('Exported code (' + language + ') as ZIP', 'info');
                        return;
                    }
                    // JSON response — fall through
                    try { res = await response.json(); } catch(jsonErr) {
                        var txt = await response.text();
                        res = JSON.parse(txt);
                    }
                } catch(zipErr) {
                    // ZIP endpoint not available, fall back to export_code
                    console.log('[dbg] export_code_zip failed, falling back to export_code:', zipErr.message);
                    res = null;
                }

                // Fallback: use plain export_code endpoint
                if (!res || res.status !== 'ok') {
                    res = await app.api.post({ op: 'export_code', language: language });
                }

                if (res.status === 'ok') {
                    var data = (res.data && res.data.value) ? res.data.value : (res.data || {});
                    var files = data.files || {};
                    var warnings = data.warnings || [];

                    // Log warnings
                    warnings.forEach(function(w) { app.log('Export warning: ' + w, 'warn'); });

                    // Download each file
                    var fileNames = Object.keys(files);
                    if (fileNames.length === 0) {
                        app.log('No files generated for ' + language, 'warn');
                        return;
                    }

                    if (fileNames.length === 1) {
                        // Single file: download directly as text
                        var fname = fileNames[0];
                        var blob = new Blob([files[fname]], { type: 'text/plain' });
                        var url = URL.createObjectURL(blob);
                        var a = document.createElement('a');
                        a.href = url; a.download = fname; a.click();
                        URL.revokeObjectURL(url);
                    } else {
                        // Multiple files: download as JSON bundle (no JSZip dependency needed)
                        _downloadJSON(files, 'azul_export_' + language + '.json');
                    }
                    app.log('Exported code (' + language + '): ' + fileNames.join(', '), 'info');
                } else {
                    app.log('Code export failed: ' + (res.message || JSON.stringify(res)), 'error');
                }
            } catch(e) {
                app.log('Code export failed: ' + e.message, 'error');
            }
        },

        /* ── Test management ── */
        newTest: function() {
            var t = { id: Date.now(), name: 'Test ' + (app.state.tests.length + 1), steps: [{ op: 'get_state', params: {}, breakpoint: false }] };
            app.state.tests.push(t);
            this.save();
            this.selectTest(t.id);
            app.ui.renderTestList();
            app.ui.switchView('testing');
        },

        selectTest: function(id) {
            app.state.activeTestId = id;
            app.state.currentStepIndex = -1;
            app.state.executionStatus = 'idle';
            app.ui.renderTestList();
            app.ui.renderSteps();
            // Update tab title to show the selected test name
            var test = app.state.tests.find(function(t) { return t.id === id; });
            if (test) document.getElementById('tab-title').innerText = test.name;
        },

        addStepFromForm: function() {
            if (!app.state.activeTestId) return;
            var op = document.getElementById('new-step-op').value;
            var inputs = document.querySelectorAll('.step-param-input');
            var params = {};
            inputs.forEach(function(inp) {
                if (inp.value !== '') params[inp.dataset.name] = inp.type === 'number' ? parseFloat(inp.value) : inp.value;
            });
            var test = app.state.tests.find(function(t) { return t.id === app.state.activeTestId; });
            test.steps.push({ op: op, params: params, breakpoint: false });
            this.save();
            app.ui.renderSteps();
        },

        deleteStep: function(idx, e) {
            e.stopPropagation();
            var test = app.state.tests.find(function(t) { return t.id === app.state.activeTestId; });
            test.steps.splice(idx, 1);
            this.save();
            app.ui.renderSteps();
        },

        toggleBreakpoint: function(idx, e) {
            e.stopPropagation();
            var test = app.state.tests.find(function(t) { return t.id === app.state.activeTestId; });
            test.steps[idx].breakpoint = !test.steps[idx].breakpoint;
            this.save();
            app.ui.renderSteps();
        },

        /* ── DOM tree ── */
        refreshSidebar: async function() {
            try {
                var res = await app.api.post({ op: 'get_node_hierarchy' });
                var data = null;
                if (res.data) data = res.data.value || res.data;
                if (data && data.nodes) {
                    app.ui.renderDomTree(data.nodes, data.root != null ? data.root : 0);
                } else {
                    document.getElementById('dom-tree-container').innerHTML = '<div class="placeholder-text">No DOM data received.</div>';
                }
            } catch(e) {
                document.getElementById('dom-tree-container').innerHTML = '<div class="placeholder-text" style="color:var(--error)">Failed to load DOM tree.</div>';
            }
            // Load component palette for inspector
            app.handlers._loadPaletteComponents();
        },

        _loadPaletteComponents: async function() {
            var container = document.getElementById('palette-component-list');
            if (!container) return;
            try {
                var res = await app.api.post({ op: 'get_component_registry' });
                var reg = null;
                if (res.data) reg = res.data.value || res.data;
                if (!reg || !reg.libraries) {
                    container.innerHTML = '<div class="placeholder-text" style="font-size:11px">No components.</div>';
                    return;
                }
                var html = '';
                reg.libraries.forEach(function(lib) {
                    if (!lib.components || !lib.components.length) return;
                    html += '<div style="font-size:10px;color:var(--text-muted);margin:4px 0 2px;text-transform:uppercase">' + esc(lib.name) + '</div>';
                    lib.components.forEach(function(comp) {
                        html += '<div class="palette-item" draggable="true" '
                            + 'data-library="' + esc(lib.name) + '" '
                            + 'data-component="' + esc(comp.tag || comp.name) + '" '
                            + 'title="' + esc(comp.description || comp.display_name || comp.tag) + '">';
                        html += '<span class="material-icons" style="font-size:12px;margin-right:4px">widgets</span>';
                        html += '<span>' + esc(comp.display_name || comp.tag || comp.name) + '</span>';
                        html += '</div>';
                    });
                });
                container.innerHTML = html || '<div class="placeholder-text" style="font-size:11px">No components.</div>';
                // Setup drag events
                container.querySelectorAll('.palette-item').forEach(function(item) {
                    item.addEventListener('dragstart', function(e) {
                        e.dataTransfer.setData('text/plain', JSON.stringify({
                            library: item.dataset.library,
                            component: item.dataset.component
                        }));
                        e.dataTransfer.effectAllowed = 'copy';
                    });
                });
            } catch(e) {
                container.innerHTML = '<div class="placeholder-text" style="font-size:11px">Failed to load.</div>';
            }
        },

        nodeSelected: async function(nodeId) {
            app.state.selectedNodeId = nodeId;
            if (app.state.hierarchy) {
                app.ui.renderDomTree(app.state.hierarchy, app.state.hierarchyRoot);
            }
            var node = app.state.hierarchy ? app.state.hierarchy.find(function(n) { return n.index === nodeId; }) : null;
            app.ui.renderNodeDetail(node);
            // Auto-resolve function pointers for this node
            if (node && node.events && node.events.length) {
                app._autoResolvePointers(node);
            }
            // Load dataset if node has one
            if (node && node.has_dataset) {
                app.handlers.loadNodeDataset(nodeId);
            } else {
                app.state.datasetJson = null;
                app.state.datasetNodeId = null;
                app.handlers._updateDatasetPanel();
            }
        },

        /* ── CSS editing (inline edit + add override) ── */
        editCssProp: function(nodeId, propName, el) {
            var currentValue = el.textContent;
            var input = document.createElement('input');
            input.type = 'text';
            input.value = currentValue;
            input.className = 'form-control';
            input.style.cssText = 'width:100%;font-size:12px;padding:2px 4px;font-family:monospace;';
            el.innerHTML = '';
            el.appendChild(input);
            input.focus();
            input.select();

            var commit = function() {
                var newVal = input.value.trim();
                if (newVal !== currentValue) {
                    // Store locally
                    if (!app.state.cssOverrides[nodeId]) app.state.cssOverrides[nodeId] = {};
                    app.state.cssOverrides[nodeId][propName] = newVal;
                    app.handlers.save();
                    el.classList.add('overridden');
                    // Send to server
                    app.api.post({ op: 'set_node_css_override', node_id: nodeId, property: propName, value: newVal }).then(function(res) {
                        app.log('CSS override: ' + propName + ' = ' + newVal + ' → ' + res.status, res.status === 'ok' ? 'info' : 'error');
                    }).catch(function(e) {
                        app.log('CSS override failed: ' + e.message, 'error');
                    });
                }
                el.textContent = newVal || currentValue;
            };
            input.addEventListener('blur', commit);
            input.addEventListener('keydown', function(e) {
                if (e.key === 'Enter') { commit(); input.blur(); }
                if (e.key === 'Escape') { el.textContent = currentValue; }
            });
        },

        addCssOverride: function(nodeId) {
            var nameEl = document.getElementById('css-add-name');
            var valueEl = document.getElementById('css-add-value');
            if (!nameEl || !valueEl) return;
            var prop = nameEl.value.trim();
            var val = valueEl.value.trim();
            if (!prop || !val) return;

            if (!app.state.cssOverrides[nodeId]) app.state.cssOverrides[nodeId] = {};
            app.state.cssOverrides[nodeId][prop] = val;
            app.handlers.save();

            app.api.post({ op: 'set_node_css_override', node_id: nodeId, property: prop, value: val }).then(function(res) {
                app.log('CSS override: ' + prop + ' = ' + val + ' → ' + res.status, res.status === 'ok' ? 'info' : 'error');
                app._loadNodeCssProperties(nodeId); // refresh
            }).catch(function(e) {
                app.log('CSS override failed: ' + e.message, 'error');
            });
        },

        /* ── Resolve function pointer ── */
        resolvePtr: async function(el) {
            var addr = el.dataset.addr || el.textContent;
            // Check cache first
            if (app.state.resolvedSymbols[addr]) {
                app.handlers._displayResolvedSymbol(el, addr, app.state.resolvedSymbols[addr]);
                return;
            }
            try {
                var res = await app.api.post({ op: 'resolve_function_pointers', addresses: [addr] });
                var resolved = (res.data && res.data.value) ? res.data.value : res.data;
                var info = (resolved && resolved.resolved && resolved.resolved.length) ? resolved.resolved[0] : null;
                if (info && info.symbol_name) {
                    // Cache the result
                    app.state.resolvedSymbols[addr] = info;
                    app.handlers._displayResolvedSymbol(el, addr, info);
                    app.log('Resolved ' + addr + ' → ' + info.symbol_name, 'info');
                } else {
                    el.textContent = addr + ' (unresolved)';
                    el.onclick = null;
                    app.log('Could not resolve ' + addr, 'warning');
                }
            } catch(e) {
                app.log('Resolve failed: ' + e.message, 'error');
            }
        },

        _displayResolvedSymbol: function(el, addr, info) {
            var displayName = info.symbol_name;
            var html = '<span class="resolved-symbol">' + esc(displayName) + '</span>';
            if (info.source_file) {
                var shortFile = info.source_file.split('/').pop();
                var lineStr = info.source_line ? ':' + info.source_line : '';
                html += ' <a class="source-link" href="#" onclick="app.handlers.openSourceFile(\'' + esc(info.source_file) + '\', ' + (info.source_line || 0) + '); return false;" title="' + esc(info.source_file + lineStr) + '">' + esc(shortFile + lineStr) + '</a>';
                if (info.approximate) {
                    html += ' <span style="color:var(--warning);font-size:9px" title="Location found by heuristic search">≈</span>';
                }
            } else if (info.file_name) {
                var shortLib = info.file_name.split('/').pop();
                html += ' <span class="source-lib">(' + esc(shortLib) + ')</span>';
            }
            if (info.hint) {
                html += ' <span style="color:var(--warning);font-size:9px" title="' + esc(info.hint) + '">⚠</span>';
            }
            el.innerHTML = html;
            el.onclick = null;
            el.title = JSON.stringify(info, null, 2);
        },

        openSourceFile: async function(file, line) {
            try {
                await app.api.post({ op: 'open_file', file: file, line: line || 0 });
                app.log('Opening ' + file + (line ? ':' + line : ''), 'info');
            } catch(e) {
                // Fallback: try vscode:// URL
                var url = 'vscode://file/' + file + (line ? ':' + line : '');
                window.open(url, '_blank');
                app.log('Opening via vscode:// URL', 'info');
            }
        },

        /* ── Context menu actions ── */
        ctxInsertChild: async function(tag) {
            var nodeId = app.state.contextMenuNodeId;
            if (nodeId == null) return;
            try {
                var nodeType = tag === 'text' ? 'text:New text' : tag;
                var res = await app.api.post({ op: 'insert_node', parent_id: nodeId, node_type: nodeType });
                app.log('Inserted ' + tag + ' into node #' + nodeId + ': ' + res.status, res.status === 'ok' ? 'info' : 'error');
                this.refreshSidebar();
            } catch(e) {
                app.log('Insert failed: ' + e.message, 'error');
            }
        },

        ctxDeleteNode: async function() {
            var nodeId = app.state.contextMenuNodeId;
            if (nodeId == null) return;
            try {
                var res = await app.api.post({ op: 'delete_node', node_id: nodeId });
                app.log('Deleted node #' + nodeId + ': ' + res.status, res.status === 'ok' ? 'info' : 'error');
                this.refreshSidebar();
            } catch(e) {
                app.log('Delete failed: ' + e.message, 'error');
            }
        },

        // H6: Create a new component from the selected DOM subtree
        ctxCreateComponentFromSubtree: async function() {
            var nodeId = app.state.contextMenuNodeId;
            if (nodeId == null) return;

            // Find the node in hierarchy
            var node = null;
            if (app.state.hierarchy) {
                var byIndex = {};
                app.state.hierarchy.forEach(function(n) { byIndex[n.index] = n; });
                node = byIndex[nodeId];
            }
            if (!node) {
                app.log('Cannot find node #' + nodeId + ' in hierarchy', 'error');
                return;
            }

            // Pick target library (only modifiable ones)
            var modLibs = (app.state.libraryList || []).filter(function(l) { return l.modifiable !== false; });
            var targetLibrary = 'user';
            if (modLibs.length === 0) {
                // No mutable libraries — create one called "user"
                try { await app.api.post({ op: 'create_library', name: 'user' }); } catch(e2) {}
                targetLibrary = 'user';
            } else if (modLibs.length === 1) {
                targetLibrary = modLibs[0].name;
            } else {
                // Multiple modifiable libs — let user choose
                var libChoices = modLibs.map(function(l) { return l.name; }).join(', ');
                var chosen = prompt('Which library? (' + libChoices + '):', modLibs[0].name);
                if (!chosen || !chosen.trim()) return;
                chosen = chosen.trim();
                if (!modLibs.some(function(l) { return l.name === chosen; })) {
                    app.log('Unknown library "' + chosen + '". Available: ' + libChoices, 'error');
                    return;
                }
                targetLibrary = chosen;
            }

            // Prompt for component name
            var tagName = prompt('Component tag name (lowercase, e.g. "my-card"):');
            if (!tagName || !tagName.trim()) return;
            tagName = tagName.trim().toLowerCase().replace(/[^a-z0-9_-]/g, '-');

            // Build a simplified render tree from the subtree
            function extractSubtree(n, byIdx) {
                var result = {
                    tag: (n.tag || n.type || 'div').toLowerCase(),
                    classes: n.classes || [],
                    children: []
                };
                if (n.type === 'Text') {
                    result.tag = '__text__';
                    result.text = n.text || '';
                }
                if (n.children && n.children.length) {
                    n.children.forEach(function(cIdx) {
                        if (byIdx[cIdx]) {
                            result.children.push(extractSubtree(byIdx[cIdx], byIdx));
                        }
                    });
                }
                return result;
            }

            var byIndex2 = {};
            app.state.hierarchy.forEach(function(n) { byIndex2[n.index] = n; });
            var subtree = extractSubtree(node, byIndex2);

            try {
                var res = await app.api.post({
                    op: 'create_component',
                    library: targetLibrary,
                    name: tagName,
                    display_name: tagName.replace(/-/g, ' ').replace(/\b\w/g, function(l) { return l.toUpperCase(); }),
                    description: 'Created from DOM subtree',
                    render_tree: subtree,
                });
                if (res.status === 'ok') {
                    app.log('Component "' + tagName + '" created in library "' + targetLibrary + '"', 'info');
                    // Switch to components view and select it
                    app.state.currentView = 'components';
                    app.ui.showView('components');
                    app.handlers.selectLibrary(targetLibrary);
                } else {
                    app.log('Failed to create component: ' + (res.message || ''), 'error');
                }
            } catch(e) {
                app.log('Create component failed: ' + e.message, 'error');
            }
        },

        /* ── App State (JSON tree in left panel) ── */
        loadAppState: async function() {
            try {
                var res = await app.api.post({ op: 'get_app_state' });
                var data = null;
                if (res.data) data = res.data.value || res.data;
                app.state.appStateJson = data;
                app.json.render('app-state-tree', data, false);
                app.ui.renderSnapshots();
            } catch(e) {
                document.getElementById('app-state-tree').innerHTML = '<div class="placeholder-text" style="color:var(--error)">Failed to load app state.</div>';
            }
        },

        /* Auto-save app state (called when user edits a value) */
        _autoSaveAppState: async function() {
            if (app.state.appStateJson == null) return;
            try {
                var res = await app.api.post({ op: 'set_app_state', state: app.state.appStateJson });
                if (res.status === 'ok') {
                    app.log('App state saved', 'info');
                    // Refresh DOM + app state after state change
                    setTimeout(function() {
                        app.handlers.refreshSidebar();
                        app.handlers.loadAppState();
                        // Re-select current node if any
                        if (app.state.selectedNodeId != null) {
                            setTimeout(function() { app.handlers.nodeSelected(app.state.selectedNodeId); }, 200);
                        }
                    }, 100);
                } else {
                    app.log('App state save failed: ' + (res.message || JSON.stringify(res.data)), 'error');
                }
            } catch(e) {
                app.log('Save app state failed: ' + e.message, 'error');
            }
        },

        /* ── Window State (editable JSON tree in left panel bottom) ── */
        loadWindowState: async function() {
            try {
                var res = await app.api.post({ op: 'get_state' });
                var data = null;
                if (res.window_state) {
                    data = res.window_state;
                } else if (res.data) {
                    data = res.data.value || res.data;
                }
                app.state.windowStateJson = data;
                app.json.render('window-state-tree', data, true);
            } catch(e) {
                document.getElementById('window-state-tree').innerHTML = '<div class="placeholder-text" style="color:var(--error)">Failed to load window state.</div>';
            }
        },

        /* ── Node Dataset (read-only JSON tree for node's RefAny dataset) ── */
        loadNodeDataset: async function(nodeId) {
            var nid = (nodeId != null) ? nodeId : app.state.selectedNodeId;
            if (nid == null) {
                app.state.datasetJson = null;
                app.state.datasetNodeId = null;
                app.handlers._updateDatasetPanel();
                return;
            }
            try {
                var res = await app.api.post({ op: 'get_node_dataset', node_id: nid });
                if (res.status === 'ok' && res.data && res.data.value) {
                    var ds = res.data.value;
                    app.state.datasetJson = ds.dataset || null;
                    app.state.datasetNodeId = nid;
                } else {
                    app.state.datasetJson = null;
                    app.state.datasetNodeId = nid;
                }
            } catch(e) {
                app.state.datasetJson = null;
                app.state.datasetNodeId = nid;
            }
            app.handlers._updateDatasetPanel();
        },

        _updateDatasetPanel: function() {
            var panel = document.getElementById('dataset-panel');
            if (!panel) return;
            var treeEl = document.getElementById('dataset-tree');
            if (app.state.datasetJson != null) {
                app.json.render('dataset-tree', app.state.datasetJson, true);
            } else if (app.state.selectedNodeId != null) {
                treeEl.innerHTML = '<div class="placeholder-text">Selected node has no dataset.</div>';
            } else {
                treeEl.innerHTML = '<div class="placeholder-text">Select a node to inspect its dataset.</div>';
            }
        },

        /* ── Component Libraries ── */
        loadLibraries: async function() {
            try {
                var res = await app.api.post({ op: 'get_libraries' });
                var data = (res.data && res.data.value) ? res.data.value : (res.data || {});
                var libraries = data.libraries || [];
                app.state.libraryList = libraries;
                var selector = document.getElementById('library-selector');
                if (!selector) return;
                var html = '';
                if (!libraries.length) {
                    html = '<option value="">No libraries</option>';
                } else {
                    libraries.forEach(function(lib) {
                        var selected = app.state.selectedLibrary === lib.name ? ' selected' : '';
                        html += '<option value="' + esc(lib.name) + '"' + selected + '>'
                            + esc(lib.name) + ' (' + lib.component_count + ')</option>';
                    });
                }
                selector.innerHTML = html;
                // Update button visibility based on modifiable flag
                app.handlers._updateLibraryButtons();
                // Auto-select first library if none selected
                if (!app.state.selectedLibrary && libraries.length > 0) {
                    app.handlers.onLibraryChange(libraries[0].name);
                } else if (app.state.selectedLibrary) {
                    app.handlers.onLibraryChange(app.state.selectedLibrary);
                }
            } catch(e) {
                var selector = document.getElementById('library-selector');
                if (selector) selector.innerHTML = '<option value="">Error loading</option>';
            }
        },

        onLibraryChange: function(libName) {
            app.state.selectedLibrary = libName;
            app.state.componentFilter = '';
            var filterInput = document.getElementById('component-filter');
            if (filterInput) filterInput.value = '';
            app.handlers._updateLibraryButtons();
            app.handlers.selectLibrary(libName);
        },

        _updateLibraryButtons: function() {
            var btns = document.getElementById('library-buttons');
            if (!btns) return;
            var lib = (app.state.libraryList || []).find(function(l) { return l.name === app.state.selectedLibrary; });
            var modifiable = lib ? lib.modifiable : false;
            btns.style.display = modifiable ? 'flex' : 'none';
        },

        selectLibrary: async function(libName) {
            app.state.selectedLibrary = libName;
            try {
                var res = await app.api.post({ op: 'get_library_components', library: libName });
                var data = (res.data && res.data.value) ? res.data.value : (res.data || {});
                var components = data.components || [];
                app.state.componentData = { library: libName, components: components };
                app.handlers._renderComponentList(components);
            } catch(e) {
                document.getElementById('component-list-container').innerHTML = '<div class="placeholder-text" style="color:var(--error)">Failed to load components.</div>';
            }
        },

        filterComponents: function(query) {
            app.state.componentFilter = (query || '').toLowerCase();
            var components = (app.state.componentData && app.state.componentData.components) || [];
            app.handlers._renderComponentList(components);
        },

        _renderComponentList: function(components) {
            var container = document.getElementById('component-list-container');
            if (!container) return;
            var filter = (app.state.componentFilter || '').toLowerCase();
            var filtered = components;
            if (filter) {
                filtered = components.filter(function(c) {
                    var name = (c.display_name || c.tag || '').toLowerCase();
                    var tag = (c.tag || '').toLowerCase();
                    return name.indexOf(filter) !== -1 || tag.indexOf(filter) !== -1;
                });
            }
            if (!filtered.length) {
                container.innerHTML = '<div class="placeholder-text">' + (filter ? 'No matching components.' : 'No components in this library.') + '</div>';
                return;
            }
            var html = '';
            filtered.forEach(function(c) {
                // Find original index for showComponentDetail
                var origIdx = components.indexOf(c);
                var lib = app.state.selectedLibrary || '';
                html += '<div class="list-item" draggable="true" onclick="app.handlers.showComponentDetail(' + origIdx + ')"';
                html += ' ondragstart="event.dataTransfer.setData(\'text/plain\', JSON.stringify({type:\'component\',library:\'' + esc(lib) + '\',component:\'' + esc(c.tag || '') + '\'}));event.dataTransfer.effectAllowed=\'copy\'"';
                html += '>';
                html += '<span style="font-weight:500">' + esc(c.display_name || c.tag) + '</span>';
                html += '</div>';
            });
            container.innerHTML = html;
        },

        createLibrary: async function() {
            var name = prompt('New library name:');
            if (!name || !name.trim()) return;
            try {
                var res = await app.api.post({ op: 'create_library', name: name.trim() });
                if (res.status === 'ok') {
                    app.log('Created library: ' + name.trim(), 'info');
                    app.state.selectedLibrary = name.trim();
                    app.handlers.loadLibraries();
                } else {
                    app.log('Failed to create library: ' + (res.message || ''), 'error');
                }
            } catch(e) {
                app.log('Create library failed: ' + e.message, 'error');
            }
        },

        createComponent: async function() {
            if (!app.state.selectedLibrary) { app.log('No library selected', 'error'); return; }
            var name = prompt('New component tag name:');
            if (!name || !name.trim()) return;
            var displayName = prompt('Display name (optional):', name.trim());
            try {
                var payload = { op: 'create_component', library: app.state.selectedLibrary, name: name.trim() };
                if (displayName && displayName.trim()) payload.display_name = displayName.trim();
                var res = await app.api.post(payload);
                if (res.status === 'ok') {
                    app.log('Created component: ' + name.trim(), 'info');
                    app.handlers.selectLibrary(app.state.selectedLibrary);
                } else {
                    app.log('Failed to create component: ' + (res.message || ''), 'error');
                }
            } catch(e) {
                app.log('Create component failed: ' + e.message, 'error');
            }
        },

        showComponentDetail: function(idx) {
            var components = (app.state.componentData && app.state.componentData.components) || [];
            var component = components[idx];
            if (!component) return;
            app.state.selectedComponentIdx = idx;
            var leftPanel = document.getElementById('component-detail-left');
            var rightPanel = document.getElementById('component-detail-right');
            if (!leftPanel) return;

            // Clear and rebuild via DOM (no innerHTML for interactive parts)
            leftPanel.innerHTML = '';

            // === Header ===
            var headerDiv = document.createElement('div');
            headerDiv.style.marginBottom = '12px';
            var h3 = document.createElement('h3');
            h3.style.margin = '0 0 4px 0';
            h3.textContent = component.display_name || component.tag;
            headerDiv.appendChild(h3);
            var qualSpan = document.createElement('span');
            qualSpan.style.cssText = 'color:var(--text-muted);font-size:12px';
            qualSpan.textContent = component.qualified_name;
            headerDiv.appendChild(qualSpan);
            if (component.description) {
                var descP = document.createElement('p');
                descP.style.cssText = 'margin:8px 0 0 0;color:var(--text-muted)';
                descP.textContent = component.description;
                headerDiv.appendChild(descP);
            }
            leftPanel.appendChild(headerDiv);

            // === Badges ===
            var badgeDiv = document.createElement('div');
            badgeDiv.style.cssText = 'display:flex;gap:6px;margin-bottom:12px';
            if (component.source) {
                var b = document.createElement('span');
                b.className = 'badge';
                b.textContent = component.source;
                badgeDiv.appendChild(b);
            }
            leftPanel.appendChild(badgeDiv);

            // === Data Model (widget-based) ===
            var dataModel = component.data_model || [];
            if (dataModel.length) {
                var dmDetails = document.createElement('details');
                dmDetails.open = true;
                var dmSummary = document.createElement('summary');
                dmSummary.style.cssText = 'font-weight:600;margin-bottom:6px';
                dmSummary.textContent = 'Data Model (' + dataModel.length + ')';
                dmDetails.appendChild(dmSummary);

                var dmEditor = app.widgets.DataModelEditor.render({
                    dataModel: {
                        name: (component.display_name || component.tag) + 'Data',
                        fields: dataModel
                    },
                    readOnly: component.source !== 'user_defined'
                }, {
                    fieldValues: {}
                }, {
                    onFieldChange: function(name, newVal) {
                        app.log('Field changed: ' + name + ' = ' + JSON.stringify(newVal), 'info');
                        // D4: Live preview update on value change
                        if (app.handlers._previewDebounce) clearTimeout(app.handlers._previewDebounce);
                        app.handlers._previewDebounce = setTimeout(function() {
                            var previewEl = document.getElementById('component-preview-widget');
                            app.handlers._loadComponentPreview(component, previewEl);
                            // Also refresh the mini tree
                            app.handlers._loadMiniTree(component);
                        }, 300);
                    }
                });
                dmDetails.appendChild(dmEditor);
                leftPanel.appendChild(dmDetails);
            }

            // === Callback slots (innerHTML — read-only table) ===
            if (component.callback_slots && component.callback_slots.length) {
                var cbDetails = document.createElement('details');
                cbDetails.open = true;
                var cbSummary = document.createElement('summary');
                cbSummary.style.cssText = 'font-weight:600;margin:12px 0 6px 0';
                cbSummary.textContent = 'Callbacks (' + component.callback_slots.length + ')';
                cbDetails.appendChild(cbSummary);
                var cbHtml = '<table class="detail-table"><tr><th>Slot</th><th>Type</th><th>Description</th></tr>';
                component.callback_slots.forEach(function(s) {
                    cbHtml += '<tr><td>' + esc(s.name) + '</td><td style="color:var(--accent);font-size:11px">' + esc(s.callback_type) + '</td><td>' + esc(s.description) + '</td></tr>';
                });
                cbHtml += '</table>';
                var cbTable = document.createElement('div');
                cbTable.innerHTML = cbHtml;
                cbDetails.appendChild(cbTable);
                leftPanel.appendChild(cbDetails);
            }

            // === Scoped CSS (widget-based) ===
            if (component.css || component.source === 'user_defined') {
                var cssDetails = document.createElement('details');
                if (component.css) cssDetails.open = true;
                var cssSummary = document.createElement('summary');
                cssSummary.style.cssText = 'font-weight:600;margin:12px 0 6px 0';
                cssSummary.textContent = 'Scoped CSS';
                cssDetails.appendChild(cssSummary);

                var isEditable = component.source === 'user_defined';
                var cssEditor = app.widgets.CssEditor.render({
                    readOnly: !isEditable
                }, {
                    css: component.css || ''
                }, {
                    onChange: function(newCss) {
                        // live update stored in memory
                        component.css = newCss;
                        // H2: Auto-preview on CSS change
                        if (app.handlers._cssPreviewDebounce) clearTimeout(app.handlers._cssPreviewDebounce);
                        app.handlers._cssPreviewDebounce = setTimeout(function() {
                            var previewEl = document.getElementById('component-preview-widget');
                            app.handlers._loadComponentPreview(component, previewEl);
                        }, 500);
                    },
                    onSave: isEditable ? function(cssText) {
                        app.handlers._saveComponentCss(cssText);
                    } : null
                });
                cssDetails.appendChild(cssEditor);
                leftPanel.appendChild(cssDetails);
            }

            // === Source Code Editors (E1, E2, E3) ===
            if (component.source === 'user_defined') {
                var srcDetails = document.createElement('details');
                var srcSummary = document.createElement('summary');
                srcSummary.style.cssText = 'font-weight:600;margin:12px 0 6px 0';
                srcSummary.textContent = 'Source Code';
                srcDetails.appendChild(srcSummary);

                // E1: Edit render_fn button
                var renderBtn = document.createElement('button');
                renderBtn.className = 'azd-btn-small';
                renderBtn.style.marginRight = '8px';
                renderBtn.textContent = 'Edit render_fn';
                renderBtn.addEventListener('click', function() {
                    app.handlers._openRenderFnEditor(component);
                });
                srcDetails.appendChild(renderBtn);

                // E2: Edit compile_fn dropdown
                var compileBtn = document.createElement('button');
                compileBtn.className = 'azd-btn-small';
                compileBtn.style.marginRight = '8px';
                compileBtn.textContent = 'Edit compile_fn \u25BE';
                compileBtn.addEventListener('click', function() {
                    var rect = compileBtn.getBoundingClientRect();
                    app.widgets.ContextMenu.show(rect.left, rect.bottom + 2, [
                        { label: 'Rust', action: function() { app.handlers._openCompileFnEditor(component, 'rust'); } },
                        { label: 'C', action: function() { app.handlers._openCompileFnEditor(component, 'c'); } },
                        { label: 'C++', action: function() { app.handlers._openCompileFnEditor(component, 'cpp'); } },
                        { label: 'Python', action: function() { app.handlers._openCompileFnEditor(component, 'python'); } },
                    ]);
                });
                srcDetails.appendChild(compileBtn);

                leftPanel.appendChild(srcDetails);
            }

            // === Universal HTML Attributes (innerHTML — read-only table, collapsed) ===
            var universalAttrs = component.universal_attributes || [];
            if (universalAttrs.length) {
                var uaDetails = document.createElement('details');
                var uaSummary = document.createElement('summary');
                uaSummary.style.cssText = 'font-weight:600;margin:12px 0 6px 0;color:var(--text-muted)';
                uaSummary.textContent = 'Universal HTML Attributes (' + universalAttrs.length + ')';
                uaDetails.appendChild(uaSummary);
                var uaHtml = '<table class="detail-table"><tr><th>Name</th><th>Type</th></tr>';
                universalAttrs.forEach(function(a) {
                    uaHtml += '<tr><td>' + esc(a.name) + '</td><td style="color:var(--accent)">' + esc(a.attr_type) + '</td></tr>';
                });
                uaHtml += '</table>';
                var uaTable = document.createElement('div');
                uaTable.innerHTML = uaHtml;
                uaDetails.appendChild(uaTable);
                leftPanel.appendChild(uaDetails);
            }

            // === Right column: Preview + Mini HTML Tree + Palette (widget-based) ===
            if (rightPanel) {
                rightPanel.style.display = 'block';
                rightPanel.innerHTML = '';

                // Preview section (widget-based)
                var previewH4 = document.createElement('h4');
                previewH4.style.margin = '12px 0 8px 0';
                previewH4.textContent = 'Preview';
                rightPanel.appendChild(previewH4);

                // H4: Preview environment selectors (OS / Theme / Language)
                var envRow = document.createElement('div');
                envRow.style.cssText = 'display:flex;gap:6px;margin-bottom:8px;align-items:center;flex-wrap:wrap';

                function makeEnvSelect(label, options, stateKey) {
                    var wrap = document.createElement('label');
                    wrap.style.cssText = 'font-size:11px;color:var(--text-muted);display:flex;align-items:center;gap:3px';
                    wrap.textContent = label;
                    var sel = document.createElement('select');
                    sel.style.cssText = 'font-size:11px;background:var(--bg-secondary);color:var(--text);border:1px solid var(--border);border-radius:3px;padding:1px 4px';
                    options.forEach(function(o) {
                        var opt = document.createElement('option');
                        opt.value = o.value;
                        opt.textContent = o.label;
                        if (app.state[stateKey] === o.value || (!app.state[stateKey] && o.value === '')) sel.selectedIndex = sel.options.length;
                        sel.appendChild(opt);
                    });
                    sel.addEventListener('change', function() {
                        app.state[stateKey] = sel.value || null;
                        var pe = document.getElementById('component-preview-widget');
                        app.handlers._loadComponentPreview(component, pe);
                    });
                    wrap.appendChild(sel);
                    return wrap;
                }

                envRow.appendChild(makeEnvSelect('OS:', [
                    { value: '', label: 'Native' },
                    { value: 'windows', label: 'Windows' },
                    { value: 'mac', label: 'macOS' },
                    { value: 'linux', label: 'Linux' }
                ], 'previewOs'));

                envRow.appendChild(makeEnvSelect('Theme:', [
                    { value: '', label: 'Native' },
                    { value: 'light', label: 'Light' },
                    { value: 'dark', label: 'Dark' }
                ], 'previewTheme'));

                envRow.appendChild(makeEnvSelect('Lang:', [
                    { value: '', label: 'Native' },
                    { value: 'en', label: 'English' },
                    { value: 'de', label: 'Deutsch' },
                    { value: 'fr', label: 'Français' },
                    { value: 'es', label: 'Español' },
                    { value: 'ja', label: '日本語' },
                    { value: 'zh', label: '中文' }
                ], 'previewLang'));

                rightPanel.appendChild(envRow);

                var previewEl = app.widgets.PreviewPanel.render({}, { loading: true }, {
                    onRefresh: function() {
                        app.handlers._loadComponentPreview(component, previewEl);
                    }
                });
                previewEl.id = 'component-preview-widget';
                rightPanel.appendChild(previewEl);

                // Load the actual preview image
                app.handlers._loadComponentPreview(component, previewEl);

                // === Mini HTML Tree (D1) ===
                var treeH4 = document.createElement('h4');
                treeH4.style.margin = '16px 0 8px 0';
                treeH4.textContent = 'Render Output';
                rightPanel.appendChild(treeH4);

                var treeContainer = document.createElement('div');
                treeContainer.id = 'component-mini-tree-container';
                rightPanel.appendChild(treeContainer);

                // Load render output tree
                app.handlers._loadMiniTree(component, treeContainer);

                // === Component Palette (D2) ===
                if (component.source === 'user_defined') {
                    var palH4 = document.createElement('h4');
                    palH4.style.margin = '16px 0 8px 0';
                    palH4.textContent = 'Component Palette';
                    rightPanel.appendChild(palH4);

                    var allComps = [];
                    var currentLib = app.state.selectedLibrary || '';
                    var currentComps = (app.state.componentData && app.state.componentData.components) || [];
                    currentComps.forEach(function(c) {
                        if (c.tag !== component.tag) {
                            allComps.push({ tag: c.tag, display_name: c.display_name, library: currentLib });
                        }
                    });

                    var palette = app.widgets.ComponentPalette.render(
                        { library: currentLib },
                        { components: allComps },
                        {}
                    );
                    rightPanel.appendChild(palette);
                }
            }
        },

        _saveComponentCss: async function(cssText) {
            var idx = app.state.selectedComponentIdx;
            var components = (app.state.componentData && app.state.componentData.components) || [];
            var component = components[idx];
            if (!component) return;
            if (cssText === undefined) {
                // Fallback: read from widget textarea
                var ta = document.querySelector('.azd-css-textarea');
                if (ta) cssText = ta.value; else return;
            }
            try {
                var res = await app.api.post({
                    op: 'update_component',
                    library: app.state.selectedLibrary,
                    name: component.tag,
                    css: cssText,
                });
                if (res.status === 'ok') {
                    app.log('CSS saved for ' + component.tag, 'info');
                    app.handlers.selectLibrary(app.state.selectedLibrary);
                } else {
                    app.log('Failed to save CSS: ' + (res.message || ''), 'error');
                }
            } catch(e) {
                app.log('Save CSS failed: ' + e.message, 'error');
            }
        },

        _loadComponentPreview: async function(component, previewEl) {
            if (!previewEl) previewEl = document.getElementById('component-preview-widget');
            if (!previewEl) { console.warn('[dbg] preview: no previewEl found'); return; }
            try {
                var payload = {
                    op: 'get_component_preview',
                    library: app.state.selectedLibrary || 'builtin',
                    name: component.tag,
                };
                // H4: pass environment overrides if set
                if (app.state.previewOs) payload.override_os = app.state.previewOs;
                if (app.state.previewTheme) payload.override_theme = app.state.previewTheme;
                if (app.state.previewLang) payload.override_lang = app.state.previewLang;
                console.log('[dbg] preview: posting', JSON.stringify(payload));
                var res = await app.api.post(payload);
                console.log('[dbg] preview: response status=' + res.status, 'data keys=', res.data ? Object.keys(res.data) : 'none');
                if (res.status === 'ok' && res.data && res.data.value) {
                    var preview = res.data.value;
                    console.log('[dbg] preview: got value, keys=', Object.keys(preview), 'has data=', !!preview.data, 'w=', preview.width, 'h=', preview.height);
                    app.widgets.PreviewPanel.update(previewEl, {
                        imageDataUri: preview.data,
                        width: preview.width,
                        height: preview.height,
                        loading: false
                    });
                } else if (res.status === 'ok' && res.data) {
                    // Try alternate response format — data directly on res.data
                    var alt = res.data;
                    if (alt.data || alt.image || alt.png) {
                        console.log('[dbg] preview: using alt format');
                        app.widgets.PreviewPanel.update(previewEl, {
                            imageDataUri: alt.data || alt.image || alt.png,
                            width: alt.width || 0,
                            height: alt.height || 0,
                            loading: false
                        });
                    } else {
                        app.widgets.PreviewPanel.update(previewEl, {
                            error: 'Preview: unexpected response format',
                            loading: false
                        });
                        console.warn('[dbg] preview: unexpected data format', JSON.stringify(res.data).substring(0, 300));
                    }
                } else {
                    app.widgets.PreviewPanel.update(previewEl, {
                        error: 'Preview failed: ' + (res.message || 'Unknown error'),
                        loading: false
                    });
                }
            } catch (e) {
                app.widgets.PreviewPanel.update(previewEl, {
                    error: 'Preview error: ' + e.message,
                    loading: false
                });
            }
        },

        /* ── Mini HTML Tree (D1, D3, D4, D5, D6) ── */

        _miniTreeCollapsed: new Set(),
        _miniTreeSelectedIdx: null,
        _miniTreeNodes: null,

        _loadMiniTree: async function(component, container) {
            if (!container) container = document.getElementById('component-mini-tree-container');
            if (!container) return;
            container.innerHTML = '<span class="azd-muted">Loading render tree\u2026</span>';
            try {
                var res = await app.api.post({
                    op: 'get_component_render_tree',
                    library: app.state.selectedLibrary || 'builtin',
                    name: component.tag,
                });
                if (res.status === 'ok' && res.data && res.data.value) {
                    var treeData = res.data.value;
                    var nodes = treeData.nodes || (treeData.root ? [treeData.root] : []);
                    app.handlers._miniTreeNodes = nodes;
                    app.handlers._renderMiniTree(component, container, nodes);
                } else {
                    // Fallback: build a placeholder from the component's data model
                    var nodes = app.handlers._buildPlaceholderTree(component);
                    app.handlers._miniTreeNodes = nodes;
                    app.handlers._renderMiniTree(component, container, nodes);
                }
            } catch(e) {
                // Fallback: placeholder tree from data model
                var nodes = app.handlers._buildPlaceholderTree(component);
                app.handlers._miniTreeNodes = nodes;
                app.handlers._renderMiniTree(component, container, nodes);
            }
        },

        _buildPlaceholderTree: function(component) {
            // Build a simple tree from the component's data model
            var root = { tag: component.tag || 'div', children: [], classes: [] };
            var dm = component.data_model || [];
            dm.forEach(function(field) {
                var ft = field.field_type_structured || field.field_type;
                if (typeof ft === 'string') ft = app.widgets._parseFieldType(ft);
                if (!ft) ft = { type: 'String' };
                var kind = ft.kind || ft.type || 'String';
                if (kind === 'Primitive' && ft.name === 'String' || kind === 'String') {
                    root.children.push({ tag: 'p', text: '{' + field.name + '}', children: [] });
                } else if (kind === 'Primitive' && ft.name === 'Bool' || kind === 'Bool') {
                    root.children.push({ tag: 'span', text: field.name + ': {bool}', children: [] });
                } else if (kind === 'StructRef') {
                    root.children.push({ tag: ft.name || 'component', children: [], classes: ['component-ref'] });
                } else {
                    root.children.push({ tag: 'span', text: field.name, children: [] });
                }
            });
            return [root];
        },

        _renderMiniTree: function(component, container, nodes) {
            if (!container) container = document.getElementById('component-mini-tree-container');
            if (!container) return;
            container.innerHTML = '';
            var isEditable = component.source === 'user_defined';

            var tree = app.widgets.MiniHtmlTree.render(
                { editable: isEditable },
                {
                    nodes: nodes,
                    collapsed: app.handlers._miniTreeCollapsed,
                    selectedIdx: app.handlers._miniTreeSelectedIdx
                },
                {
                    onSelect: function(idx) {
                        app.handlers._miniTreeSelectedIdx = idx;
                        app.handlers._renderMiniTree(component, container, nodes);
                    },
                    onToggle: function(idx) {
                        app.handlers._renderMiniTree(component, container, nodes);
                    },
                    onDrop: function(data, parentIdx, position) {
                        app.handlers._handleTreeDrop(component, data, parentIdx, position);
                    },
                    onContextMenu: function(e, node) {
                        app.handlers._showTreeContextMenu(e, component, node);
                    }
                }
            );
            container.appendChild(tree);
        },

        _handleTreeDrop: async function(component, data, parentIdx, position) {
            if (!data || data.type !== 'component') return;
            app.log('Drop: ' + data.component + ' into parent=' + parentIdx + ' pos=' + position, 'info');

            // Find the parent node and insert a new child
            var nodes = app.handlers._miniTreeNodes;
            if (!nodes) return;

            var targetNode = null;
            function findByIdx(node) {
                if (node._idx === parentIdx) { targetNode = node; return; }
                if (node.children) node.children.forEach(findByIdx);
            }
            if (parentIdx === -1) {
                // Insert at root level
                nodes.splice(position, 0, {
                    tag: data.component,
                    children: [],
                    classes: ['component-instance'],
                    _component: { library: data.library, component: data.component }
                });
            } else {
                nodes.forEach(findByIdx);
                if (targetNode) {
                    if (!targetNode.children) targetNode.children = [];
                    targetNode.children.splice(position, 0, {
                        tag: data.component,
                        children: [],
                        classes: ['component-instance'],
                        _component: { library: data.library, component: data.component }
                    });
                }
            }

            // Re-render tree
            var container = document.getElementById('component-mini-tree-container');
            app.handlers._renderMiniTree(component, container, nodes);

            // Notify server (D6)
            app.handlers._syncTreeToServer(component);
        },

        _showTreeContextMenu: function(e, component, node) {
            var items = [];
            if (component.source === 'user_defined') {
                items.push({
                    icon: 'add',
                    label: 'Insert Child\u2026',
                    action: function() {
                        var tag = prompt('Child tag name:', 'div');
                        if (!tag) return;
                        if (!node.children) node.children = [];
                        node.children.push({ tag: tag, children: [], text: '' });
                        var container = document.getElementById('component-mini-tree-container');
                        app.handlers._renderMiniTree(component, container, app.handlers._miniTreeNodes);
                        app.handlers._syncTreeToServer(component);
                    }
                });
                items.push({
                    icon: 'content_copy',
                    label: 'Duplicate',
                    action: function() {
                        var clone = JSON.parse(JSON.stringify(node));
                        delete clone._idx;
                        // Find parent and insert after
                        var nodes = app.handlers._miniTreeNodes;
                        function insertAfter(parent, children) {
                            for (var i = 0; i < children.length; i++) {
                                if (children[i]._idx === node._idx) {
                                    children.splice(i + 1, 0, clone);
                                    return true;
                                }
                                if (children[i].children && insertAfter(children[i], children[i].children)) return true;
                            }
                            return false;
                        }
                        insertAfter(null, nodes);
                        var container = document.getElementById('component-mini-tree-container');
                        app.handlers._renderMiniTree(component, container, nodes);
                        app.handlers._syncTreeToServer(component);
                    }
                });
                items.push({
                    icon: 'arrow_upward',
                    label: 'Move Up',
                    action: function() {
                        app.handlers._moveTreeNode(component, node._idx, -1);
                    }
                });
                items.push({
                    icon: 'arrow_downward',
                    label: 'Move Down',
                    action: function() {
                        app.handlers._moveTreeNode(component, node._idx, 1);
                    }
                });
                items.push({ separator: true });
                items.push({
                    icon: 'delete',
                    label: 'Delete',
                    danger: true,
                    action: function() {
                        var nodes = app.handlers._miniTreeNodes;
                        function removeNode(children) {
                            for (var i = 0; i < children.length; i++) {
                                if (children[i]._idx === node._idx) {
                                    children.splice(i, 1);
                                    return true;
                                }
                                if (children[i].children && removeNode(children[i].children)) return true;
                            }
                            return false;
                        }
                        removeNode(nodes);
                        var container = document.getElementById('component-mini-tree-container');
                        app.handlers._renderMiniTree(component, container, nodes);
                        app.handlers._syncTreeToServer(component);
                    }
                });
            }
            if (items.length) {
                app.widgets.ContextMenu.show(e.clientX, e.clientY, items);
            }
        },

        _moveTreeNode: function(component, nodeIdx, direction) {
            var nodes = app.handlers._miniTreeNodes;
            function findAndMove(children) {
                for (var i = 0; i < children.length; i++) {
                    if (children[i]._idx === nodeIdx) {
                        var newPos = i + direction;
                        if (newPos < 0 || newPos >= children.length) return false;
                        var tmp = children[i];
                        children[i] = children[newPos];
                        children[newPos] = tmp;
                        return true;
                    }
                    if (children[i].children && findAndMove(children[i].children)) return true;
                }
                return false;
            }
            findAndMove(nodes);
            var container = document.getElementById('component-mini-tree-container');
            app.handlers._renderMiniTree(component, container, nodes);
            app.handlers._syncTreeToServer(component);
        },

        _syncTreeToServer: async function(component) {
            // D6: Send tree structure change to server
            try {
                var res = await app.api.post({
                    op: 'update_component',
                    library: app.state.selectedLibrary,
                    name: component.tag,
                    // Send the structure as JSON for the server to interpret
                    render_tree: app.handlers._miniTreeNodes
                });
                if (res.status === 'ok') {
                    // Re-render preview
                    var previewEl = document.getElementById('component-preview-widget');
                    app.handlers._loadComponentPreview(component, previewEl);
                } else {
                    app.log('Sync failed: ' + (res.message || ''), 'error');
                }
            } catch(e) {
                app.log('Sync error: ' + e.message, 'error');
            }
        },

        /* ── Source Code Editors (E1-E3) ── */

        _openRenderFnEditor: async function(component) {
            // E1: Load render_fn source and open popup editor
            var code = '';
            try {
                var res = await app.api.post({
                    op: 'get_component_source',
                    library: app.state.selectedLibrary,
                    name: component.tag,
                    source_type: 'render_fn',
                });
                if (res.status === 'ok' && res.data && res.data.value) {
                    code = res.data.value.source || '';
                }
            } catch(e) {
                app.log('Failed to load render_fn source: ' + e.message, 'error');
            }

            app.widgets.SourceEditor.open(
                { title: 'Edit render_fn — ' + (component.display_name || component.tag), language: 'rust' },
                { code: code },
                {
                    onSave: async function(newCode) {
                        try {
                            var res = await app.api.post({
                                op: 'update_component_render_fn',
                                library: app.state.selectedLibrary,
                                name: component.tag,
                                source: newCode,
                            });
                            if (res.status === 'ok') {
                                app.log('render_fn saved for ' + component.tag, 'info');
                                app.widgets.SourceEditor.close();
                                // Refresh preview
                                var previewEl = document.getElementById('component-preview-widget');
                                app.handlers._loadComponentPreview(component, previewEl);
                                app.handlers._loadMiniTree(component);
                            } else {
                                app.log('Save failed: ' + (res.message || ''), 'error');
                            }
                        } catch(e) {
                            app.log('Save error: ' + e.message, 'error');
                        }
                    },
                    onClose: function() {}
                }
            );
        },

        _openCompileFnEditor: async function(component, language) {
            // E2: Load compile_fn source for given language and open popup editor
            var code = '';
            try {
                var res = await app.api.post({
                    op: 'get_component_source',
                    library: app.state.selectedLibrary,
                    name: component.tag,
                    source_type: 'compile_fn',
                    language: language,
                });
                if (res.status === 'ok' && res.data && res.data.value) {
                    code = res.data.value.source || '';
                }
            } catch(e) {
                app.log('Failed to load compile_fn source: ' + e.message, 'error');
            }

            var languages = ['rust', 'c', 'cpp', 'python'];
            app.widgets.SourceEditor.open(
                {
                    title: 'Edit compile_fn — ' + (component.display_name || component.tag),
                    language: language,
                    languages: languages
                },
                { code: code },
                {
                    onSave: async function(newCode) {
                        try {
                            var res = await app.api.post({
                                op: 'update_component_compile_fn',
                                library: app.state.selectedLibrary,
                                name: component.tag,
                                source: newCode,
                                language: language,
                            });
                            if (res.status === 'ok') {
                                app.log('compile_fn (' + language + ') saved for ' + component.tag, 'info');
                                app.widgets.SourceEditor.close();
                            } else {
                                app.log('Save failed: ' + (res.message || ''), 'error');
                            }
                        } catch(e) {
                            app.log('Save error: ' + e.message, 'error');
                        }
                    },
                    onLanguageChange: function(newLang) {
                        app.widgets.SourceEditor.close();
                        app.handlers._openCompileFnEditor(component, newLang);
                    },
                    onClose: function() {}
                }
            );
        },

        /* ── Terminal ── */
        terminalKeydown: function(event, input) {
            var menu = document.getElementById('autocomplete-popup');
            if (menu) {
                if (event.key === 'ArrowUp') {
                    event.preventDefault();
                    app._acNavigate('up');
                    return;
                }
                if (event.key === 'ArrowDown') {
                    event.preventDefault();
                    app._acNavigate('down');
                    return;
                }
                if (event.key === 'Tab') {
                    event.preventDefault();
                    if (app._acSelectedIndex >= 0) {
                        app._acAccept();
                    } else {
                        app._acNavigate('down');
                    }
                    return;
                }
                if (event.key === 'Enter') {
                    if (app._acSelectedIndex >= 0) {
                        event.preventDefault();
                        app._acAccept();
                        return;
                    }
                    // No selection — fall through to send command
                }
                if (event.key === 'Escape') {
                    event.preventDefault();
                    app._hideAutocomplete();
                    return;
                }
            } else {
                if (event.key === 'Escape') return;
            }
            if (event.key === 'Enter') {
                app.handlers.terminalEnter(input);
            }
        },

        terminalInput: function(input) {
            var val = input.value;
            if (val.startsWith('/')) {
                // Only show autocomplete while user is still typing the command name
                // (i.e., no space yet, or they just typed "/")
                var spaceIdx = val.indexOf(' ');
                if (spaceIdx === -1) {
                    var filter = val.substring(1);
                    app._showAutocomplete(filter);
                } else {
                    // Already past command name — only show if filter still matches commands
                    var cmdPart = val.substring(1, spaceIdx);
                    var exact = app.schema.commands[cmdPart];
                    if (!exact) {
                        app._showAutocomplete(cmdPart);
                    } else {
                        app._hideAutocomplete();
                    }
                }
            } else {
                app._hideAutocomplete();
            }
        },

        terminalEnter: async function(input) {
            var val = input.value.trim();
            if (!val) return;

            var payload;
            if (val.startsWith('/')) {
                // Slash command: /cmd arg1 arg2 ...
                payload = _parseSlashCommand(val);
                if (!payload) {
                    app.log('Unknown command: ' + val, 'error');
                    return;
                }
            } else {
                // Raw JSON
                try {
                    payload = JSON.parse(val);
                } catch(e) {
                    app.log('Invalid JSON: ' + e.message, 'error');
                    return;
                }
            }

            app.log(JSON.stringify(payload), 'command');
            try {
                var res = await app.api.post(payload);
                app.log(JSON.stringify(res, null, 2), 'info');
            } catch(e) {
                app.log('Error: ' + e.message, 'error');
            }
            input.value = '';
            app._hideAutocomplete();
        },

        /* ── Persistence ── */
        save: function() {
            localStorage.setItem('azul_debugger', JSON.stringify({
                tests: app.state.tests,
                cssOverrides: app.state.cssOverrides,
                snapshots: app.state.snapshots,
                // Extra persistent state
                currentView: app.state.currentView,
                activeTestId: app.state.activeTestId,
                selectedLibrary: app.state.selectedLibrary,
                selectedNodeId: app.state.selectedNodeId,
                selectedComponentIdx: app.state.selectedComponentIdx,
                collapsedNodes: Array.from(app.state.collapsedNodes || []),
                previewOs: app.state.previewOs,
                previewTheme: app.state.previewTheme,
                previewLang: app.state.previewLang,
            }));
        },
    },

    /* ================================================================
     * RESIZER — drag-to-resize panels
     * ================================================================ */
    resizer: {
        init: function() {
            // Vertical resizers (between columns)
            document.querySelectorAll('.resizer').forEach(function(el) {
                el.addEventListener('mousedown', function(e) {
                    e.preventDefault();
                    var targetId = el.dataset.target;
                    var target = document.getElementById(targetId);
                    if (!target) return;
                    var minW = parseInt(el.dataset.min) || 100;
                    var maxW = parseInt(el.dataset.max) || 600;
                    var side = el.dataset.side || 'left'; // 'left' = width grows rightward, 'right' = width grows leftward
                    var startX = e.clientX;
                    var startW = target.offsetWidth;
                    el.classList.add('active');

                    function onMove(e2) {
                        var dx = e2.clientX - startX;
                        var newW = side === 'right' ? startW - dx : startW + dx;
                        newW = Math.max(minW, Math.min(maxW, newW));
                        target.style.width = newW + 'px';
                    }
                    function onUp() {
                        el.classList.remove('active');
                        document.removeEventListener('mousemove', onMove);
                        document.removeEventListener('mouseup', onUp);
                    }
                    document.addEventListener('mousemove', onMove);
                    document.addEventListener('mouseup', onUp);
                });
            });

            // Horizontal resizers (top of bottom panel)
            document.querySelectorAll('.resizer-h').forEach(function(el) {
                el.addEventListener('mousedown', function(e) {
                    e.preventDefault();
                    var targetId = el.dataset.target;
                    var target = document.getElementById(targetId);
                    if (!target) return;
                    var minH = parseInt(el.dataset.min) || 60;
                    var maxH = parseInt(el.dataset.max) || 600;
                    var startY = e.clientY;
                    var startH = target.offsetHeight;
                    el.classList.add('active');

                    function onMove(e2) {
                        var dy = startY - e2.clientY; // dragging up = increase
                        var newH = Math.max(minH, Math.min(maxH, startH + dy));
                        target.style.height = newH + 'px';
                    }
                    function onUp() {
                        el.classList.remove('active');
                        document.removeEventListener('mousemove', onMove);
                        document.removeEventListener('mouseup', onUp);
                    }
                    document.addEventListener('mousemove', onMove);
                    document.addEventListener('mouseup', onUp);
                });
            });
        }
    },

    /* ================================================================
     * JSON TREE — collapsible, editable, grouped for large arrays
     * ================================================================ */
    json: {
        GROUP_SIZE: 100,

        render: function(containerId, data, readOnly) {
            var container = document.getElementById(containerId);
            if (!container) return;
            container.innerHTML = '';
            if (data == null) {
                container.innerHTML = '<div class="placeholder-text">No data.</div>';
                return;
            }
            // Store data for re-render on collapse/expand
            if (!this._lastRenderData) this._lastRenderData = {};
            this._lastRenderData[containerId] = data;
            var tree = document.createElement('div');
            tree.className = 'json-tree';
            this._buildNode(tree, '', data, 0, '', !!readOnly, containerId);
            container.appendChild(tree);
        },

        _buildNode: function(container, key, value, depth, path, readOnly, containerId) {
            var self = this;
            var fullPath = path ? (path + '.' + key) : key;

            if (value === null || value === undefined) {
                this._addLeaf(container, key, '<span class="json-null">null</span>', depth, fullPath, 'null', readOnly, containerId);
            } else if (typeof value === 'boolean') {
                this._addLeaf(container, key, '<span class="json-bool">' + value + '</span>', depth, fullPath, 'bool', readOnly, containerId);
            } else if (typeof value === 'number') {
                this._addLeaf(container, key, '<span class="json-number">' + value + '</span>', depth, fullPath, 'number', readOnly, containerId);
            } else if (typeof value === 'string') {
                this._addLeaf(container, key, '<span class="json-string">"' + esc(value) + '"</span>', depth, fullPath, 'string', readOnly, containerId);
            } else if (Array.isArray(value)) {
                this._addCollapsible(container, key, value, depth, fullPath, true, readOnly, containerId);
            } else if (typeof value === 'object') {
                this._addCollapsible(container, key, value, depth, fullPath, false, readOnly, containerId);
            }
        },

        _addLeaf: function(container, key, valueHtml, depth, fullPath, valueType, readOnly, containerId) {
            var row = document.createElement('div');
            row.className = 'json-row';
            row.style.paddingLeft = (depth * 14 + 8) + 'px';
            var keyHtml = key !== '' ? '<span class="json-key">' + esc(key) + '</span>: ' : '';

            var valueSpan = document.createElement('span');
            valueSpan.className = 'json-value-editable';
            valueSpan.innerHTML = valueHtml;

            if (!readOnly) {
                valueSpan.title = 'Click to edit';
                valueSpan.dataset.path = fullPath;
                valueSpan.dataset.type = valueType;
                valueSpan.addEventListener('click', function() {
                    app.json._startEdit(valueSpan, fullPath, valueType);
                });
            }

            var rowInner = document.createElement('span');
            rowInner.innerHTML = '<span class="json-toggle-icon">&nbsp;</span>' + keyHtml;
            row.appendChild(rowInner);
            row.appendChild(valueSpan);
            container.appendChild(row);
        },

        _startEdit: function(el, path, valueType) {
            // Get current raw value from the app state JSON
            var rawValue = this._getValueAtPath(app.state.appStateJson, path);
            var input = document.createElement('input');
            input.type = 'text';
            input.className = 'json-edit-input';
            input.value = valueType === 'string' ? rawValue : JSON.stringify(rawValue);

            el.innerHTML = '';
            el.appendChild(input);
            input.focus();
            input.select();

            var self = this;
            var commit = function() {
                var newValStr = input.value.trim();
                var newVal;
                if (valueType === 'string') {
                    newVal = newValStr;
                } else if (valueType === 'number') {
                    newVal = parseFloat(newValStr);
                    if (isNaN(newVal)) newVal = 0;
                } else if (valueType === 'bool') {
                    newVal = newValStr === 'true';
                } else if (valueType === 'null') {
                    newVal = newValStr === 'null' ? null : newValStr;
                } else {
                    try { newVal = JSON.parse(newValStr); } catch(e) { newVal = newValStr; }
                }

                // Update the app state JSON in memory
                self._setValueAtPath(app.state.appStateJson, path, newVal);
                // Re-render the tree
                self.render('app-state-tree', app.state.appStateJson, false);
                // Auto-save to server
                app.handlers._autoSaveAppState();
            };

            input.addEventListener('blur', commit);
            input.addEventListener('keydown', function(e) {
                if (e.key === 'Enter') { commit(); }
                if (e.key === 'Escape') {
                    self.render('app-state-tree', app.state.appStateJson, false);
                }
            });
        },

        _getValueAtPath: function(obj, path) {
            if (!path) return obj;
            var parts = path.split('.');
            var current = obj;
            for (var i = 0; i < parts.length; i++) {
                if (current == null) return undefined;
                current = current[parts[i]];
            }
            return current;
        },

        _setValueAtPath: function(obj, path, value) {
            if (!path) return;
            var parts = path.split('.');
            var current = obj;
            for (var i = 0; i < parts.length - 1; i++) {
                if (current == null) return;
                current = current[parts[i]];
            }
            if (current != null) {
                current[parts[parts.length - 1]] = value;
            }
        },

        _addCollapsible: function(container, key, value, depth, path, isArray, readOnly, containerId) {
            var self = this;
            var collapsedSet = readOnly ? app.state.jsonReadOnlyCollapsed : app.state.jsonCollapsed;
            var groupCollapsedSet = readOnly ? app.state.jsonReadOnlyGroupCollapsed : app.state.jsonGroupCollapsed;
            var isCollapsed = collapsedSet.has(path);
            var entries = isArray ? value : Object.keys(value);
            var count = entries.length;
            var bracket = isArray ? ['[', ']'] : ['{', '}'];

            // Header row
            var row = document.createElement('div');
            row.className = 'json-row';
            row.style.paddingLeft = (depth * 14 + 8) + 'px';

            var toggle = document.createElement('span');
            toggle.className = 'json-toggle-icon';
            toggle.textContent = isCollapsed ? '▶' : '▼';
            toggle.addEventListener('click', function() {
                if (collapsedSet.has(path)) {
                    collapsedSet.delete(path);
                } else {
                    collapsedSet.add(path);
                }
                self.render(containerId, self._lastRenderData[containerId], readOnly);
            });
            row.appendChild(toggle);

            var label = document.createElement('span');
            var keyHtml = key !== '' ? '<span class="json-key">' + esc(key) + '</span>: ' : '';
            label.innerHTML = keyHtml + '<span class="json-bracket">' + bracket[0] + '</span>' +
                (isCollapsed ? '<span class="json-type-hint"> …' + count + ' items </span><span class="json-bracket">' + bracket[1] + '</span>' : '');
            row.appendChild(label);
            container.appendChild(row);

            // Children
            if (!isCollapsed) {
                var childContainer = document.createElement('div');
                childContainer.className = 'json-children';

                if (isArray && count > this.GROUP_SIZE) {
                    // Group large arrays into chunks
                    for (var g = 0; g < count; g += this.GROUP_SIZE) {
                        var gEnd = Math.min(g + this.GROUP_SIZE, count);
                        var groupPath = path + '.[' + g + '-' + (gEnd - 1) + ']';
                        var groupCollapsed = groupCollapsedSet.has(groupPath);

                        var groupRow = document.createElement('div');
                        groupRow.className = 'json-row';
                        groupRow.style.paddingLeft = ((depth + 1) * 14 + 8) + 'px';

                        var gToggle = document.createElement('span');
                        gToggle.className = 'json-toggle-icon';
                        gToggle.textContent = groupCollapsed ? '▶' : '▼';
                        (function(gp) {
                            gToggle.addEventListener('click', function() {
                                if (groupCollapsedSet.has(gp)) {
                                    groupCollapsedSet.delete(gp);
                                } else {
                                    groupCollapsedSet.add(gp);
                                }
                                self.render(containerId, self._lastRenderData[containerId], readOnly);
                            });
                        })(groupPath);
                        groupRow.appendChild(gToggle);

                        var gLabel = document.createElement('span');
                        gLabel.className = 'json-group-header';
                        gLabel.textContent = '[' + g + ' … ' + (gEnd - 1) + ']';
                        groupRow.appendChild(gLabel);
                        childContainer.appendChild(groupRow);

                        if (!groupCollapsed) {
                            for (var i = g; i < gEnd; i++) {
                                this._buildNode(childContainer, String(i), value[i], depth + 2, path, readOnly, containerId);
                            }
                        }
                    }
                } else if (isArray) {
                    for (var i = 0; i < count; i++) {
                        this._buildNode(childContainer, String(i), value[i], depth + 1, path, readOnly, containerId);
                    }
                } else {
                    var keys = Object.keys(value);
                    for (var k = 0; k < keys.length; k++) {
                        this._buildNode(childContainer, keys[k], value[keys[k]], depth + 1, path, readOnly, containerId);
                    }
                }

                // Closing bracket
                var closeRow = document.createElement('div');
                closeRow.className = 'json-row';
                closeRow.style.paddingLeft = (depth * 14 + 8) + 'px';
                closeRow.innerHTML = '<span class="json-toggle-icon">&nbsp;</span><span class="json-bracket">' + bracket[1] + '</span>';
                childContainer.appendChild(closeRow);

                container.appendChild(childContainer);
            }
        },
    },

    /* ================================================================
     * WIDGETS — reusable, composable UI components for type-aware editing
     *
     * Contract: each widget has:
     *   render(config, state, callbacks) → HTMLElement
     *   update(el, newState)             → void (optional, for perf)
     *
     * All widgets use createElement (never innerHTML for interactive parts).
     * Callbacks via config — widgets never touch global state directly.
     * ================================================================ */
    widgets: {

        /* ── W2: TypeBadge — color-coded type indicator ── */
        TypeBadge: {
            render: function(config) {
                var el = document.createElement('span');
                var ft = config.fieldType || {};
                var t = ft.type || 'Unknown';
                el.className = 'azd-type-badge azd-type-' + t.toLowerCase();
                switch (t) {
                    case 'String':    el.textContent = 'Str'; break;
                    case 'Bool':      el.textContent = 'Bool'; break;
                    case 'I32':       el.textContent = 'i32'; break;
                    case 'I64':       el.textContent = 'i64'; break;
                    case 'U32':       el.textContent = 'u32'; break;
                    case 'U64':       el.textContent = 'u64'; break;
                    case 'Usize':     el.textContent = 'usize'; break;
                    case 'F32':       el.textContent = 'f32'; break;
                    case 'F64':       el.textContent = 'f64'; break;
                    case 'ColorU':    el.textContent = '\u25A0'; el.style.color = '#f0f'; break;
                    case 'Option':    el.textContent = (ft.inner ? ft.inner.type : '?') + '?'; break;
                    case 'Vec':       el.textContent = '[' + (ft.inner ? ft.inner.type : '?') + ']'; break;
                    case 'StyledDom': el.textContent = '\u25FB Slot'; break;
                    case 'Callback':  el.textContent = 'fn()'; break;
                    case 'RefAny':    el.textContent = 'ref'; break;
                    case 'EnumRef':   el.textContent = ft.name || 'enum'; break;
                    case 'StructRef': el.textContent = ft.name || 'struct'; break;
                    case 'ImageRef':  el.textContent = 'img'; break;
                    case 'FontRef':   el.textContent = 'font'; break;
                    default:          el.textContent = t; break;
                }
                return el;
            }
        },

        /* ── W3: FieldInput — primitive input controls ── */
        FieldInput: {
            render: function(config, state, callbacks) {
                var ft = config.fieldType || {};
                var t = ft.type || 'String';
                switch (t) {
                    case 'String':   return this._renderString(config, state, callbacks);
                    case 'Bool':     return this._renderBool(config, state, callbacks);
                    case 'I32': case 'I64': case 'U32': case 'U64': case 'Usize':
                                     return this._renderInt(config, state, callbacks);
                    case 'F32': case 'F64':
                                     return this._renderFloat(config, state, callbacks);
                    case 'ColorU':   return this._renderColor(config, state, callbacks);
                    case 'Option':   return this._renderOption(config, state, callbacks);
                    case 'Vec':      return this._renderVec(config, state, callbacks);
                    case 'StyledDom':return this._renderSlot(config, state, callbacks);
                    case 'Callback': return this._renderCallback(config, state, callbacks);
                    case 'EnumRef':  return this._renderEnum(config, state, callbacks);
                    case 'StructRef':return this._renderStruct(config, state, callbacks);
                    default:         return this._renderString(config, state, callbacks);
                }
            },

            _renderString: function(config, state, callbacks) {
                var input = document.createElement('input');
                input.type = 'text';
                input.className = 'azd-input azd-input-string';
                input.value = state.value != null ? String(state.value) : '';
                input.placeholder = config.default || '';
                input.disabled = !!config.readOnly;
                if (config.description) input.title = config.description;
                input.addEventListener('input', function() {
                    if (callbacks.onChange) callbacks.onChange(config.name, { type: 'String', value: input.value });
                });
                return input;
            },

            _renderBool: function(config, state, callbacks) {
                var label = document.createElement('label');
                label.className = 'azd-input azd-input-bool';
                var cb = document.createElement('input');
                cb.type = 'checkbox';
                cb.checked = !!state.value;
                cb.disabled = !!config.readOnly;
                var text = document.createTextNode(cb.checked ? 'true' : 'false');
                cb.addEventListener('change', function() {
                    text.textContent = cb.checked ? 'true' : 'false';
                    if (callbacks.onChange) callbacks.onChange(config.name, { type: 'Bool', value: cb.checked });
                });
                label.appendChild(cb);
                label.appendChild(text);
                return label;
            },

            _renderInt: function(config, state, callbacks) {
                var input = document.createElement('input');
                input.type = 'number';
                input.className = 'azd-input azd-input-int';
                input.value = state.value != null ? state.value : 0;
                input.step = '1';
                input.disabled = !!config.readOnly;
                if (config.description) input.title = config.description;
                input.addEventListener('input', function() {
                    var v = parseInt(input.value, 10);
                    if (isNaN(v)) v = 0;
                    if (callbacks.onChange) callbacks.onChange(config.name, { type: config.fieldType.type, value: v });
                });
                return input;
            },

            _renderFloat: function(config, state, callbacks) {
                var input = document.createElement('input');
                input.type = 'number';
                input.className = 'azd-input azd-input-float';
                input.value = state.value != null ? state.value : 0;
                input.step = '0.1';
                input.disabled = !!config.readOnly;
                if (config.description) input.title = config.description;
                input.addEventListener('input', function() {
                    var v = parseFloat(input.value);
                    if (isNaN(v)) v = 0.0;
                    if (callbacks.onChange) callbacks.onChange(config.name, { type: config.fieldType.type, value: v });
                });
                return input;
            },

            _renderColor: function(config, state, callbacks) {
                var wrap = document.createElement('div');
                wrap.className = 'azd-input azd-input-color';
                var picker = document.createElement('input');
                picker.type = 'color';
                picker.value = state.value ? app.widgets._colorUToHex(state.value) : '#000000';
                picker.disabled = !!config.readOnly;
                var hex = document.createElement('span');
                hex.className = 'azd-color-hex';
                hex.textContent = picker.value;
                picker.addEventListener('input', function() {
                    hex.textContent = picker.value;
                    if (callbacks.onChange) callbacks.onChange(config.name, {
                        type: 'ColorU',
                        value: app.widgets._hexToColorU(picker.value)
                    });
                });
                wrap.appendChild(picker);
                wrap.appendChild(hex);
                return wrap;
            },

            _renderOption: function(config, state, callbacks) {
                var wrap = document.createElement('div');
                wrap.className = 'azd-input azd-input-option';
                var hasValue = state.value !== null && state.value !== undefined;

                var toggle = document.createElement('input');
                toggle.type = 'checkbox';
                toggle.checked = hasValue;
                toggle.disabled = !!config.readOnly;

                var innerWrap = document.createElement('div');
                innerWrap.className = 'azd-option-inner';

                var self = this;
                var rebuildInner = function(val) {
                    innerWrap.innerHTML = '';
                    if (val !== null && val !== undefined) {
                        var innerConfig = Object.assign({}, config, {
                            fieldType: config.fieldType.inner || { type: 'String' },
                            name: config.name
                        });
                        innerWrap.appendChild(self.render(innerConfig, { value: val }, callbacks));
                    } else {
                        var none = document.createElement('span');
                        none.className = 'azd-muted';
                        none.textContent = 'None';
                        innerWrap.appendChild(none);
                    }
                };

                rebuildInner(hasValue ? state.value : null);

                toggle.addEventListener('change', function() {
                    if (toggle.checked) {
                        var def = app.widgets._defaultForType(config.fieldType.inner || { type: 'String' });
                        rebuildInner(def);
                        if (callbacks.onChange) callbacks.onChange(config.name, { type: 'Some', value: def });
                    } else {
                        rebuildInner(null);
                        if (callbacks.onChange) callbacks.onChange(config.name, { type: 'None' });
                    }
                });

                wrap.appendChild(toggle);
                wrap.appendChild(innerWrap);
                return wrap;
            },

            _renderVec: function(config, state, callbacks) {
                var wrap = document.createElement('div');
                wrap.className = 'azd-input azd-input-vec';
                var items = state.value || [];

                var list = document.createElement('div');
                list.className = 'azd-vec-items';

                var self = this;
                items.forEach(function(item, idx) {
                    var itemRow = document.createElement('div');
                    itemRow.className = 'azd-vec-item';

                    var innerConfig = Object.assign({}, config, {
                        fieldType: config.fieldType.inner || { type: 'String' },
                        name: config.name + '[' + idx + ']'
                    });
                    itemRow.appendChild(self.render(innerConfig, { value: item }, {
                        onChange: function(_, newVal) {
                            var newItems = items.slice();
                            newItems[idx] = newVal.value;
                            if (callbacks.onChange) callbacks.onChange(config.name, { type: 'Vec', value: newItems });
                        }
                    }));

                    if (!config.readOnly) {
                        var removeBtn = document.createElement('button');
                        removeBtn.className = 'azd-btn-icon';
                        removeBtn.textContent = '\u00D7';
                        removeBtn.title = 'Remove';
                        removeBtn.addEventListener('click', function() {
                            var newItems = items.slice();
                            newItems.splice(idx, 1);
                            if (callbacks.onChange) callbacks.onChange(config.name, { type: 'Vec', value: newItems });
                        });
                        itemRow.appendChild(removeBtn);
                    }
                    list.appendChild(itemRow);
                });
                wrap.appendChild(list);

                if (!config.readOnly) {
                    var addBtn = document.createElement('button');
                    addBtn.className = 'azd-btn-small';
                    addBtn.textContent = '+ Add';
                    addBtn.addEventListener('click', function() {
                        var newItems = items.slice();
                        newItems.push(app.widgets._defaultForType(config.fieldType.inner || { type: 'String' }));
                        if (callbacks.onChange) callbacks.onChange(config.name, { type: 'Vec', value: newItems });
                    });
                    wrap.appendChild(addBtn);
                }
                return wrap;
            },

            _renderSlot: function(config, state, callbacks) {
                var zone = document.createElement('div');
                zone.className = 'azd-input azd-input-slot';
                zone.setAttribute('data-slot', config.name);

                if (state.value && state.value.component) {
                    zone.textContent = state.value.library + '.' + state.value.component;
                    zone.classList.add('azd-slot-filled');
                } else {
                    zone.textContent = 'Drop component here';
                    zone.classList.add('azd-slot-empty');
                }

                zone.addEventListener('dragover', function(e) {
                    e.preventDefault();
                    zone.classList.add('azd-slot-hover');
                });
                zone.addEventListener('dragleave', function() {
                    zone.classList.remove('azd-slot-hover');
                });
                zone.addEventListener('drop', function(e) {
                    e.preventDefault();
                    zone.classList.remove('azd-slot-hover');
                    try {
                        var data = JSON.parse(e.dataTransfer.getData('text/plain'));
                        if (callbacks.onChange) callbacks.onChange(config.name, {
                            type: 'ComponentInstance',
                            library: data.library,
                            component: data.component
                        });
                    } catch(err) { /* ignore invalid drops */ }
                });
                return zone;
            },

            _renderCallback: function(config, state, callbacks) {
                var wrap = document.createElement('div');
                wrap.className = 'azd-input azd-input-callback';

                var sig = (config.fieldType || {}).signature;
                var sigText = 'fn(';
                if (sig && sig.args) {
                    sigText += sig.args.map(function(a) { return a.name + ': ' + a.arg_type; }).join(', ');
                }
                sigText += ') \u2192 ' + (sig && sig.return_type ? sig.return_type : 'Update');

                var sigBadge = document.createElement('code');
                sigBadge.className = 'azd-callback-sig';
                sigBadge.textContent = sigText;
                wrap.appendChild(sigBadge);

                if (state.value && state.value.fn_name) {
                    var fnName = document.createElement('span');
                    fnName.className = 'azd-callback-fn';
                    fnName.textContent = state.value.fn_name;
                    wrap.appendChild(fnName);
                }
                return wrap;
            },

            _renderEnum: function(config, state, callbacks) {
                var select = document.createElement('select');
                select.className = 'azd-input azd-input-enum';
                select.disabled = !!config.readOnly;
                if (config.enumModel) {
                    config.enumModel.variants.forEach(function(v) {
                        var opt = document.createElement('option');
                        opt.value = v.name;
                        opt.textContent = v.name;
                        if (state.value && state.value.variant === v.name) opt.selected = true;
                        select.appendChild(opt);
                    });
                }
                select.addEventListener('change', function() {
                    if (callbacks.onChange) callbacks.onChange(config.name, {
                        type: 'Enum', variant: select.value, fields: []
                    });
                });
                return select;
            },

            _renderStruct: function(config, state, callbacks) {
                var wrap = document.createElement('div');
                wrap.className = 'azd-input azd-input-struct';

                var header = document.createElement('div');
                header.className = 'azd-struct-header';
                header.textContent = (config.fieldType || {}).name || 'struct';
                wrap.appendChild(header);

                if (config.structModel && config.structModel.fields) {
                    config.structModel.fields.forEach(function(field) {
                        var fieldEl = app.widgets.FieldEditor.render({
                            name: field.name,
                            fieldType: field.field_type,
                            required: field.required,
                            description: field.description,
                            readOnly: config.readOnly
                        }, {
                            value: (state.value && state.value[field.name]) || null
                        }, {
                            onChange: function(fieldName, newVal) {
                                var newStruct = Object.assign({}, state.value || {});
                                newStruct[fieldName] = newVal.value;
                                if (callbacks.onChange) callbacks.onChange(config.name, { type: 'Struct', value: newStruct });
                            }
                        });
                        wrap.appendChild(fieldEl);
                    });
                }
                return wrap;
            }
        },

        /* ── W1: FieldEditor — type-aware field row (label + badge + input) ── */
        FieldEditor: {
            render: function(config, state, callbacks) {
                var row = document.createElement('div');
                row.className = 'azd-field-row';

                // Label
                var label = document.createElement('label');
                label.className = 'azd-field-label';
                label.textContent = config.name;
                if (config.required) label.classList.add('azd-required');
                if (config.description) label.title = config.description;
                row.appendChild(label);

                // Type badge
                var badge = app.widgets.TypeBadge.render({ fieldType: config.fieldType || {} });
                row.appendChild(badge);

                // Input control
                var input = app.widgets.FieldInput.render(config, state, callbacks);
                row.appendChild(input);

                return row;
            }
        },

        /* ── W8: DataModelEditor — full data model editing panel ── */
        DataModelEditor: {
            render: function(config, state, callbacks) {
                var wrap = document.createElement('div');
                wrap.className = 'azd-data-model-editor';

                // Header
                var header = document.createElement('div');
                header.className = 'azd-dm-header';
                var dm = config.dataModel || {};
                var fields = dm.fields || [];
                header.textContent = (dm.name || 'Data Model') + ' (' + fields.length + ')';
                wrap.appendChild(header);

                // Struct-like display header
                var structHead = document.createElement('div');
                structHead.className = 'azd-dm-struct-head';
                structHead.textContent = 'struct ' + (dm.name || 'Data') + ' {';
                wrap.appendChild(structHead);

                // Field rows
                var fieldValues = state.fieldValues || {};
                fields.forEach(function(field) {
                    var ft = field.field_type || {};
                    // Parse field_type if it comes as a plain string from the API
                    if (typeof ft === 'string') {
                        ft = app.widgets._parseFieldType(ft);
                    }
                    var val = fieldValues[field.name];
                    if (val === undefined && field.default != null) {
                        val = field.default;
                    }

                    var fieldEl = app.widgets.FieldEditor.render({
                        name: field.name,
                        fieldType: ft,
                        required: field.required,
                        description: field.description,
                        readOnly: config.readOnly,
                        default: field.default != null ? String(field.default) : ''
                    }, {
                        value: val
                    }, {
                        onChange: function(name, newVal) {
                            if (callbacks.onFieldChange) callbacks.onFieldChange(name, newVal);
                        }
                    });
                    wrap.appendChild(fieldEl);
                });

                // Closing brace
                var structClose = document.createElement('div');
                structClose.className = 'azd-dm-struct-close';
                structClose.textContent = '}';
                wrap.appendChild(structClose);

                // Add field button (if editable)
                if (!config.readOnly && callbacks.onAddField) {
                    var addBtn = document.createElement('button');
                    addBtn.className = 'azd-btn-small';
                    addBtn.textContent = '+ Add Field';
                    addBtn.addEventListener('click', function() {
                        callbacks.onAddField();
                    });
                    wrap.appendChild(addBtn);
                }

                return wrap;
            }
        },

        /* ── W6: CssEditor — CSS editor with save support ── */
        CssEditor: {
            render: function(config, state, callbacks) {
                var wrap = document.createElement('div');
                wrap.className = 'azd-css-editor';

                var textarea = document.createElement('textarea');
                textarea.className = 'azd-css-textarea';
                textarea.value = state.css || '';
                textarea.disabled = !!config.readOnly;
                textarea.spellcheck = false;
                textarea.placeholder = config.readOnly ? '(no CSS)' : 'Enter component CSS\u2026';
                textarea.setAttribute('data-lang', 'css');

                var debounceTimer = null;
                textarea.addEventListener('input', function() {
                    clearTimeout(debounceTimer);
                    debounceTimer = setTimeout(function() {
                        if (callbacks.onChange) callbacks.onChange(textarea.value);
                    }, 150);
                });
                wrap.appendChild(textarea);

                // Error display
                if (state.errors && state.errors.length > 0) {
                    var errorList = document.createElement('div');
                    errorList.className = 'azd-css-errors';
                    state.errors.forEach(function(err) {
                        var errEl = document.createElement('div');
                        errEl.className = 'azd-css-error';
                        errEl.textContent = err;
                        errorList.appendChild(errEl);
                    });
                    wrap.appendChild(errorList);
                }

                // Save button for user_defined components
                if (!config.readOnly && callbacks.onSave) {
                    var saveBtn = document.createElement('button');
                    saveBtn.className = 'azd-btn-small';
                    saveBtn.textContent = 'Save CSS';
                    saveBtn.addEventListener('click', function() {
                        callbacks.onSave(textarea.value);
                    });
                    wrap.appendChild(saveBtn);
                }

                return wrap;
            }
        },

        /* ── W7: PreviewPanel — live component preview with context switcher ── */
        PreviewPanel: {
            render: function(config, state, callbacks) {
                var wrap = document.createElement('div');
                wrap.className = 'azd-preview-panel';

                // Preview image container
                var imgWrap = document.createElement('div');
                imgWrap.className = 'azd-preview-img-wrap';
                var img = document.createElement('img');
                img.className = 'azd-preview-img';
                if (state.imageDataUri) {
                    img.src = state.imageDataUri;
                    if (state.width && state.height) {
                        img.title = Math.round(state.width) + ' \u00D7 ' + Math.round(state.height) + ' px';
                    }
                }
                if (state.loading) {
                    imgWrap.classList.add('azd-loading');
                    if (!state.imageDataUri) {
                        var loadText = document.createElement('span');
                        loadText.className = 'azd-muted';
                        loadText.textContent = 'Loading preview\u2026';
                        imgWrap.appendChild(loadText);
                    }
                }
                imgWrap.appendChild(img);

                // Error display
                if (state.error) {
                    var errEl = document.createElement('div');
                    errEl.className = 'azd-preview-error';
                    errEl.textContent = state.error;
                    imgWrap.appendChild(errEl);
                }
                wrap.appendChild(imgWrap);

                // Refresh button
                if (callbacks.onRefresh) {
                    var refreshBtn = document.createElement('button');
                    refreshBtn.className = 'azd-btn-small';
                    refreshBtn.textContent = '\u21BB Refresh';
                    refreshBtn.addEventListener('click', function() {
                        callbacks.onRefresh();
                    });
                    wrap.appendChild(refreshBtn);
                }

                return wrap;
            },

            update: function(el, newState) {
                if (!el) return;
                var img = el.querySelector('.azd-preview-img');
                if (img && newState.imageDataUri) {
                    img.src = newState.imageDataUri;
                    if (newState.width && newState.height) {
                        img.title = Math.round(newState.width) + ' \u00D7 ' + Math.round(newState.height) + ' px';
                    }
                }
                var wrap = el.querySelector('.azd-preview-img-wrap');
                if (wrap) {
                    if (newState.loading) wrap.classList.add('azd-loading');
                    else wrap.classList.remove('azd-loading');
                    // Remove "Loading preview..." text when image arrives
                    if (!newState.loading) {
                        var loadText = wrap.querySelector('.azd-muted');
                        if (loadText) loadText.remove();
                    }
                }
                var errEl = el.querySelector('.azd-preview-error');
                if (errEl && newState.error) {
                    errEl.textContent = newState.error;
                } else if (errEl && !newState.error) {
                    errEl.remove();
                } else if (!errEl && newState.error) {
                    var newErr = document.createElement('div');
                    newErr.className = 'azd-preview-error';
                    newErr.textContent = newState.error;
                    if (wrap) wrap.appendChild(newErr);
                }
            }
        },

        /* ── Utility helpers ── */

        /**
         * Parse a flat field_type string from the API into a structured object.
         * Examples:
         *   "String"            → { type: "String" }
         *   "Option<String>"    → { type: "Option", inner: { type: "String" } }
         *   "Vec<i32>"          → { type: "Vec", inner: { type: "i32" } }
         *   "Callback(Sig)"     → { type: "Callback", signature: "Sig" }
         *   "bool"              → { type: "Bool" }
         */
        _parseFieldType: function(s) {
            if (!s || typeof s !== 'string') return s && typeof s === 'object' ? s : { type: 'String' };
            s = s.trim();
            // Normalize lowercase builtins
            var lcMap = { 'string': 'String', 'bool': 'Bool', 'i32': 'I32', 'i64': 'I64',
                'u32': 'U32', 'u64': 'U64', 'usize': 'Usize', 'f32': 'F32', 'f64': 'F64' };
            if (lcMap[s]) return { type: lcMap[s] };
            if (lcMap[s.toLowerCase()]) return { type: lcMap[s.toLowerCase()] };
            // Option<T>
            var m = s.match(/^Option<(.+)>$/);
            if (m) return { type: 'Option', inner: this._parseFieldType(m[1]) };
            // Vec<T>
            m = s.match(/^Vec<(.+)>$/);
            if (m) return { type: 'Vec', inner: this._parseFieldType(m[1]) };
            // Callback(Sig)
            m = s.match(/^Callback\((.+)\)$/);
            if (m) return { type: 'Callback', signature: m[1] };
            // Known types
            var known = ['String','Bool','I32','I64','U32','U64','Usize','F32','F64',
                'ColorU','CssProperty','ImageRef','FontRef','StyledDom','RefAny'];
            for (var i = 0; i < known.length; i++) {
                if (s === known[i]) return { type: s };
            }
            // Assume StructRef for unknown capitalized names, else fallback
            if (s[0] === s[0].toUpperCase()) return { type: 'StructRef', name: s };
            return { type: 'String' };
        },

        _colorUToHex: function(c) {
            return '#' + [c.r, c.g, c.b].map(function(v) {
                return ('0' + (v || 0).toString(16)).slice(-2);
            }).join('');
        },

        _hexToColorU: function(hex) {
            var r = parseInt(hex.slice(1, 3), 16) || 0;
            var g = parseInt(hex.slice(3, 5), 16) || 0;
            var b = parseInt(hex.slice(5, 7), 16) || 0;
            return { r: r, g: g, b: b, a: 255 };
        },

        _defaultForType: function(ft) {
            if (!ft) return null;
            switch (ft.type) {
                case 'String':   return '';
                case 'Bool':     return false;
                case 'I32': case 'I64': case 'U32': case 'U64': case 'Usize': return 0;
                case 'F32': case 'F64': return 0.0;
                case 'ColorU':   return { r: 0, g: 0, b: 0, a: 255 };
                case 'Option':   return null;
                case 'Vec':      return [];
                default:         return null;
            }
        },

        _findModel: function(models, name) {
            if (!models) return null;
            for (var i = 0; i < models.length; i++) {
                if (models[i].name === name) return models[i];
            }
            return null;
        },

        /* ── W9: MiniHtmlTree — navigable DOM tree of render_fn output (D1, D3, D5) ── */
        MiniHtmlTree: {
            render: function(config, state, callbacks) {
                var wrap = document.createElement('div');
                wrap.className = 'azd-mini-tree';
                var nodes = state.nodes || [];
                var collapsed = state.collapsed || new Set();
                var selectedIdx = state.selectedIdx;
                var self = this;

                function buildNode(node, depth, parentIdx, siblingIdx) {
                    // Drop zone BEFORE this node (sibling insertion)
                    if (config.editable) {
                        var dz = document.createElement('div');
                        dz.className = 'azd-drop-zone';
                        dz.setAttribute('data-parent', parentIdx);
                        dz.setAttribute('data-pos', siblingIdx);
                        dz.addEventListener('dragover', function(e) { e.preventDefault(); dz.classList.add('azd-drop-hover'); });
                        dz.addEventListener('dragleave', function() { dz.classList.remove('azd-drop-hover'); });
                        dz.addEventListener('drop', function(e) {
                            e.preventDefault(); dz.classList.remove('azd-drop-hover');
                            try {
                                var data = JSON.parse(e.dataTransfer.getData('text/plain'));
                                if (callbacks.onDrop) callbacks.onDrop(data, parseInt(dz.getAttribute('data-parent')), parseInt(dz.getAttribute('data-pos')));
                            } catch(err) {}
                        });
                        wrap.appendChild(dz);
                    }

                    var row = document.createElement('div');
                    row.className = 'azd-mini-tree-node';
                    row.style.paddingLeft = (depth * 14 + 6) + 'px';
                    if (node._idx === selectedIdx) row.classList.add('azd-selected');

                    // Toggle
                    var hasChildren = node.children && node.children.length > 0;
                    var toggle = document.createElement('span');
                    toggle.className = 'azd-mini-tree-toggle';
                    if (hasChildren) {
                        var isCollapsed = collapsed.has(node._idx);
                        toggle.textContent = isCollapsed ? '\u25B6' : '\u25BC';
                        toggle.addEventListener('click', function(e) {
                            e.stopPropagation();
                            if (collapsed.has(node._idx)) collapsed.delete(node._idx);
                            else collapsed.add(node._idx);
                            if (callbacks.onToggle) callbacks.onToggle(node._idx);
                        });
                    } else {
                        toggle.innerHTML = '&nbsp;';
                    }
                    row.appendChild(toggle);

                    // Tag name
                    var tag = document.createElement('span');
                    tag.className = 'azd-mini-tree-tag';
                    tag.textContent = '<' + (node.tag || 'div') + '>';
                    row.appendChild(tag);

                    // Text content preview
                    if (node.text) {
                        var textSpan = document.createElement('span');
                        textSpan.className = 'azd-mini-tree-text';
                        textSpan.textContent = node.text.length > 30 ? node.text.substring(0, 30) + '\u2026' : node.text;
                        row.appendChild(textSpan);
                    }

                    // Classes
                    if (node.classes && node.classes.length) {
                        var cls = document.createElement('span');
                        cls.className = 'azd-mini-tree-class';
                        cls.textContent = '.' + node.classes.join('.');
                        row.appendChild(cls);
                    }

                    // Click to select
                    row.addEventListener('click', function() {
                        if (callbacks.onSelect) callbacks.onSelect(node._idx);
                    });

                    // Right-click context menu (D5)
                    if (config.editable) {
                        row.addEventListener('contextmenu', function(e) {
                            e.preventDefault();
                            if (callbacks.onContextMenu) callbacks.onContextMenu(e, node);
                        });

                        // Drop INTO this node (as child)
                        row.addEventListener('dragover', function(e) {
                            e.preventDefault();
                            row.classList.add('azd-drop-into');
                        });
                        row.addEventListener('dragleave', function() {
                            row.classList.remove('azd-drop-into');
                        });
                        row.addEventListener('drop', function(e) {
                            e.preventDefault(); row.classList.remove('azd-drop-into');
                            try {
                                var data = JSON.parse(e.dataTransfer.getData('text/plain'));
                                var childCount = (node.children || []).length;
                                if (callbacks.onDrop) callbacks.onDrop(data, node._idx, childCount);
                            } catch(err) {}
                        });
                    }

                    wrap.appendChild(row);

                    // Recurse children
                    if (hasChildren && !collapsed.has(node._idx)) {
                        node.children.forEach(function(child, ci) {
                            buildNode(child, depth + 1, node._idx, ci);
                        });
                        // Final drop zone after last child
                        if (config.editable) {
                            var dzLast = document.createElement('div');
                            dzLast.className = 'azd-drop-zone';
                            dzLast.setAttribute('data-parent', node._idx);
                            dzLast.setAttribute('data-pos', node.children.length);
                            dzLast.addEventListener('dragover', function(e) { e.preventDefault(); dzLast.classList.add('azd-drop-hover'); });
                            dzLast.addEventListener('dragleave', function() { dzLast.classList.remove('azd-drop-hover'); });
                            dzLast.addEventListener('drop', function(e) {
                                e.preventDefault(); dzLast.classList.remove('azd-drop-hover');
                                try {
                                    var data = JSON.parse(e.dataTransfer.getData('text/plain'));
                                    if (callbacks.onDrop) callbacks.onDrop(data, parseInt(dzLast.getAttribute('data-parent')), parseInt(dzLast.getAttribute('data-pos')));
                                } catch(err) {}
                            });
                            wrap.appendChild(dzLast);
                        }
                    }
                }

                // Index nodes for lookup
                var idx = 0;
                function indexNodes(node) {
                    node._idx = idx++;
                    if (node.children) node.children.forEach(indexNodes);
                }
                nodes.forEach(indexNodes);

                // Build tree
                nodes.forEach(function(rootNode, ri) {
                    buildNode(rootNode, 0, -1, ri);
                });

                if (!nodes.length) {
                    var empty = document.createElement('div');
                    empty.className = 'azd-muted';
                    empty.style.padding = '8px';
                    empty.textContent = 'No render output available.';
                    wrap.appendChild(empty);
                }

                return wrap;
            }
        },

        /* ── W10: ComponentPalette — draggable component items (D2) ── */
        ComponentPalette: {
            render: function(config, state, callbacks) {
                var wrap = document.createElement('div');
                wrap.className = 'azd-component-palette';
                var items = state.components || [];
                var filter = (state.filter || '').toLowerCase();

                items.forEach(function(comp) {
                    var name = (comp.display_name || comp.tag || '').toLowerCase();
                    if (filter && name.indexOf(filter) === -1) return;

                    var item = document.createElement('div');
                    item.className = 'azd-palette-item';
                    item.draggable = true;
                    item.setAttribute('data-library', comp.library || '');
                    item.setAttribute('data-component', comp.tag || '');

                    var icon = document.createElement('div');
                    icon.className = 'azd-palette-icon';
                    icon.textContent = (comp.tag || 'C')[0].toUpperCase();
                    item.appendChild(icon);

                    var label = document.createElement('span');
                    label.className = 'azd-palette-label';
                    label.textContent = comp.display_name || comp.tag;
                    item.appendChild(label);

                    if (comp.tag) {
                        var tagSpan = document.createElement('span');
                        tagSpan.className = 'azd-palette-tag';
                        tagSpan.textContent = comp.tag;
                        item.appendChild(tagSpan);
                    }

                    // Drag start
                    item.addEventListener('dragstart', function(e) {
                        e.dataTransfer.setData('text/plain', JSON.stringify({
                            type: 'component',
                            library: comp.library || config.library || '',
                            component: comp.tag || ''
                        }));
                        e.dataTransfer.effectAllowed = 'copy';
                    });

                    wrap.appendChild(item);
                });

                if (!items.length) {
                    var empty = document.createElement('div');
                    empty.className = 'azd-muted';
                    empty.style.padding = '8px';
                    empty.textContent = 'No components available.';
                    wrap.appendChild(empty);
                }

                return wrap;
            }
        },

        /* ── W11: ContextMenu — right-click context menu (D5) ── */
        ContextMenu: {
            _el: null,

            show: function(x, y, items) {
                this.hide();
                var menu = document.createElement('div');
                menu.className = 'azd-context-menu';
                menu.style.left = x + 'px';
                menu.style.top = y + 'px';

                items.forEach(function(item) {
                    if (item.separator) {
                        var sep = document.createElement('div');
                        sep.className = 'azd-context-menu-sep';
                        menu.appendChild(sep);
                        return;
                    }
                    var el = document.createElement('div');
                    el.className = 'azd-context-menu-item';
                    if (item.danger) el.classList.add('azd-danger');
                    if (item.icon) {
                        var iconEl = document.createElement('span');
                        iconEl.className = 'material-icons';
                        iconEl.style.fontSize = '14px';
                        iconEl.textContent = item.icon;
                        el.appendChild(iconEl);
                    }
                    var label = document.createElement('span');
                    label.textContent = item.label;
                    el.appendChild(label);
                    el.addEventListener('click', function() {
                        app.widgets.ContextMenu.hide();
                        if (item.action) item.action();
                    });
                    menu.appendChild(el);
                });

                document.body.appendChild(menu);
                this._el = menu;

                // Close on click outside
                var self = this;
                setTimeout(function() {
                    document.addEventListener('click', self._onOutsideClick);
                    document.addEventListener('contextmenu', self._onOutsideClick);
                }, 0);
            },

            hide: function() {
                if (this._el) {
                    this._el.remove();
                    this._el = null;
                }
                document.removeEventListener('click', this._onOutsideClick);
                document.removeEventListener('contextmenu', this._onOutsideClick);
            },

            _onOutsideClick: function() {
                app.widgets.ContextMenu.hide();
            }
        },

        /* ── W12: SourceEditor — popup code editor (E1-E5) ── */
        SourceEditor: {
            _overlay: null,

            /**
             * Open a source code editor popup.
             * @param {Object} config - { title, language, readOnly }
             * @param {Object} state  - { code }
             * @param {Object} callbacks - { onSave(code), onClose() }
             */
            open: function(config, state, callbacks) {
                this.close();
                var overlay = document.createElement('div');
                overlay.className = 'azd-popup-overlay';

                var popup = document.createElement('div');
                popup.className = 'azd-popup';

                // Header
                var header = document.createElement('div');
                header.className = 'azd-popup-header';
                var titleEl = document.createElement('span');
                titleEl.textContent = config.title || 'Source Editor';
                header.appendChild(titleEl);
                var closeBtn = document.createElement('button');
                closeBtn.className = 'azd-popup-close';
                closeBtn.textContent = '\u00D7';
                closeBtn.addEventListener('click', function() {
                    app.widgets.SourceEditor.close();
                    if (callbacks.onClose) callbacks.onClose();
                });
                header.appendChild(closeBtn);
                popup.appendChild(header);

                // Language tabs (for compile_fn)
                if (config.languages && config.languages.length > 1) {
                    var tabs = document.createElement('div');
                    tabs.className = 'azd-lang-tabs';
                    config.languages.forEach(function(lang) {
                        var tab = document.createElement('div');
                        tab.className = 'azd-lang-tab';
                        if (lang === config.language) tab.classList.add('active');
                        tab.textContent = lang;
                        tab.addEventListener('click', function() {
                            if (callbacks.onLanguageChange) callbacks.onLanguageChange(lang);
                        });
                        tabs.appendChild(tab);
                    });
                    popup.appendChild(tabs);
                }

                // Body — code textarea
                var body = document.createElement('div');
                body.className = 'azd-popup-body';
                var textarea = document.createElement('textarea');
                textarea.className = 'azd-code-editor';
                textarea.value = state.code || '';
                textarea.readOnly = !!config.readOnly;
                textarea.spellcheck = false;
                textarea.setAttribute('data-lang', config.language || 'rust');

                // Tab key support in textarea
                textarea.addEventListener('keydown', function(e) {
                    if (e.key === 'Tab') {
                        e.preventDefault();
                        var start = textarea.selectionStart;
                        var end = textarea.selectionEnd;
                        textarea.value = textarea.value.substring(0, start) + '    ' + textarea.value.substring(end);
                        textarea.selectionStart = textarea.selectionEnd = start + 4;
                    }
                    // Ctrl+S / Cmd+S to save
                    if ((e.ctrlKey || e.metaKey) && e.key === 's') {
                        e.preventDefault();
                        if (!config.readOnly && callbacks.onSave) {
                            callbacks.onSave(textarea.value);
                        }
                    }
                    // Escape to close
                    if (e.key === 'Escape') {
                        app.widgets.SourceEditor.close();
                        if (callbacks.onClose) callbacks.onClose();
                    }
                });
                body.appendChild(textarea);
                popup.appendChild(body);

                // Footer
                var footer = document.createElement('div');
                footer.className = 'azd-popup-footer';
                if (!config.readOnly && callbacks.onSave) {
                    var saveBtn = document.createElement('button');
                    saveBtn.className = 'azd-btn-small';
                    saveBtn.textContent = 'Save';
                    saveBtn.addEventListener('click', function() {
                        callbacks.onSave(textarea.value);
                    });
                    footer.appendChild(saveBtn);
                }
                var cancelBtn = document.createElement('button');
                cancelBtn.className = 'azd-btn-small';
                cancelBtn.style.background = 'var(--bg-alt)';
                cancelBtn.textContent = 'Close';
                cancelBtn.addEventListener('click', function() {
                    app.widgets.SourceEditor.close();
                    if (callbacks.onClose) callbacks.onClose();
                });
                footer.appendChild(cancelBtn);
                popup.appendChild(footer);

                overlay.appendChild(popup);
                document.body.appendChild(overlay);
                this._overlay = overlay;

                // Focus the textarea
                setTimeout(function() { textarea.focus(); }, 50);

                // Close on overlay click (outside popup)
                overlay.addEventListener('click', function(e) {
                    if (e.target === overlay) {
                        app.widgets.SourceEditor.close();
                        if (callbacks.onClose) callbacks.onClose();
                    }
                });
            },

            close: function() {
                if (this._overlay) {
                    this._overlay.remove();
                    this._overlay = null;
                }
            },

            /**
             * Basic syntax highlighting (E5).
             * Returns HTML with keyword/string/comment spans.
             */
            highlight: function(code, language) {
                if (!code) return '';
                var h = esc(code);
                // Comments
                h = h.replace(/(\/\/[^\n]*)/g, '<span class="azd-code-comment">$1</span>');
                h = h.replace(/(\/\*[\s\S]*?\*\/)/g, '<span class="azd-code-comment">$1</span>');
                h = h.replace(/(#[^\n]*)/g, '<span class="azd-code-comment">$1</span>');
                // Strings
                h = h.replace(/(&quot;(?:[^&]|&(?!quot;))*?&quot;)/g, '<span class="azd-code-string">$1</span>');
                // Numbers
                h = h.replace(/\b(\d+\.?\d*)\b/g, '<span class="azd-code-number">$1</span>');
                // Keywords per language
                var kwList = [];
                if (language === 'rust') {
                    kwList = ['fn','let','mut','pub','struct','enum','impl','use','mod','return','if','else','for','while','match','self','Self','true','false','const','static','type','where','trait','async','await','move','ref','loop','break','continue','unsafe','extern','crate','super','as','in','dyn','Box','Vec','Option','Result','Some','None','Ok','Err','String'];
                } else if (language === 'c' || language === 'cpp' || language === 'c++') {
                    kwList = ['void','int','char','float','double','bool','struct','enum','typedef','return','if','else','for','while','do','switch','case','break','continue','const','static','extern','sizeof','NULL','true','false','auto','class','public','private','protected','virtual','override','new','delete','namespace','using','template','typename'];
                } else if (language === 'python') {
                    kwList = ['def','class','return','if','elif','else','for','while','import','from','as','with','try','except','finally','raise','pass','break','continue','None','True','False','self','lambda','yield','global','nonlocal','and','or','not','in','is'];
                }
                if (kwList.length) {
                    var kwRegex = new RegExp('\\b(' + kwList.join('|') + ')\\b', 'g');
                    h = h.replace(kwRegex, '<span class="azd-code-keyword">$1</span>');
                }
                return h;
            }
        }
    },

    /* ================================================================
     * TEST RUNNER
     * ================================================================ */
    runner: {
        run: async function() {
            if (app.state.executionStatus === 'running') return;
            app.state.executionStatus = 'running';
            var test = app.state.tests.find(function(t) { return t.id === app.state.activeTestId; });
            if (!test) return;
            if (app.state.currentStepIndex === -1) {
                test.steps.forEach(function(s) { delete s.status; delete s.error; delete s.lastResponse; delete s.screenshot; delete s.duration_ms; });
                app.ui.renderSteps();
                app.state.currentStepIndex = 0;
            }
            this._loop();
        },

        _loop: async function() {
            var test = app.state.tests.find(function(t) { return t.id === app.state.activeTestId; });
            if (!test || app.state.executionStatus !== 'running') return;
            if (app.state.currentStepIndex >= test.steps.length) {
                app.state.executionStatus = 'idle';
                app.state.currentStepIndex = -1;
                app.ui.renderSteps();
                app.log('Test "' + test.name + '" completed.', 'info');
                return;
            }
            var idx = app.state.currentStepIndex;
            var step = test.steps[idx];
            app.ui.renderSteps();

            if (step.breakpoint && idx > 0 && !step._breakHit) {
                app.state.executionStatus = 'paused';
                step._breakHit = true;
                app.log('Paused at breakpoint: Step ' + (idx + 1), 'warning');
                return;
            }
            step._breakHit = false;

            var t0 = performance.now();
            try {
                var res;
                // Handle restore_snapshot locally
                if (step.op === 'restore_snapshot') {
                    var alias = (step.params && step.params.alias) || '';
                    var snapshot = app.state.snapshots[alias];
                    if (!snapshot) throw new Error('Snapshot not found: ' + alias);
                    res = await app.api.post({ op: 'set_app_state', state: snapshot });
                } else {
                    res = await app.api.post({ op: step.op, ...step.params });
                }
                step.lastResponse = res;
                step.duration_ms = Math.round(performance.now() - t0);
                // HTTP-level error
                if (res.status === 'error') throw new Error(res.message || 'Server error');
                // Semantic failure: check data.value for success/found/passed fields
                var val = (res.data && res.data.value) ? res.data.value : null;
                if (val) {
                    if (val.success === false) throw new Error(val.message || val.error || 'Operation failed (success: false)');
                    if (val.found === false) throw new Error('Target not found (found: false)');
                    if (val.passed === false) throw new Error(val.message || 'Assertion failed (passed: false)');
                }
                if (res.data && res.data.value && res.data.value.data && res.data.type === 'screenshot') step.screenshot = res.data.value.data;
                step.status = 'pass';
            } catch(e) {
                step.status = 'fail';
                step.error = e.message;
                step.duration_ms = Math.round(performance.now() - t0);
                app.state.executionStatus = 'idle';
                app.log('Step ' + (idx + 1) + ' failed: ' + e.message, 'error');
                app.ui.renderSteps();
                return;
            }
            app.state.currentStepIndex++;
            setTimeout(function() { app.runner._loop(); }, 100);
        },

        pause: function() {
            if (app.state.executionStatus === 'running') {
                app.state.executionStatus = 'paused';
                app.log('Paused.', 'warning');
            }
        },

        step: function() {
            if (app.state.executionStatus === 'paused' || (app.state.executionStatus === 'idle' && app.state.currentStepIndex === -1)) {
                if (app.state.currentStepIndex === -1) app.state.currentStepIndex = 0;
                app.state.executionStatus = 'running';
                app.runner._loop().then(function() {
                    if (app.state.executionStatus === 'running') app.state.executionStatus = 'paused';
                });
            }
        },

        reset: function() {
            app.state.executionStatus = 'idle';
            app.state.currentStepIndex = -1;
            var test = app.state.tests.find(function(t) { return t.id === app.state.activeTestId; });
            if (test) test.steps.forEach(function(s) { delete s.status; delete s.error; delete s.screenshot; delete s.duration_ms; });
            app.ui.renderSteps();
            app.log('Reset.', 'info');
        },

        runServerSide: async function() {
            var test = app.state.tests.find(function(t) { return t.id === app.state.activeTestId; });
            if (!test) { app.log('No test selected', 'error'); return; }
            var statusEl = document.getElementById('run-status');
            statusEl.classList.remove('hidden');
            app.log('Running "' + test.name + '" on server...', 'info');
            try {
                var payload = { name: test.name, steps: test.steps.map(function(s) { return Object.assign({ op: s.op }, s.params || {}); }) };
                var res = await app.api.postE2e(payload);
                if (res.status === 'ok' && res.results && res.results.length) {
                    var r = res.results[0];
                    test._result = r;
                    if (r.steps) r.steps.forEach(function(sr, i) {
                        if (test.steps[i]) {
                            test.steps[i].status = sr.status;
                            test.steps[i].error = sr.error;
                            test.steps[i].screenshot = sr.screenshot;
                            test.steps[i].duration_ms = sr.duration_ms;
                            test.steps[i].lastResponse = sr.response;
                        }
                    });
                    app.log('"' + test.name + '": ' + r.status.toUpperCase() + ' (' + r.duration_ms + 'ms, ' + r.steps_passed + '/' + r.step_count + ' passed)', r.status === 'pass' ? 'info' : 'error');
                }
            } catch(e) { app.log('Failed: ' + e.message, 'error'); }
            statusEl.classList.add('hidden');
            app.ui.renderTestList();
            app.ui.renderSteps();
        },

        runAllServerSide: async function() {
            var statusEl = document.getElementById('run-status');
            statusEl.classList.remove('hidden');
            app.log('Running all ' + app.state.tests.length + ' test(s)...', 'info');
            try {
                var payload = app.state.tests.map(function(t) {
                    return { name: t.name, steps: t.steps.map(function(s) { return Object.assign({ op: s.op }, s.params || {}); }) };
                });
                var res = await app.api.postE2e(payload);
                if (res.status === 'ok' && res.results) {
                    var p = 0, f = 0;
                    res.results.forEach(function(r, ti) {
                        if (app.state.tests[ti]) {
                            app.state.tests[ti]._result = r;
                            if (r.steps) r.steps.forEach(function(sr, si) {
                                if (app.state.tests[ti].steps[si]) {
                                    app.state.tests[ti].steps[si].status = sr.status;
                                    app.state.tests[ti].steps[si].error = sr.error;
                                    app.state.tests[ti].steps[si].duration_ms = sr.duration_ms;
                                }
                            });
                        }
                        if (r.status === 'pass') p++; else f++;
                    });
                    app.log('Done: ' + p + ' passed, ' + f + ' failed', f ? 'error' : 'info');
                }
            } catch(e) { app.log('Failed: ' + e.message, 'error'); }
            statusEl.classList.add('hidden');
            app.ui.renderTestList();
            app.ui.renderSteps();
        }
    }
};

/* ================================================================
 * HELPERS (global)
 * ================================================================ */
function esc(s) {
    return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}
function round(n) {
    return Math.round(n * 10) / 10;
}
function _downloadJSON(data, filename) {
    var blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    var url = URL.createObjectURL(blob);
    var a = document.createElement('a');
    a.href = url; a.download = filename; a.click();
    URL.revokeObjectURL(url);
}
function _extractBase64Image(obj) {
    if (!obj || typeof obj !== 'object') return null;
    // data.value.data (screenshot response), data.screenshot, etc.
    if (obj.data && obj.data.value && obj.data.value.data && typeof obj.data.value.data === 'string' && obj.data.value.data.length > 100) return obj.data.value.data;
    if (obj.data && obj.data.screenshot && typeof obj.data.screenshot === 'string') return obj.data.screenshot;
    if (obj.screenshot && typeof obj.screenshot === 'string') return obj.screenshot;
    // Nested: data.value might be the base64 string directly for take_screenshot
    if (obj.data && typeof obj.data === 'string' && obj.data.length > 100) return obj.data;
    return null;
}

/**
 * Parse a slash command with named parameters.
 * Syntax: /cmd key1 value1 key2 value2 ...
 * Falls back to positional args if names don't match schema params.
 */
function _parseSlashCommand(input) {
    // Split: "/cmd arg1 arg2 ..." — but respect quoted strings
    var parts = [];
    var re = /(?:"([^"]*)")|(\S+)/g;
    var m;
    while ((m = re.exec(input)) !== null) {
        parts.push(m[1] !== undefined ? m[1] : m[2]);
    }
    if (!parts.length) return null;
    var cmd = parts[0].replace(/^\//, '');
    var schema = app.schema.commands[cmd];
    if (!schema) return null;

    var payload = { op: cmd };
    var args = parts.slice(1);
    var params = schema.params || [];
    var paramNames = {};
    params.forEach(function(p) { paramNames[p.name] = p; });

    // Try named-parameter parsing: check if args[0] is a known param name
    var isNamed = args.length >= 2 && paramNames[args[0]];

    if (isNamed) {
        // Parse key-value pairs
        var i = 0;
        while (i < args.length) {
            var key = args[i];
            var param = paramNames[key];
            if (param) {
                i++;
                if (i < args.length) {
                    var val = args[i];
                    if (param.type === 'number') {
                        payload[key] = parseFloat(val);
                    } else if (key === 'addresses') {
                        payload[key] = val.split(',');
                    } else if (key === 'state') {
                        try { payload[key] = JSON.parse(args.slice(i).join(' ')); } catch(e) { payload[key] = val; }
                        break;
                    } else if (key === 'classes') {
                        payload[key] = args.slice(i);
                        break;
                    } else {
                        payload[key] = val;
                    }
                    i++;
                }
            } else {
                // Unknown key, skip
                i++;
            }
        }
    } else {
        // Fallback: positional args
        for (var i = 0; i < params.length && i < args.length; i++) {
            var p = params[i];
            var v = args[i];
            if (p.type === 'number') {
                payload[p.name] = parseFloat(v);
            } else if (p.name === 'classes') {
                payload[p.name] = args.slice(i);
                break;
            } else if (p.name === 'addresses') {
                payload[p.name] = v.split(',');
            } else if (p.name === 'state') {
                try { payload[p.name] = JSON.parse(args.slice(i).join(' ')); } catch(e) { payload[p.name] = v; }
                break;
            } else {
                payload[p.name] = v;
            }
        }
    }
    return payload;
}

window.onload = function() { app.init(); };
