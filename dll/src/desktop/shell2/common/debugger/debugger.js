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
        // JSON tree collapsed paths
        jsonCollapsed: new Set(),
        // JSON tree grouped ranges that are collapsed
        jsonGroupCollapsed: new Set(),
    },

    /* ================================================================
     * SCHEMA — every debug API command with params, description, example
     * ================================================================ */
    schema: {
        commands: {
            // ── Queries ──
            'get_state':            { desc: 'Get debug server state',              example: '/get_state',                                params: [] },
            'get_dom':              { desc: 'Get raw DOM structure',                example: '/get_dom',                                  params: [] },
            'get_html_string':      { desc: 'Get DOM as HTML string',              example: '/get_html_string',                          params: [] },
            'get_dom_tree':         { desc: 'Get detailed DOM tree',               example: '/get_dom_tree',                             params: [] },
            'get_node_hierarchy':   { desc: 'Get raw node hierarchy',              example: '/get_node_hierarchy',                       params: [] },
            'get_layout_tree':      { desc: 'Get layout tree (debug)',             example: '/get_layout_tree',                          params: [] },
            'get_display_list':     { desc: 'Get display list items',              example: '/get_display_list',                         params: [] },
            'get_all_nodes_layout': { desc: 'Get all nodes with layout',           example: '/get_all_nodes_layout',                     params: [] },
            'get_logs':             { desc: 'Get server logs',                     example: '/get_logs',                                 params: [{ name: 'since_request_id', type: 'number', placeholder: '0' }] },

            // ── Mouse ──
            'mouse_move':     { desc: 'Move mouse to (x, y)',                      example: '/mouse_move 100 200',                       params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }] },
            'mouse_down':     { desc: 'Mouse button press at (x, y)',              example: '/mouse_down 100 200 left',                  params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }, { name: 'button', type: 'text', placeholder: 'left' }] },
            'mouse_up':       { desc: 'Mouse button release at (x, y)',            example: '/mouse_up 100 200 left',                    params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }, { name: 'button', type: 'text', placeholder: 'left' }] },
            'click':          { desc: 'Click (by selector, text, coords, or ID)',  example: '/click .btn',                               params: [{ name: 'selector', type: 'text', placeholder: '.btn' }, { name: 'text', type: 'text', placeholder: 'Label' }] },
            'double_click':   { desc: 'Double-click at (x, y)',                    example: '/double_click 100 200',                     params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }, { name: 'button', type: 'text', placeholder: 'left' }] },
            'click_node':     { desc: 'Click on node by ID (deprecated)',          example: '/click_node 5',                             params: [{ name: 'node_id', type: 'number', value: 0 }, { name: 'button', type: 'text', placeholder: 'left' }] },
            'scroll':         { desc: 'Scroll at (x, y) by delta',                example: '/scroll 100 200 0 -50',                     params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }, { name: 'delta_x', type: 'number', value: 0 }, { name: 'delta_y', type: 'number', value: 50 }] },
            'hit_test':       { desc: 'Find node at (x, y)',                       example: '/hit_test 100 200',                         params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }] },

            // ── Keyboard ──
            'key_down':       { desc: 'Key press event',                           example: '/key_down Enter',                           params: [{ name: 'key', type: 'text', placeholder: 'Enter' }] },
            'key_up':         { desc: 'Key release event',                         example: '/key_up Enter',                             params: [{ name: 'key', type: 'text', placeholder: 'Enter' }] },
            'text_input':     { desc: 'Type text string',                          example: '/text_input Hello',                         params: [{ name: 'text', type: 'text', placeholder: 'Hello' }] },

            // ── Window ──
            'resize':         { desc: 'Resize window',                             example: '/resize 800 600',                           params: [{ name: 'width', type: 'number', value: 800 }, { name: 'height', type: 'number', value: 600 }] },
            'move':           { desc: 'Move window to (x, y)',                     example: '/move 100 100',                             params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }] },
            'focus':          { desc: 'Focus the window',                          example: '/focus',                                    params: [] },
            'blur':           { desc: 'Blur (unfocus) the window',                 example: '/blur',                                     params: [] },
            'close':          { desc: 'Close the window',                          example: '/close',                                    params: [] },
            'dpi_changed':    { desc: 'Simulate DPI change',                       example: '/dpi_changed 2',                            params: [{ name: 'dpi', type: 'number', value: 1 }] },

            // ── DOM Inspection ──
            'get_node_css_properties': { desc: 'Get computed CSS for a node',      example: '/get_node_css_properties 3',                params: [{ name: 'node_id', type: 'number', placeholder: '0' }, { name: 'selector', type: 'text', placeholder: '.item' }] },
            'get_node_layout':         { desc: 'Get position/size of a node',      example: '/get_node_layout 3',                        params: [{ name: 'node_id', type: 'number', placeholder: '0' }, { name: 'selector', type: 'text', placeholder: '.item' }] },
            'find_node_by_text':       { desc: 'Find node by text content',        example: '/find_node_by_text "Hello"',                params: [{ name: 'text', type: 'text', placeholder: 'Hello' }] },

            // ── Scrolling ──
            'get_scroll_states':    { desc: 'Get all scroll positions',            example: '/get_scroll_states',                        params: [] },
            'get_scrollable_nodes': { desc: 'List scrollable nodes',               example: '/get_scrollable_nodes',                     params: [] },
            'scroll_node_by':       { desc: 'Scroll a node by delta',              example: '/scroll_node_by 5 0 -50',                   params: [{ name: 'node_id', type: 'number', placeholder: '5' }, { name: 'delta_x', type: 'number', value: 0 }, { name: 'delta_y', type: 'number', value: 0 }] },
            'scroll_node_to':       { desc: 'Scroll a node to position',           example: '/scroll_node_to 5 0 100',                   params: [{ name: 'node_id', type: 'number', placeholder: '5' }, { name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }] },
            'scroll_into_view':     { desc: 'Scroll node into view (W3C)',         example: '/scroll_into_view .item',                   params: [{ name: 'selector', type: 'text', placeholder: '.item' }, { name: 'block', type: 'text', placeholder: 'center' }] },
            'get_scrollbar_info':   { desc: 'Get scrollbar geometry for node',     example: '/get_scrollbar_info 5',                     params: [{ name: 'node_id', type: 'number', placeholder: '5' }, { name: 'orientation', type: 'text', placeholder: 'both' }] },

            // ── Selection / Drag ──
            'get_selection_state':    { desc: 'Get text selection state',           example: '/get_selection_state',                      params: [] },
            'dump_selection_manager': { desc: 'Dump selection manager (debug)',     example: '/dump_selection_manager',                   params: [] },
            'get_drag_state':         { desc: 'Get drag state',                    example: '/get_drag_state',                           params: [] },
            'get_drag_context':       { desc: 'Get drag context (debug)',           example: '/get_drag_context',                         params: [] },
            'get_focus_state':        { desc: 'Get current focus node',            example: '/get_focus_state',                          params: [] },
            'get_cursor_state':       { desc: 'Get cursor position/blink',         example: '/get_cursor_state',                         params: [] },

            // ── Control ──
            'relayout':        { desc: 'Force re-layout',                          example: '/relayout',                                 params: [] },
            'redraw':          { desc: 'Force redraw',                             example: '/redraw',                                   params: [] },
            'wait':            { desc: 'Wait milliseconds',                        example: '/wait 500',                                 params: [{ name: 'ms', type: 'number', value: 500 }] },
            'wait_frame':      { desc: 'Wait for next frame',                      example: '/wait_frame',                               params: [] },

            // ── Screenshots ──
            'take_screenshot':        { desc: 'Take screenshot (SW render)',       example: '/take_screenshot',                          params: [] },
            'take_native_screenshot': { desc: 'Take native OS screenshot',         example: '/take_native_screenshot',                   params: [] },

            // ── App State ──
            'get_app_state':   { desc: 'Get global app state as JSON',             example: '/get_app_state',                            params: [] },
            'set_app_state':   { desc: 'Set global app state from JSON',           example: '/set_app_state {"counter":0}',              params: [{ name: 'state', type: 'text', placeholder: '{"counter": 0}' }] },

            // ── DOM Mutation ──
            'insert_node':     { desc: 'Insert child node',                        example: '/insert_node 0 div',                        params: [{ name: 'parent_id', type: 'number', value: 0 }, { name: 'node_type', type: 'text', placeholder: 'div' }, { name: 'position', type: 'number', placeholder: '' }] },
            'delete_node':     { desc: 'Delete a node',                            example: '/delete_node 5',                            params: [{ name: 'node_id', type: 'number', value: 0 }] },
            'set_node_text':   { desc: 'Set text content of a node',               example: '/set_node_text 3 "Hello"',                  params: [{ name: 'node_id', type: 'number', value: 0 }, { name: 'text', type: 'text', placeholder: 'Hello' }] },
            'set_node_classes':{ desc: 'Set CSS classes on a node',                example: '/set_node_classes 3 btn primary',            params: [{ name: 'node_id', type: 'number', value: 0 }, { name: 'classes', type: 'text', placeholder: 'btn primary' }] },
            'set_node_css_override': { desc: 'Override a CSS property on a node',  example: '/set_node_css_override 3 width 100px',       params: [{ name: 'node_id', type: 'number', value: 0 }, { name: 'property', type: 'text', placeholder: 'width' }, { name: 'value', type: 'text', placeholder: '100px' }] },

            // ── Debugging ──
            'resolve_function_pointers': { desc: 'Resolve fn ptrs to symbols',    example: '/resolve_function_pointers 0x1234',          params: [{ name: 'addresses', type: 'text', placeholder: '0x1234,0x5678' }] },
            'get_component_registry':    { desc: 'Get registered components',      example: '/get_component_registry',                   params: [] },

            // ── E2E ──
            'run_e2e_tests':  { desc: 'Run E2E test suite on server',              example: '/run_e2e_tests',                            params: [] },

            // ── Assertions (virtual, used in E2E steps) ──
            'assert_text':       { desc: 'Assert node text equals expected',       example: '/assert_text .label Hello',                 params: [{ name: 'selector', type: 'text', placeholder: '.label' }, { name: 'expected', type: 'text', placeholder: 'Hello' }] },
            'assert_exists':     { desc: 'Assert element exists',                  example: '/assert_exists .element',                   params: [{ name: 'selector', type: 'text', placeholder: '.element' }] },
            'assert_not_exists': { desc: 'Assert element does NOT exist',          example: '/assert_not_exists .gone',                  params: [{ name: 'selector', type: 'text', placeholder: '.gone' }] },
            'assert_node_count': { desc: 'Assert selector matches N nodes',        example: '/assert_node_count li 5',                   params: [{ name: 'selector', type: 'text', placeholder: 'li' }, { name: 'expected', type: 'number', value: 5 }] },
            'assert_layout':     { desc: 'Assert layout property value',           example: '/assert_layout .box width 100 1',           params: [{ name: 'selector', type: 'text', placeholder: '.box' }, { name: 'property', type: 'text', placeholder: 'width' }, { name: 'expected', type: 'number', value: 100 }, { name: 'tolerance', type: 'number', value: 1 }] },
            'assert_app_state':  { desc: 'Assert app state path value',            example: '/assert_app_state counter 42',              params: [{ name: 'path', type: 'text', placeholder: 'counter' }, { name: 'expected', type: 'text', placeholder: '42' }] },
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
        var text = typeof msg === 'object' ? JSON.stringify(msg) : msg;
        div.textContent = '[' + time + '] ' + text;
        panel.appendChild(div);
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
            item.innerHTML =
                '<span class="autocomplete-cmd">/' + esc(cmd) + '</span>' +
                '<span class="autocomplete-desc">' + esc(schema.desc || '') + '</span>' +
                '<span class="autocomplete-example">' + esc(schema.example || '') + '</span>';
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
            if (payload.op === 'get_component_registry') {
                return { status: 'ok', data: { type: 'component_registry', value: { components: [] }}};
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
            var titles = { inspector: 'DOM Explorer', testing: 'Test Explorer', components: 'Components' };
            document.getElementById('sidebar-title').innerText = titles[view] || view;
            var tabTitles = { inspector: 'Inspector', testing: 'runner.e2e', components: 'Components' };
            document.getElementById('tab-title').innerText = tabTitles[view] || view;
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

            // Header
            html += '<div class="detail-section">';
            html += '<div class="detail-section-header">Node #' + node.index + '</div>';
            html += '<div class="detail-row"><span class="detail-key">type</span><span class="detail-value">' + esc(node.type) + '</span></div>';
            if (node.tag) html += '<div class="detail-row"><span class="detail-key">tag</span><span class="detail-value">' + esc(node.tag) + '</span></div>';
            if (node.id) html += '<div class="detail-row"><span class="detail-key">id</span><span class="detail-value">' + esc(node.id) + '</span></div>';
            if (node.classes && node.classes.length) html += '<div class="detail-row"><span class="detail-key">classes</span><span class="detail-value">' + esc(node.classes.join(' ')) + '</span></div>';
            if (node.text) html += '<div class="detail-row"><span class="detail-key">text</span><span class="detail-value">' + esc(node.text) + '</span></div>';
            if (node.tab_index != null) html += '<div class="detail-row"><span class="detail-key">tabindex</span><span class="detail-value">' + node.tab_index + '</span></div>';
            if (node.contenteditable) html += '<div class="detail-row"><span class="detail-key">contenteditable</span><span class="detail-value">true</span></div>';
            html += '</div>';

            // Box Model (Chrome-style) — placeholder, filled by async fetch
            html += '<div id="node-box-model" class="detail-section">';
            html += '<div class="detail-section-header">Layout</div>';
            html += '<div class="placeholder-text">Loading...</div>';
            html += '</div>';

            // Events
            if (node.events && node.events.length) {
                html += '<div class="detail-section">';
                html += '<div class="detail-section-header">Event Handlers (' + node.events.length + ')</div>';
                node.events.forEach(function(ev) {
                    html += '<div class="event-row">';
                    html += '<span class="event-type">' + esc(ev.event) + '</span>';
                    html += '<span class="event-ptr" title="Click to resolve" onclick="app.handlers.resolvePtr(\'' + esc(ev.callback_ptr) + '\')">' + esc(ev.callback_ptr) + '</span>';
                    html += '</div>';
                });
                html += '</div>';
            }

            // Unified CSS Properties section (merged: display + add override)
            html += '<div id="node-css-section" class="detail-section">';
            html += '<div class="detail-section-header">CSS Properties</div>';
            html += '<div class="placeholder-text">Loading...</div>';
            html += '</div>';

            panel.innerHTML = html;

            // Fetch layout (for box model) and CSS properties async
            app._loadNodeBoxModel(node.index);
            app._loadNodeCssProperties(node.index);
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
                div.innerHTML = '<span class="material-icons" style="font-size:16px;color:' + iconColor + '">' + icon + '</span> ' + esc(test.name);
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

                div.innerHTML =
                    '<div class="step-gutter"><div class="breakpoint ' + (step.breakpoint ? 'active' : '') + '" onclick="app.handlers.toggleBreakpoint(' + idx + ', event)"></div></div>' +
                    '<div class="step-content" onclick="app.ui.showStepDetails(' + idx + ')">' +
                    '<div class="step-title">' + esc(step.op) + thumbHtml + '</div>' +
                    '<div class="step-meta">' + (Object.entries(step.params || {}).map(function(e) { return e[0] + '=' + e[1]; }).join(', ') || 'No params') +
                    (step.duration_ms != null ? ' <span>' + step.duration_ms + 'ms</span>' : '') + '</div>' +
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

        showAddStepForm: function() {
            var container = document.getElementById('details-content');
            var ops = '<select id="new-step-op" class="form-control" onchange="app.ui.updateStepParamsForm()">';
            for (var op in app.schema.commands) ops += '<option value="' + op + '">' + op + '</option>';
            ops += '</select>';
            container.innerHTML =
                '<h3 style="margin-bottom:10px">Add Step</h3>' +
                '<div class="form-group"><label class="form-label">Operation</label>' + ops + '</div>' +
                '<div id="step-params-container"></div>' +
                '<button class="btn-primary" onclick="app.handlers.addStepFromForm()">Add Step</button>';
            this.updateStepParamsForm();
        },

        updateStepParamsForm: function() {
            var op = document.getElementById('new-step-op').value;
            var schema = app.schema.commands[op];
            var container = document.getElementById('step-params-container');
            var html = '';
            (schema.params || []).forEach(function(p) {
                html += '<div class="form-group"><label class="form-label">' + p.name + ' (' + p.type + ')</label>' +
                    '<input type="' + (p.type === 'number' ? 'number' : 'text') + '" class="form-control step-param-input" data-name="' + p.name + '" placeholder="' + (p.placeholder || '') + '" value="' + (p.value !== undefined ? p.value : '') + '"></div>';
            });
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
            html += '<h4>Response</h4><div class="mono" style="background:var(--bg-input);padding:8px;border-radius:3px;max-height:250px;overflow:auto;font-size:11px;white-space:pre-wrap;">' + esc(step.lastResponse ? JSON.stringify(step.lastResponse, null, 2) : 'Not executed') + '</div>';
            container.innerHTML = html;
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
                    app.handlers.save();
                    app.ui.renderTestList();
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
            _downloadJSON({
                version: 1, exported_at: new Date().toISOString(),
                tests: app.state.tests, cssOverrides: app.state.cssOverrides,
                apiUrl: app.config.apiUrl,
            }, 'azul-debugger-project.json');
            app.log('Exported project', 'info');
        },

        exportE2eTests: function() {
            var exported = app.state.tests.map(function(t) {
                return { name: t.name, steps: t.steps.map(function(s) { return Object.assign({ op: s.op }, s.params || {}); }) };
            });
            _downloadJSON(exported, 'azul_e2e_tests.json');
            app.log('Exported ' + exported.length + ' E2E test(s)', 'info');
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
            this._renderPropertiesSidebar(node);
        },

        _renderPropertiesSidebar: async function(node) {
            var container = document.getElementById('details-content');
            if (!node) {
                container.innerHTML = '<div class="placeholder-text">No node selected.</div>';
                return;
            }
            var tag = (node.tag || node.type).toLowerCase();
            var html = '<h4 style="margin-bottom:8px">Node #' + node.index + ' ' + esc(tag) + '</h4>';

            if (node.rect) {
                html += '<div style="font-size:11px;color:var(--text-muted);margin-bottom:8px">' +
                    round(node.rect.width) + ' × ' + round(node.rect.height) + ' @ (' + round(node.rect.x) + ', ' + round(node.rect.y) + ')</div>';
            }

            html += '<div style="font-size:11px;margin-bottom:8px">' + (node.children ? node.children.length : 0) + ' children';
            if (node.events && node.events.length) html += ', ' + node.events.length + ' event handler(s)';
            html += '</div>';

            if (node.events && node.events.length) {
                html += '<div class="detail-section"><div class="detail-section-header">Events</div>';
                node.events.forEach(function(ev) {
                    html += '<div class="event-row"><span class="event-type">' + esc(ev.event) + '</span><span class="event-ptr" onclick="app.handlers.resolvePtr(\'' + esc(ev.callback_ptr) + '\')">' + esc(ev.callback_ptr) + '</span></div>';
                });
                html += '</div>';
            }

            container.innerHTML = html;
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
        resolvePtr: async function(addr) {
            try {
                var res = await app.api.post({ op: 'resolve_function_pointers', addresses: [addr] });
                var resolved = (res.data && res.data.value) ? res.data.value : res.data;
                app.log('Resolved ' + addr + ': ' + JSON.stringify(resolved), 'info');
            } catch(e) {
                app.log('Resolve failed: ' + e.message, 'error');
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
                app.json.render('app-state-tree', data);
            } catch(e) {
                document.getElementById('app-state-tree').innerHTML = '<div class="placeholder-text" style="color:var(--error)">Failed to load app state.</div>';
            }
        },

        saveAppState: async function() {
            if (app.state.appStateJson == null) {
                app.log('No app state loaded. Click refresh first.', 'warning');
                return;
            }
            try {
                var res = await app.api.post({ op: 'set_app_state', state: app.state.appStateJson });
                app.log('App state saved: ' + res.status, res.status === 'ok' ? 'info' : 'error');
            } catch(e) {
                app.log('Save app state failed: ' + e.message, 'error');
            }
        },

        /* ── Component Registry ── */
        loadComponents: async function() {
            try {
                var res = await app.api.post({ op: 'get_component_registry' });
                var data = (res.data && res.data.value) ? res.data.value : (res.data || {});
                var container = document.getElementById('component-registry-container');
                var components = data.components || [];
                if (!components.length) {
                    container.innerHTML = '<div class="placeholder-text">No components registered.</div>';
                    return;
                }
                var html = '';
                components.forEach(function(c) {
                    html += '<div class="list-item" onclick="app.handlers.showComponentDetail(' + JSON.stringify(c).replace(/"/g, '&quot;') + ')">';
                    html += '<span class="material-icons" style="font-size:14px">widgets</span> ' + esc(c.name || c.tag || 'Unknown');
                    html += '</div>';
                });
                container.innerHTML = html;
            } catch(e) {
                document.getElementById('component-registry-container').innerHTML = '<div class="placeholder-text" style="color:var(--error)">Failed to load.</div>';
            }
        },

        showComponentDetail: function(component) {
            var panel = document.getElementById('component-detail-panel');
            panel.innerHTML = '<h4>' + esc(component.name || component.tag || 'Component') + '</h4>' +
                '<div class="mono" style="font-size:11px;margin-top:8px;background:var(--bg-input);padding:8px;border-radius:3px;white-space:pre-wrap;">' +
                esc(JSON.stringify(component, null, 2)) + '</div>';
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

        render: function(containerId, data) {
            var container = document.getElementById(containerId);
            if (!container) return;
            container.innerHTML = '';
            if (data == null) {
                container.innerHTML = '<div class="placeholder-text">No data.</div>';
                return;
            }
            var tree = document.createElement('div');
            tree.className = 'json-tree';
            this._buildNode(tree, '', data, 0, '');
            container.appendChild(tree);
        },

        _buildNode: function(container, key, value, depth, path) {
            var self = this;
            var fullPath = path ? (path + '.' + key) : key;

            if (value === null || value === undefined) {
                this._addLeaf(container, key, '<span class="json-null">null</span>', depth);
            } else if (typeof value === 'boolean') {
                this._addLeaf(container, key, '<span class="json-bool">' + value + '</span>', depth);
            } else if (typeof value === 'number') {
                this._addLeaf(container, key, '<span class="json-number">' + value + '</span>', depth);
            } else if (typeof value === 'string') {
                this._addLeaf(container, key, '<span class="json-string">"' + esc(value) + '"</span>', depth);
            } else if (Array.isArray(value)) {
                this._addCollapsible(container, key, value, depth, fullPath, true);
            } else if (typeof value === 'object') {
                this._addCollapsible(container, key, value, depth, fullPath, false);
            }
        },

        _addLeaf: function(container, key, valueHtml, depth) {
            var row = document.createElement('div');
            row.className = 'json-row';
            row.style.paddingLeft = (depth * 14 + 8) + 'px';
            var keyHtml = key !== '' ? '<span class="json-key">' + esc(key) + '</span>: ' : '';
            row.innerHTML = '<span class="json-toggle-icon">&nbsp;</span>' + keyHtml + valueHtml;
            container.appendChild(row);
        },

        _addCollapsible: function(container, key, value, depth, path, isArray) {
            var self = this;
            var isCollapsed = app.state.jsonCollapsed.has(path);
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
                if (app.state.jsonCollapsed.has(path)) {
                    app.state.jsonCollapsed.delete(path);
                } else {
                    app.state.jsonCollapsed.add(path);
                }
                self.render('app-state-tree', app.state.appStateJson);
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
                        var groupCollapsed = app.state.jsonGroupCollapsed.has(groupPath);

                        var groupRow = document.createElement('div');
                        groupRow.className = 'json-row';
                        groupRow.style.paddingLeft = ((depth + 1) * 14 + 8) + 'px';

                        var gToggle = document.createElement('span');
                        gToggle.className = 'json-toggle-icon';
                        gToggle.textContent = groupCollapsed ? '▶' : '▼';
                        (function(gp) {
                            gToggle.addEventListener('click', function() {
                                if (app.state.jsonGroupCollapsed.has(gp)) {
                                    app.state.jsonGroupCollapsed.delete(gp);
                                } else {
                                    app.state.jsonGroupCollapsed.add(gp);
                                }
                                self.render('app-state-tree', app.state.appStateJson);
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
                                this._buildNode(childContainer, String(i), value[i], depth + 2, path);
                            }
                        }
                    }
                } else if (isArray) {
                    for (var i = 0; i < count; i++) {
                        this._buildNode(childContainer, String(i), value[i], depth + 1, path);
                    }
                } else {
                    var keys = Object.keys(value);
                    for (var k = 0; k < keys.length; k++) {
                        this._buildNode(childContainer, keys[k], value[keys[k]], depth + 1, path);
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
                var res = await app.api.post({ op: step.op, ...step.params });
                step.lastResponse = res;
                step.duration_ms = Math.round(performance.now() - t0);
                if (res.status === 'error') throw new Error(res.message);
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

/**
 * Parse a slash command like "/click .btn" into a JSON payload.
 * Maps positional args to the schema params in order.
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

    for (var i = 0; i < params.length && i < args.length; i++) {
        var p = params[i];
        var v = args[i];
        if (p.type === 'number') {
            payload[p.name] = parseFloat(v);
        } else if (p.name === 'classes') {
            // Collect remaining args as array
            payload[p.name] = args.slice(i);
            break;
        } else if (p.name === 'addresses') {
            payload[p.name] = v.split(',');
        } else if (p.name === 'state') {
            // Try to parse as JSON, else use as string
            try { payload[p.name] = JSON.parse(args.slice(i).join(' ')); } catch(e) { payload[p.name] = v; }
            break;
        } else {
            payload[p.name] = v;
        }
    }
    return payload;
}

window.onload = function() { app.init(); };
