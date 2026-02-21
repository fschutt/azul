/**
 * Azul Debugger — DOM inspector, CSS editor, and E2E test runner.
 */
const app = {
    config: {
        apiUrl: window.location.origin || 'http://localhost:8765',
        isMock: false,
    },
    state: {
        currentView: 'inspector',   // 'inspector' | 'testing'
        activePanel: 'terminal',    // 'terminal' | 'debug'
        activeTestId: null,
        tests: [],
        executionStatus: 'idle',
        currentStepIndex: -1,
        // DOM tree
        hierarchy: null,            // array from get_node_hierarchy
        hierarchyRoot: -1,
        selectedNodeId: null,
        collapsedNodes: new Set(),
        contextMenuNodeId: null,
        // CSS overrides (node_id -> { prop: value })
        cssOverrides: {},
        // Open menu
        openMenu: null,
    },

    schema: {
        commands: {
            'get_state':       { params: [] },
            'click':           { params: [{ name: 'selector', type: 'text', placeholder: '.btn' }, { name: 'text', type: 'text', placeholder: 'Label' }] },
            'text_input':      { params: [{ name: 'text', type: 'text', placeholder: 'Hello' }] },
            'key_down':        { params: [{ name: 'key', type: 'text', placeholder: 'Enter' }] },
            'key_up':          { params: [{ name: 'key', type: 'text', placeholder: 'Enter' }] },
            'mouse_move':      { params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }] },
            'mouse_down':      { params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }, { name: 'button', type: 'text', placeholder: 'left' }] },
            'mouse_up':        { params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }, { name: 'button', type: 'text', placeholder: 'left' }] },
            'scroll':          { params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }, { name: 'delta_x', type: 'number', value: 0 }, { name: 'delta_y', type: 'number', value: 50 }] },
            'resize':          { params: [{ name: 'width', type: 'number', value: 800 }, { name: 'height', type: 'number', value: 600 }] },
            'focus':           { params: [] },
            'blur':            { params: [] },
            'close':           { params: [] },
            'wait':            { params: [{ name: 'ms', type: 'number', value: 500 }] },
            'wait_frame':      { params: [] },
            'relayout':        { params: [] },
            'take_screenshot': { params: [] },
            'get_html_string': { params: [] },
            'get_dom_tree':    { params: [] },
            'get_display_list':{ params: [] },
            'get_app_state':   { params: [] },
            'set_app_state':   { params: [{ name: 'state', type: 'text', placeholder: '{"counter": 0}' }] },
            'assert_text':       { params: [{ name: 'selector', type: 'text', placeholder: '.label' }, { name: 'expected', type: 'text', placeholder: 'Hello' }] },
            'assert_exists':     { params: [{ name: 'selector', type: 'text', placeholder: '.element' }] },
            'assert_not_exists': { params: [{ name: 'selector', type: 'text', placeholder: '.gone' }] },
            'assert_node_count': { params: [{ name: 'selector', type: 'text', placeholder: 'li' }, { name: 'expected', type: 'number', value: 5 }] },
            'assert_layout':     { params: [{ name: 'selector', type: 'text', placeholder: '.box' }, { name: 'property', type: 'text', placeholder: 'width' }, { name: 'expected', type: 'number', value: 100 }, { name: 'tolerance', type: 'number', value: 1 }] },
            'assert_app_state':  { params: [{ name: 'path', type: 'text', placeholder: 'counter' }, { name: 'expected', type: 'text', placeholder: '42' }] },
        }
    },

    // ──────────── Init ────────────
    init: async function() {
        console.log('[dbg] init, apiUrl =', this.config.apiUrl);
        if (window.location.port) this.config.apiUrl = window.location.origin;

        // Load saved state
        const saved = localStorage.getItem('azul_debugger');
        if (saved) {
            try {
                const s = JSON.parse(saved);
                if (s.tests) this.state.tests = s.tests;
                if (s.cssOverrides) this.state.cssOverrides = s.cssOverrides;
            } catch(e) { console.warn('[dbg] bad localStorage:', e); }
        }
        if (!this.state.tests.length) this.handlers.newTest();

        // Menu click-to-open logic
        this._initMenubar();

        // Global click to close context menu
        document.addEventListener('click', () => {
            document.getElementById('context-menu').classList.add('hidden');
        });

        // Connect
        try {
            const r = await this.api.post({ op: 'get_state' });
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
    },

    _initMenubar: function() {
        const menubar = document.getElementById('menubar');
        const items = menubar.querySelectorAll('.menu-item[data-menu]');
        items.forEach(mi => {
            mi.addEventListener('click', (e) => {
                e.stopPropagation();
                const wasOpen = mi.classList.contains('open');
                // Close all
                items.forEach(x => x.classList.remove('open'));
                if (!wasOpen) {
                    mi.classList.add('open');
                    app.state.openMenu = mi.dataset.menu;
                } else {
                    app.state.openMenu = null;
                }
            });
            // While a menu is open, hovering another item switches to it
            mi.addEventListener('mouseenter', () => {
                if (app.state.openMenu && app.state.openMenu !== mi.dataset.menu) {
                    items.forEach(x => x.classList.remove('open'));
                    mi.classList.add('open');
                    app.state.openMenu = mi.dataset.menu;
                }
            });
        });
        // Click outside closes menu
        document.addEventListener('click', () => {
            items.forEach(x => x.classList.remove('open'));
            app.state.openMenu = null;
        });
        // Clicks inside dropdown items should close menu after action
        menubar.querySelectorAll('.menu-dropdown-item').forEach(di => {
            di.addEventListener('click', (e) => {
                e.stopPropagation();
                items.forEach(x => x.classList.remove('open'));
                app.state.openMenu = null;
                // The onclick on the element will fire normally
            });
        });
    },

    // ──────────── Logging ────────────
    log: function(msg, type = 'info') {
        const panel = document.getElementById('panel-terminal');
        this._appendLog(panel, msg, type);
    },
    debugLog: function(msg, type = 'info') {
        const panel = document.getElementById('panel-debug');
        this._appendLog(panel, msg, type);
    },
    _appendLog: function(panel, msg, type) {
        const div = document.createElement('div');
        div.className = 'log-entry ' + type;
        const time = new Date().toLocaleTimeString('en', { hour12: false });
        const text = typeof msg === 'object' ? JSON.stringify(msg) : msg;
        div.textContent = '[' + time + '] ' + text;
        panel.appendChild(div);
        panel.scrollTop = panel.scrollHeight;
    },

    // ──────────── API ────────────
    api: {
        post: async function(payload) {
            if (app.config.isMock) return app.api.mockResponse(payload);
            const t0 = performance.now();
            app.debugLog(payload, 'request');
            const res = await fetch(app.config.apiUrl, { method: 'POST', body: JSON.stringify(payload) });
            const json = await res.json();
            const ms = Math.round(performance.now() - t0);
            app.debugLog('[' + ms + 'ms] ' + JSON.stringify(json).substring(0, 400), 'response');
            return json;
        },

        postE2e: async function(tests) {
            const testArr = Array.isArray(tests) ? tests : [tests];
            if (app.config.isMock) return app.api.mockE2eResponse(tests);
            const payload = { op: 'run_e2e_tests', tests: testArr, timeout_secs: 300 };
            app.debugLog(payload, 'request');
            const res = await fetch(app.config.apiUrl, { method: 'POST', body: JSON.stringify(payload) });
            const json = await res.json();
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
            await new Promise(r => setTimeout(r, 50));
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
                return { status: 'ok', data: { type: 'node_layout', value: { node_id: payload.node_id, size: { width: 100, height: 50 }, position: { x: 0, y: 0 }, rect: { x: 0, y: 0, width: 100, height: 50 }}}};
            }
            return { status: 'ok', data: {} };
        },

        mockE2eResponse: async function(tests) {
            await new Promise(r => setTimeout(r, 300));
            const results = (Array.isArray(tests) ? tests : [tests]).map(t => ({
                name: t.name || 'Test', status: 'pass', duration_ms: 42, step_count: (t.steps||[]).length,
                steps_passed: (t.steps||[]).length, steps_failed: 0,
                steps: (t.steps||[]).map((s, i) => ({ step_index: i, op: s.op, status: 'pass', duration_ms: 5, logs: [], error: null, response: null }))
            }));
            return { status: 'ok', results };
        }
    },

    // ──────────── UI ────────────
    ui: {
        switchView: function(view) {
            app.state.currentView = view;
            document.querySelectorAll('.activity-icon').forEach(el => el.classList.toggle('active', el.dataset.view === view));
            document.getElementById('sidebar-inspector').classList.toggle('hidden', view !== 'inspector');
            document.getElementById('sidebar-testing').classList.toggle('hidden', view !== 'testing');
            document.getElementById('view-inspector').classList.toggle('hidden', view !== 'inspector');
            document.getElementById('view-testing').classList.toggle('hidden', view !== 'testing');
            document.getElementById('sidebar-title').innerText = view === 'inspector' ? 'DOM Explorer' : 'Test Explorer';
            document.getElementById('tab-title').innerText = view === 'inspector' ? 'Inspector' : 'runner.e2e';
        },

        switchPanel: function(panel) {
            app.state.activePanel = panel;
            document.querySelectorAll('.panel-tab').forEach(el => el.classList.toggle('active', el.dataset.panel === panel));
            document.getElementById('panel-terminal').classList.toggle('hidden', panel !== 'terminal');
            document.getElementById('panel-debug').classList.toggle('hidden', panel !== 'debug');
        },

        // ── DOM Tree rendering ──
        renderDomTree: function(hierarchy, root) {
            const container = document.getElementById('dom-tree-container');
            container.innerHTML = '';
            if (!hierarchy || !hierarchy.length) {
                container.innerHTML = '<div class="placeholder-text">No DOM data.</div>';
                return;
            }
            app.state.hierarchy = hierarchy;
            app.state.hierarchyRoot = root;

            // Build a quick lookup
            const byIndex = {};
            hierarchy.forEach(n => { byIndex[n.index] = n; });

            // Render recursively
            const rootNode = byIndex[root] || hierarchy[0];
            this._renderTreeNode(container, rootNode, byIndex, 0);
        },

        _renderTreeNode: function(container, node, byIndex, depth) {
            if (!node) return;
            const hasChildren = node.children && node.children.length > 0;
            const isCollapsed = app.state.collapsedNodes.has(node.index);
            const isSelected = app.state.selectedNodeId === node.index;

            const row = document.createElement('div');
            row.className = 'tree-row' + (isSelected ? ' selected' : '');
            row.dataset.nodeId = node.index;
            row.dataset.type = node.type === 'Text' ? 'text' : 'element';

            // Indent
            const indent = document.createElement('span');
            indent.className = 'tree-indent';
            indent.style.width = (depth * 16 + 4) + 'px';
            row.appendChild(indent);

            // Toggle
            const toggle = document.createElement('span');
            toggle.className = 'tree-toggle';
            if (hasChildren) {
                toggle.textContent = isCollapsed ? '▶' : '▼';
                toggle.addEventListener('click', (e) => {
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

            // Label
            const label = document.createElement('span');
            label.className = 'tree-label';

            if (node.type === 'Text') {
                const textSpan = document.createElement('span');
                textSpan.className = 'tree-text-content';
                textSpan.textContent = '"' + (node.text || '') + '"';
                label.appendChild(textSpan);
            } else {
                // Tag name
                const tagSpan = document.createElement('span');
                tagSpan.className = 'tree-tag';
                tagSpan.textContent = '<' + (node.tag || node.type).toLowerCase();
                label.appendChild(tagSpan);

                // ID
                if (node.id) {
                    const idSpan = document.createElement('span');
                    idSpan.className = 'tree-id';
                    idSpan.textContent = ' #' + node.id;
                    label.appendChild(idSpan);
                }

                // Classes
                if (node.classes && node.classes.length) {
                    const clsSpan = document.createElement('span');
                    clsSpan.className = 'tree-class';
                    clsSpan.textContent = ' .' + node.classes.join('.');
                    label.appendChild(clsSpan);
                }

                const closeTag = document.createElement('span');
                closeTag.className = 'tree-tag';
                closeTag.textContent = '>';
                label.appendChild(closeTag);
            }

            // Event badges
            if (node.events && node.events.length) {
                const badge = document.createElement('span');
                badge.className = 'tree-event-badge';
                badge.textContent = '⚡' + node.events.length;
                badge.title = node.events.map(e => e.event).join(', ');
                label.appendChild(badge);
            }

            row.appendChild(label);

            // Click to select
            row.addEventListener('click', (e) => {
                e.stopPropagation();
                app.handlers.nodeSelected(node.index);
            });

            // Right-click context menu
            row.addEventListener('contextmenu', (e) => {
                e.preventDefault();
                e.stopPropagation();
                app.state.contextMenuNodeId = node.index;
                const menu = document.getElementById('context-menu');
                menu.classList.remove('hidden');
                menu.style.left = e.clientX + 'px';
                menu.style.top = e.clientY + 'px';
            });

            container.appendChild(row);

            // Children
            if (hasChildren && !isCollapsed) {
                node.children.forEach(childIdx => {
                    this._renderTreeNode(container, byIndex[childIdx], byIndex, depth + 1);
                });
            }
        },

        // ── Node detail panel (main editor area) ──
        renderNodeDetail: function(node) {
            const panel = document.getElementById('node-detail-panel');
            if (!node) {
                panel.innerHTML = '<div class="placeholder-text">Select a node in the DOM Explorer to inspect it.</div>';
                return;
            }

            let html = '';

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

            // Layout
            if (node.rect) {
                html += '<div class="detail-section">';
                html += '<div class="detail-section-header">Layout</div>';
                html += '<div class="detail-row"><span class="detail-key">x</span><span class="detail-value">' + round(node.rect.x) + 'px</span></div>';
                html += '<div class="detail-row"><span class="detail-key">y</span><span class="detail-value">' + round(node.rect.y) + 'px</span></div>';
                html += '<div class="detail-row"><span class="detail-key">width</span><span class="detail-value">' + round(node.rect.width) + 'px</span></div>';
                html += '<div class="detail-row"><span class="detail-key">height</span><span class="detail-value">' + round(node.rect.height) + 'px</span></div>';
                html += '</div>';
            }

            // Events
            if (node.events && node.events.length) {
                html += '<div class="detail-section">';
                html += '<div class="detail-section-header">Event Handlers (' + node.events.length + ')</div>';
                node.events.forEach(ev => {
                    html += '<div class="event-row">';
                    html += '<span class="event-type">' + esc(ev.event) + '</span>';
                    html += '<span class="event-ptr" title="Function pointer address">' + esc(ev.callback_ptr) + '</span>';
                    html += '</div>';
                });
                html += '</div>';
            }

            // CSS properties placeholder (will be filled async)
            html += '<div id="node-css-section" class="detail-section">';
            html += '<div class="detail-section-header">CSS Properties</div>';
            html += '<div class="placeholder-text">Loading...</div>';
            html += '</div>';

            panel.innerHTML = html;

            // Fetch CSS properties async
            app._loadNodeCssProperties(node.index);
        },

        renderTestList: function() {
            const container = document.getElementById('test-list-container');
            container.innerHTML = '';
            app.state.tests.forEach(test => {
                const div = document.createElement('div');
                div.className = 'list-item' + (app.state.activeTestId === test.id ? ' selected' : '');
                div.onclick = () => app.handlers.selectTest(test.id);
                const icon = test._result ? (test._result.status === 'pass' ? 'check_circle' : 'cancel') : 'description';
                const iconColor = test._result ? (test._result.status === 'pass' ? 'var(--success)' : 'var(--error)') : 'inherit';
                div.innerHTML = '<span class="material-icons" style="font-size:16px;color:' + iconColor + '">' + icon + '</span> ' + esc(test.name);
                container.appendChild(div);
            });
        },

        renderSteps: function() {
            const container = document.getElementById('steps-container');
            container.innerHTML = '';
            const activeTest = app.state.tests.find(t => t.id === app.state.activeTestId);
            if (!activeTest) return;

            activeTest.steps.forEach((step, idx) => {
                const div = document.createElement('div');
                let cls = '';
                if (step.status === 'pass') cls = 'pass';
                if (step.status === 'fail') cls = 'fail';
                if (app.state.currentStepIndex === idx) cls = 'active';
                div.className = 'step-item ' + cls;

                let thumbHtml = step.screenshot ? '<img class="screenshot-thumb" src="' + step.screenshot + '" style="max-width:40px;max-height:24px;margin-left:5px;vertical-align:middle;cursor:pointer" onclick="app.ui.showScreenshot(this.src)">' : '';

                div.innerHTML =
                    '<div class="step-gutter"><div class="breakpoint ' + (step.breakpoint ? 'active' : '') + '" onclick="app.handlers.toggleBreakpoint(' + idx + ', event)"></div></div>' +
                    '<div class="step-content" onclick="app.ui.showStepDetails(' + idx + ')">' +
                    '<div class="step-title">' + esc(step.op) + thumbHtml + '</div>' +
                    '<div class="step-meta">' + (Object.entries(step.params || {}).map(([k,v]) => k + '=' + v).join(', ') || 'No params') +
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
            const container = document.getElementById('details-content');
            let ops = '<select id="new-step-op" class="form-control" onchange="app.ui.updateStepParamsForm()">';
            for (let op in app.schema.commands) ops += '<option value="' + op + '">' + op + '</option>';
            ops += '</select>';
            container.innerHTML =
                '<h3 style="margin-bottom:10px">Add Step</h3>' +
                '<div class="form-group"><label class="form-label">Operation</label>' + ops + '</div>' +
                '<div id="step-params-container"></div>' +
                '<button class="btn-primary" onclick="app.handlers.addStepFromForm()">Add Step</button>';
            this.updateStepParamsForm();
        },

        updateStepParamsForm: function() {
            const op = document.getElementById('new-step-op').value;
            const schema = app.schema.commands[op];
            const container = document.getElementById('step-params-container');
            let html = '';
            (schema.params || []).forEach(p => {
                html += '<div class="form-group"><label class="form-label">' + p.name + ' (' + p.type + ')</label>' +
                    '<input type="' + (p.type === 'number' ? 'number' : 'text') + '" class="form-control step-param-input" data-name="' + p.name + '" placeholder="' + (p.placeholder || '') + '" value="' + (p.value !== undefined ? p.value : '') + '"></div>';
            });
            container.innerHTML = html;
        },

        showStepDetails: function(idx) {
            const test = app.state.tests.find(t => t.id === app.state.activeTestId);
            if (!test) return;
            const step = test.steps[idx];
            const container = document.getElementById('details-content');
            let html = '<h3>Step ' + (idx + 1) + ': ' + esc(step.op) + '</h3>';
            html += '<div class="mono" style="font-size:11px;margin:10px 0;background:var(--bg-input);padding:8px;border-radius:3px;white-space:pre-wrap;">' + esc(JSON.stringify(step.params || {}, null, 2)) + '</div>';
            if (step.status) html += '<div style="margin-bottom:10px"><strong>Status:</strong> <span style="color:' + (step.status === 'pass' ? 'var(--success)' : 'var(--error)') + '">' + step.status.toUpperCase() + '</span></div>';
            if (step.error) html += '<div style="color:var(--error);margin-bottom:10px">' + esc(step.error) + '</div>';
            if (step.screenshot) html += '<div style="margin:10px 0"><img src="' + step.screenshot + '" style="max-width:100%;cursor:pointer;border:1px solid var(--border)" onclick="app.ui.showScreenshot(this.src)"></div>';
            html += '<h4>Response</h4><div class="mono" style="background:var(--bg-input);padding:8px;border-radius:3px;max-height:250px;overflow:auto;font-size:11px;white-space:pre-wrap;">' + esc(step.lastResponse ? JSON.stringify(step.lastResponse, null, 2) : 'Not executed') + '</div>';
            container.innerHTML = html;
        },
    },

    // Helper: load CSS properties for a node and render into the detail panel
    _loadNodeCssProperties: async function(nodeId) {
        const section = document.getElementById('node-css-section');
        if (!section) return;
        try {
            const res = await this.api.post({ op: 'get_node_css_properties', node_id: nodeId });
            let props = [];
            if (res.data) {
                const d = res.data.value || res.data;
                props = d.properties || [];
            }
            const overrides = this.state.cssOverrides[nodeId] || {};

            let html = '<div class="detail-section-header">CSS Properties (' + props.length + ')</div>';
            if (props.length === 0) {
                html += '<div class="placeholder-text">No CSS properties set.</div>';
            } else {
                props.forEach(propStr => {
                    const colonIdx = propStr.indexOf(':');
                    const name = colonIdx > 0 ? propStr.substring(0, colonIdx).trim() : propStr;
                    const value = colonIdx > 0 ? propStr.substring(colonIdx + 1).trim() : '';
                    const isOverridden = overrides[name] !== undefined;
                    const displayValue = isOverridden ? overrides[name] : value;
                    html += '<div class="css-prop-row">';
                    html += '<span class="css-prop-name">' + esc(name) + '</span>';
                    html += '<span class="css-prop-value' + (isOverridden ? ' overridden' : '') + '" title="Click to edit" onclick="app.handlers.editCssProp(' + nodeId + ', \'' + esc(name) + '\', this)">' + esc(displayValue) + '</span>';
                    html += '</div>';
                });
            }
            section.innerHTML = html;
        } catch(e) {
            section.innerHTML = '<div class="detail-section-header">CSS Properties</div><div class="placeholder-text" style="color:var(--error)">Failed to load</div>';
        }
    },

    // ──────────── Handlers ────────────
    handlers: {
        // ── Import / Export ──
        importProject: function() {
            document.getElementById('file-import-project').click();
        },
        importE2eTests: function() {
            document.getElementById('file-import-e2e').click();
        },
        handleProjectImport: function(input) {
            const file = input.files[0];
            if (!file) return;
            const reader = new FileReader();
            reader.onload = function(e) {
                try {
                    const project = JSON.parse(e.target.result);
                    if (!confirm('Replace current project with imported data?\n\nThis will overwrite tests and CSS overrides.')) return;
                    if (project.tests) app.state.tests = project.tests;
                    if (project.cssOverrides) app.state.cssOverrides = project.cssOverrides;
                    app.handlers.save();
                    app.ui.renderTestList();
                    app.log('Imported project from ' + file.name, 'info');
                } catch(err) {
                    alert('Invalid project JSON: ' + err.message);
                }
            };
            reader.readAsText(file);
            input.value = '';
        },
        handleE2eImport: function(input) {
            const file = input.files[0];
            if (!file) return;
            const reader = new FileReader();
            reader.onload = function(e) {
                try {
                    const imported = JSON.parse(e.target.result);
                    const arr = Array.isArray(imported) ? imported : [imported];
                    arr.forEach(t => {
                        app.state.tests.push({
                            id: Date.now() + Math.random(),
                            name: t.name || 'Imported Test',
                            steps: (t.steps || []).map(s => {
                                const { op, ...params } = s;
                                return { op, params, breakpoint: false };
                            })
                        });
                    });
                    app.handlers.save();
                    app.ui.renderTestList();
                    app.log('Appended ' + arr.length + ' E2E test(s) from ' + file.name, 'info');
                } catch(err) {
                    alert('Invalid test JSON: ' + err.message);
                }
            };
            reader.readAsText(file);
            input.value = '';
        },
        exportProject: function() {
            const project = {
                version: 1,
                exported_at: new Date().toISOString(),
                tests: app.state.tests,
                cssOverrides: app.state.cssOverrides,
                apiUrl: app.config.apiUrl,
            };
            _downloadJSON(project, 'azul-debugger-project.json');
            app.log('Exported project', 'info');
        },
        exportE2eTests: function() {
            // CLI runner format: array of { name, steps: [{ op, ...params }] }
            const exported = app.state.tests.map(t => ({
                name: t.name,
                steps: t.steps.map(s => ({ op: s.op, ...(s.params || {}) }))
            }));
            _downloadJSON(exported, 'azul_e2e_tests.json');
            app.log('Exported ' + exported.length + ' E2E test(s)', 'info');
        },

        // ── Test management ──
        newTest: function() {
            const t = { id: Date.now(), name: 'Test ' + (app.state.tests.length + 1), steps: [{ op: 'get_state', params: {}, breakpoint: false }] };
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
            const op = document.getElementById('new-step-op').value;
            const inputs = document.querySelectorAll('.step-param-input');
            const params = {};
            inputs.forEach(inp => { if (inp.value !== '') params[inp.dataset.name] = inp.type === 'number' ? parseFloat(inp.value) : inp.value; });
            const test = app.state.tests.find(t => t.id === app.state.activeTestId);
            test.steps.push({ op, params, breakpoint: false });
            this.save();
            app.ui.renderSteps();
        },
        deleteStep: function(idx, e) {
            e.stopPropagation();
            const test = app.state.tests.find(t => t.id === app.state.activeTestId);
            test.steps.splice(idx, 1);
            this.save();
            app.ui.renderSteps();
        },
        toggleBreakpoint: function(idx, e) {
            e.stopPropagation();
            const test = app.state.tests.find(t => t.id === app.state.activeTestId);
            test.steps[idx].breakpoint = !test.steps[idx].breakpoint;
            this.save();
            app.ui.renderSteps();
        },

        // ── DOM tree ──
        refreshSidebar: async function() {
            try {
                const res = await app.api.post({ op: 'get_node_hierarchy' });
                let data = null;
                if (res.data) {
                    data = res.data.value || res.data;
                }
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
            // Re-render tree to update selection highlight
            if (app.state.hierarchy) {
                app.ui.renderDomTree(app.state.hierarchy, app.state.hierarchyRoot);
            }
            // Find node data
            const node = app.state.hierarchy ? app.state.hierarchy.find(n => n.index === nodeId) : null;
            // Render detail in main panel
            app.ui.renderNodeDetail(node);
            // Render properties in secondary sidebar
            this._renderPropertiesSidebar(node);
        },

        _renderPropertiesSidebar: async function(node) {
            const container = document.getElementById('details-content');
            if (!node) {
                container.innerHTML = '<div class="placeholder-text">No node selected.</div>';
                return;
            }
            let html = '<h4 style="margin-bottom:8px">Node #' + node.index + ' &lt;' + esc(node.tag || node.type) + '&gt;</h4>';

            // Layout info
            if (node.rect) {
                html += '<div style="font-size:11px;color:var(--text-muted);margin-bottom:8px">' +
                    round(node.rect.width) + ' × ' + round(node.rect.height) + ' @ (' + round(node.rect.x) + ', ' + round(node.rect.y) + ')</div>';
            }

            // Children summary
            html += '<div style="font-size:11px;margin-bottom:8px">' + (node.children ? node.children.length : 0) + ' children';
            if (node.events && node.events.length) html += ', ' + node.events.length + ' event handler(s)';
            html += '</div>';

            // Event list
            if (node.events && node.events.length) {
                html += '<div class="detail-section"><div class="detail-section-header">Events</div>';
                node.events.forEach(ev => {
                    html += '<div class="event-row"><span class="event-type">' + esc(ev.event) + '</span><span class="event-ptr">' + esc(ev.callback_ptr) + '</span></div>';
                });
                html += '</div>';
            }

            container.innerHTML = html;
        },

        // ── CSS editing ──
        editCssProp: function(nodeId, propName, el) {
            const currentValue = el.textContent;
            const input = document.createElement('input');
            input.type = 'text';
            input.value = currentValue;
            input.className = 'form-control';
            input.style.cssText = 'width:100%;font-size:12px;padding:2px 4px;font-family:monospace;';
            el.innerHTML = '';
            el.appendChild(input);
            input.focus();
            input.select();

            const commit = () => {
                const newVal = input.value.trim();
                if (newVal !== currentValue) {
                    if (!app.state.cssOverrides[nodeId]) app.state.cssOverrides[nodeId] = {};
                    app.state.cssOverrides[nodeId][propName] = newVal;
                    app.handlers.save();
                    el.classList.add('overridden');
                }
                el.textContent = newVal || currentValue;
            };
            input.addEventListener('blur', commit);
            input.addEventListener('keydown', (e) => {
                if (e.key === 'Enter') { commit(); input.blur(); }
                if (e.key === 'Escape') { el.textContent = currentValue; }
            });
        },

        // ── Context menu actions ──
        ctxInsertChild: function(tag) {
            const nodeId = app.state.contextMenuNodeId;
            if (nodeId == null) return;
            app.log('TODO: Insert <' + tag + '> child into node #' + nodeId + ' (requires server API)', 'warning');
            // Future: POST { op: 'insert_node', parent: nodeId, type: tag }
        },
        ctxDeleteNode: function() {
            const nodeId = app.state.contextMenuNodeId;
            if (nodeId == null) return;
            app.log('TODO: Delete node #' + nodeId + ' (requires server API)', 'warning');
            // Future: POST { op: 'delete_node', node_id: nodeId }
        },

        // ── Terminal ──
        terminalEnter: async function(input) {
            try {
                const cmd = JSON.parse(input.value);
                app.log(input.value, 'command');
                const res = await app.api.post(cmd);
                app.log(JSON.stringify(res, null, 2), 'info');
                input.value = '';
            } catch(e) {
                app.log('Invalid JSON: ' + e.message, 'error');
            }
        },

        // ── Persistence ──
        save: function() {
            localStorage.setItem('azul_debugger', JSON.stringify({
                tests: app.state.tests,
                cssOverrides: app.state.cssOverrides,
            }));
        },
    },

    // ──────────── Test Runner ────────────
    runner: {
        run: async function() {
            if (app.state.executionStatus === 'running') return;
            app.state.executionStatus = 'running';
            const test = app.state.tests.find(t => t.id === app.state.activeTestId);
            if (!test) return;
            if (app.state.currentStepIndex === -1) {
                test.steps.forEach(s => { delete s.status; delete s.error; delete s.lastResponse; delete s.screenshot; delete s.duration_ms; });
                app.ui.renderSteps();
                app.state.currentStepIndex = 0;
            }
            this._loop();
        },

        _loop: async function() {
            const test = app.state.tests.find(t => t.id === app.state.activeTestId);
            if (!test || app.state.executionStatus !== 'running') return;
            if (app.state.currentStepIndex >= test.steps.length) {
                app.state.executionStatus = 'idle';
                app.state.currentStepIndex = -1;
                app.ui.renderSteps();
                app.log('Test "' + test.name + '" completed.', 'info');
                return;
            }
            const idx = app.state.currentStepIndex;
            const step = test.steps[idx];
            app.ui.renderSteps();

            if (step.breakpoint && idx > 0 && !step._breakHit) {
                app.state.executionStatus = 'paused';
                step._breakHit = true;
                app.log('Paused at breakpoint: Step ' + (idx + 1), 'warning');
                return;
            }
            step._breakHit = false;

            const t0 = performance.now();
            try {
                const res = await app.api.post({ op: step.op, ...step.params });
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
            setTimeout(() => app.runner._loop(), 100);
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
                app.runner._loop().then(() => { if (app.state.executionStatus === 'running') app.state.executionStatus = 'paused'; });
            }
        },
        reset: function() {
            app.state.executionStatus = 'idle';
            app.state.currentStepIndex = -1;
            const test = app.state.tests.find(t => t.id === app.state.activeTestId);
            if (test) test.steps.forEach(s => { delete s.status; delete s.error; delete s.screenshot; delete s.duration_ms; });
            app.ui.renderSteps();
            app.log('Reset.', 'info');
        },

        runServerSide: async function() {
            const test = app.state.tests.find(t => t.id === app.state.activeTestId);
            if (!test) { app.log('No test selected', 'error'); return; }
            const statusEl = document.getElementById('run-status');
            statusEl.classList.remove('hidden');
            app.log('Running "' + test.name + '" on server...', 'info');
            try {
                const payload = { name: test.name, steps: test.steps.map(s => ({ op: s.op, ...(s.params || {}) })) };
                const res = await app.api.postE2e(payload);
                if (res.status === 'ok' && res.results && res.results.length) {
                    const r = res.results[0];
                    test._result = r;
                    if (r.steps) r.steps.forEach((sr, i) => {
                        if (test.steps[i]) { test.steps[i].status = sr.status; test.steps[i].error = sr.error; test.steps[i].screenshot = sr.screenshot; test.steps[i].duration_ms = sr.duration_ms; test.steps[i].lastResponse = sr.response; }
                    });
                    app.log('"' + test.name + '": ' + r.status.toUpperCase() + ' (' + r.duration_ms + 'ms, ' + r.steps_passed + '/' + r.step_count + ' passed)', r.status === 'pass' ? 'info' : 'error');
                }
            } catch(e) { app.log('Failed: ' + e.message, 'error'); }
            statusEl.classList.add('hidden');
            app.ui.renderTestList();
            app.ui.renderSteps();
        },

        runAllServerSide: async function() {
            const statusEl = document.getElementById('run-status');
            statusEl.classList.remove('hidden');
            app.log('Running all ' + app.state.tests.length + ' test(s)...', 'info');
            try {
                const payload = app.state.tests.map(t => ({ name: t.name, steps: t.steps.map(s => ({ op: s.op, ...(s.params || {}) })) }));
                const res = await app.api.postE2e(payload);
                if (res.status === 'ok' && res.results) {
                    let p = 0, f = 0;
                    res.results.forEach((r, ti) => {
                        if (app.state.tests[ti]) {
                            app.state.tests[ti]._result = r;
                            if (r.steps) r.steps.forEach((sr, si) => {
                                if (app.state.tests[ti].steps[si]) { app.state.tests[ti].steps[si].status = sr.status; app.state.tests[ti].steps[si].error = sr.error; app.state.tests[ti].steps[si].duration_ms = sr.duration_ms; }
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

// ── Helpers ──
function esc(s) { return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;'); }
function round(n) { return Math.round(n * 10) / 10; }
function _downloadJSON(data, filename) {
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url; a.download = filename; a.click();
    URL.revokeObjectURL(url);
}

window.onload = () => app.init();
