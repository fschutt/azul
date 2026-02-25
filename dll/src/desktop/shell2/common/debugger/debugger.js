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
            'create_library':            { desc: 'Create new component library',    examples: ['/create_library name mylib'],                    params: [{ name: 'name', type: 'text', placeholder: 'mylib' }] },
            'delete_library':            { desc: 'Delete a component library',      examples: ['/delete_library name mylib'],                    params: [{ name: 'name', type: 'text', placeholder: 'mylib' }] },
            'create_component':          { desc: 'Create component in library',     examples: ['/create_component library mylib name mycomp'],   params: [{ name: 'library', type: 'text', placeholder: 'mylib' }, { name: 'name', type: 'text', placeholder: 'mycomp' }] },
            'delete_component':          { desc: 'Delete component from library',   examples: ['/delete_component library mylib name mycomp'],   params: [{ name: 'library', type: 'text', placeholder: 'mylib' }, { name: 'name', type: 'text', placeholder: 'mycomp' }] },
            'update_component':          { desc: 'Update component properties',     examples: ['/update_component library mylib name mycomp'],   params: [{ name: 'library', type: 'text', placeholder: 'mylib' }, { name: 'name', type: 'text', placeholder: 'mycomp' }] },

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
    _showAutocomplete: function(filter) {
        var existing = document.getElementById('autocomplete-popup');
        if (existing) existing.remove();

        var cmdNames = Object.keys(this.schema.commands);
        var matches = filter
            ? cmdNames.filter(function(c) { return c.indexOf(filter) !== -1; })
            : cmdNames;
        if (!matches.length) return;

        var menu = document.createElement('div');
        menu.id = 'autocomplete-popup';
        menu.className = 'autocomplete-menu';

        var self = this;
        matches.forEach(function(cmd) {
            var schema = self.schema.commands[cmd];
            var item = document.createElement('div');
            item.className = 'autocomplete-item';
            var examplesArr = schema.examples || (schema.example ? [schema.example] : []);
            var firstExample = examplesArr[0] || '';
            var extraExamples = examplesArr.slice(1);

            var html = '<div class="autocomplete-main">' +
                '<span class="autocomplete-cmd">/' + esc(cmd) + '</span>' +
                '<span class="autocomplete-desc">' + esc(schema.desc || '') + '</span>' +
                '</div>';
            // First example on main line
            if (firstExample) {
                html += '<div class="autocomplete-example">' + esc(firstExample) + '</div>';
            }
            // Additional variant examples
            if (extraExamples.length) {
                html += '<div class="autocomplete-variants">';
                extraExamples.forEach(function(ex) {
                    html += '<div class="autocomplete-variant">' + esc(ex) + '</div>';
                });
                html += '</div>';
            }
            item.innerHTML = html;
            item.addEventListener('mousedown', function(e) {
                e.preventDefault();
                var input = document.getElementById('terminal-cmd');
                input.value = '/' + cmd + ' ';
                input.focus();
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
            this._renderTreeNode(container, rootNode, byIndex, 0);
        },

        _renderTreeNode: function(container, node, byIndex, depth) {
            if (!node) return;
            var hasChildren = node.children && node.children.length > 0;
            var isCollapsed = app.state.collapsedNodes.has(node.index);
            var isSelected = app.state.selectedNodeId === node.index;

            var row = document.createElement('div');
            row.className = 'tree-row' + (isSelected ? ' selected' : '');
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
                node.children.forEach(function(childIdx) {
                    self._renderTreeNode(container, byIndex[childIdx], byIndex, depth + 1);
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

            // Top section: Node info + Box Model side by side
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

            // Right: Box Model (Chrome-style) — placeholder, filled by async fetch
            html += '<div id="node-box-model" class="detail-box-model-col">';
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

            // Fetch layout (for box model) and CSS properties async
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
                var res = await app.api.post({ op: 'export_code', language: language });
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
                        // Multiple files: download as JSON bundle
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
            if (app.state.datasetJson != null) {
                panel.style.display = '';
                app.json.render('dataset-tree', app.state.datasetJson, true);
            } else {
                panel.style.display = 'none';
                document.getElementById('dataset-tree').innerHTML = '<div class="placeholder-text">No dataset.</div>';
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
                html += '<div class="list-item" onclick="app.handlers.showComponentDetail(' + origIdx + ')">';
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
                    },
                    onSave: isEditable ? function(cssText) {
                        app.handlers._saveComponentCss(cssText);
                    } : null
                });
                cssDetails.appendChild(cssEditor);
                leftPanel.appendChild(cssDetails);
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

            // === Right column: Preview (widget-based) ===
            if (rightPanel) {
                rightPanel.style.display = 'block';
                rightPanel.innerHTML = '';

                // Preview section (widget-based)
                var previewH4 = document.createElement('h4');
                previewH4.style.margin = '12px 0 8px 0';
                previewH4.textContent = 'Preview';
                rightPanel.appendChild(previewH4);

                var previewEl = app.widgets.PreviewPanel.render({}, { loading: true }, {
                    onRefresh: function() {
                        app.handlers._loadComponentPreview(component, previewEl);
                    }
                });
                previewEl.id = 'component-preview-widget';
                rightPanel.appendChild(previewEl);

                // Load the actual preview image
                app.handlers._loadComponentPreview(component, previewEl);
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
            if (!previewEl) return;
            try {
                var payload = {
                    op: 'get_component_preview',
                    library: app.state.selectedLibrary || 'builtin',
                    name: component.tag,
                };
                var res = await app.api.post(payload);
                if (res.status === 'ok' && res.data && res.data.value) {
                    var preview = res.data.value;
                    app.widgets.PreviewPanel.update(previewEl, {
                        imageDataUri: preview.data,
                        width: preview.width,
                        height: preview.height,
                        loading: false
                    });
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

        /* ── Terminal ── */
        terminalInput: function(input) {
            var val = input.value;
            if (val.startsWith('/')) {
                var filter = val.substring(1).split(' ')[0];
                app._showAutocomplete(filter);
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
