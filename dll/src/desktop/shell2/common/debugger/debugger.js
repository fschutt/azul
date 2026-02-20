/**
 * Azul Debugger — unified UI for DOM inspection and E2E testing.
 */
const app = {
    config: {
        apiUrl: window.location.origin || 'http://localhost:8765',
        isMock: false
    },
    state: {
        currentView: 'inspector',
        activeTestId: null,
        tests: [],
        executionStatus: 'idle', // idle, running, paused
        currentStepIndex: -1,
        selectedNodeId: null
    },

    // All known E2E operations (superset of debug API ops + assertions)
    schema: {
        commands: {
            'get_state':    { params: [] },
            'click':        { params: [
                { name: 'selector', type: 'text', placeholder: '.button-class' },
                { name: 'text', type: 'text', placeholder: 'Button Label' }
            ]},
            'text_input':   { params: [{ name: 'text', type: 'text', placeholder: 'Input text' }] },
            'key_down':     { params: [{ name: 'key', type: 'text', placeholder: 'Enter' }] },
            'key_up':       { params: [{ name: 'key', type: 'text', placeholder: 'Enter' }] },
            'mouse_move':   { params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }] },
            'mouse_down':   { params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }, { name: 'button', type: 'text', placeholder: 'left' }] },
            'mouse_up':     { params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }, { name: 'button', type: 'text', placeholder: 'left' }] },
            'scroll':       { params: [{ name: 'x', type: 'number', value: 0 }, { name: 'y', type: 'number', value: 0 }, { name: 'delta_x', type: 'number', value: 0 }, { name: 'delta_y', type: 'number', value: 50 }] },
            'resize':       { params: [{ name: 'width', type: 'number', value: 800 }, { name: 'height', type: 'number', value: 600 }] },
            'focus':        { params: [] },
            'blur':         { params: [] },
            'close':        { params: [] },
            'wait':         { params: [{ name: 'ms', type: 'number', value: 500 }] },
            'wait_frame':   { params: [] },
            'relayout':     { params: [] },
            'take_screenshot': { params: [] },
            'get_html_string': { params: [] },
            'get_dom_tree': { params: [] },
            'get_display_list': { params: [] },
            'get_app_state': { params: [] },
            'set_app_state': { params: [{ name: 'state', type: 'text', placeholder: '{"counter": 0}' }] },
            'scroll_node_by': { params: [
                { name: 'selector', type: 'text', placeholder: '.scrollable' },
                { name: 'delta_x', type: 'number', value: 0 },
                { name: 'delta_y', type: 'number', value: 100 }
            ]},
            // Assertions — E2E only
            'assert_text':       { params: [{ name: 'selector', type: 'text', placeholder: '.label' }, { name: 'expected', type: 'text', placeholder: 'Hello' }] },
            'assert_exists':     { params: [{ name: 'selector', type: 'text', placeholder: '.element' }] },
            'assert_not_exists': { params: [{ name: 'selector', type: 'text', placeholder: '.element' }] },
            'assert_node_count': { params: [{ name: 'selector', type: 'text', placeholder: 'li' }, { name: 'expected', type: 'number', value: 5 }] },
            'assert_layout':     { params: [{ name: 'selector', type: 'text', placeholder: '.box' }, { name: 'property', type: 'text', placeholder: 'width' }, { name: 'expected', type: 'number', value: 100 }, { name: 'tolerance', type: 'number', value: 1 }] },
            'assert_app_state':  { params: [{ name: 'path', type: 'text', placeholder: 'counter' }, { name: 'expected', type: 'text', placeholder: '42' }] },
        }
    },

    init: async function() {
        this.log('Initializing Azul Debugger...', 'info');

        // Detect API URL from current page origin
        if (window.location.port) {
            this.config.apiUrl = window.location.origin;
        }

        // Load saved tests
        const saved = localStorage.getItem('azul_tests');
        if (saved) {
            try { this.state.tests = JSON.parse(saved); } catch(e) {}
        }
        if (!this.state.tests.length) this.handlers.newTest();

        // Try connecting
        try {
            const r = await this.api.post({ op: 'get_state' });
            document.getElementById('connection-status').innerText = 'Connected';
            document.getElementById('connection-status').style.color = 'var(--success)';
        } catch(e) {
            this.log('Connection failed — Mock Mode.', 'warning');
            this.config.isMock = true;
            document.getElementById('connection-status').innerText = 'Mock Mode';
        }

        this.ui.renderTestList();
        this.handlers.refreshSidebar();
    },

    log: function(msg, type = 'info') {
        const term = document.getElementById('terminal-output');
        const div = document.createElement('div');
        div.className = `log-entry ${type}`;
        const time = new Date().toLocaleTimeString().split(' ')[0];
        div.innerText = `[${time}] ${typeof msg === 'object' ? JSON.stringify(msg) : msg}`;
        term.appendChild(div);
        term.scrollTop = term.scrollHeight;
    },

    // ─── API Layer ───
    api: {
        post: async function(payload) {
            if (app.config.isMock) return app.api.mockResponse(payload);
            const res = await fetch(app.config.apiUrl, {
                method: 'POST',
                body: JSON.stringify(payload)
            });
            const json = await res.json();
            app.log(payload, 'command');
            app.log(json, 'info');
            return json;
        },

        postE2e: async function(tests) {
            if (app.config.isMock) return app.api.mockE2eResponse(tests);
            // run_e2e_tests is a normal debug command — sent via POST /
            const payload = {
                op: 'run_e2e_tests',
                tests: Array.isArray(tests) ? tests : [tests],
                timeout_secs: 300
            };
            const res = await fetch(app.config.apiUrl, {
                method: 'POST',
                body: JSON.stringify(payload)
            });
            const json = await res.json();
            // Unwrap the standard DebugHttpResponse envelope:
            // { status, request_id, data: { E2eResults: { results: [...] } } }
            if (json.data && json.data.E2eResults) {
                return { status: json.status, results: json.data.E2eResults.results };
            }
            return json;
        },

        mockResponse: async function(payload) {
            await new Promise(r => setTimeout(r, 80));
            app.log(payload, 'command');
            let response = { status: 'ok', request_id: Date.now(), data: {} };
            if (payload.op === 'get_html_string') {
                response.data = { type: 'html_string', value: { html: '<body>\n  <div id="app">\n    <div class="header">App Header</div>\n    <button class="btn">Click Me</button>\n  </div>\n</body>' }};
            } else if (payload.op === 'take_screenshot') {
                // Return a tiny 1x1 transparent PNG as placeholder
                response.data = { type: 'screenshot', value: { data: 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==' }};
            } else {
                response.data = { message: 'Mock execution successful' };
            }
            return response;
        },

        mockE2eResponse: async function(tests) {
            await new Promise(r => setTimeout(r, 500));
            const results = (Array.isArray(tests) ? tests : [tests]).map(t => ({
                name: t.name || 'Unnamed',
                status: 'pass',
                duration_ms: 42,
                step_count: (t.steps || []).length,
                steps_passed: (t.steps || []).length,
                steps_failed: 0,
                steps: (t.steps || []).map((s, i) => ({
                    step_index: i,
                    op: s.op,
                    status: 'pass',
                    duration_ms: 5,
                    logs: [],
                    screenshot: null,
                    error: null,
                    response: { status: 'ok' }
                }))
            }));
            return { status: 'ok', results };
        }
    },

    // ─── UI Logic ───
    ui: {
        switchView: function(viewName) {
            app.state.currentView = viewName;
            document.querySelectorAll('.activity-icon').forEach(el => {
                el.classList.toggle('active', el.dataset.view === viewName);
            });
            document.getElementById('sidebar-inspector').classList.toggle('hidden', viewName !== 'inspector');
            document.getElementById('sidebar-testing').classList.toggle('hidden', viewName !== 'testing');
            document.getElementById('view-inspector').classList.toggle('hidden', viewName !== 'inspector');
            document.getElementById('view-testing').classList.toggle('hidden', viewName !== 'testing');
            document.getElementById('sidebar-title').innerText = viewName === 'inspector' ? 'DOM Explorer' : 'Test Explorer';
            document.getElementById('tab-title').innerText = viewName === 'inspector' ? 'main.html' : 'runner.e2e';
        },

        renderTree: function(htmlString) {
            const container = document.getElementById('sidebar-inspector');
            container.innerHTML = '';
            htmlString.split('\n').forEach((line, idx) => {
                const div = document.createElement('div');
                div.className = 'tree-node';
                div.onclick = () => app.handlers.nodeSelected(idx);
                div.innerHTML = `<span class="node-content">${line.replace(/</g, '&lt;')}</span>`;
                container.appendChild(div);
            });
            document.getElementById('dom-preview').textContent = htmlString;
        },

        renderTestList: function() {
            const container = document.getElementById('test-list-container');
            container.innerHTML = '';
            app.state.tests.forEach(test => {
                const div = document.createElement('div');
                div.className = `list-item ${app.state.activeTestId === test.id ? 'selected' : ''}`;
                div.onclick = () => app.handlers.selectTest(test.id);
                const icon = test._result ? (test._result.status === 'pass' ? 'check_circle' : 'cancel') : 'description';
                const iconColor = test._result ? (test._result.status === 'pass' ? 'var(--success)' : 'var(--error)') : 'inherit';
                div.innerHTML = `<span class="material-icons" style="font-size:16px;color:${iconColor}">${icon}</span> ${test.name}`;
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
                let statusClass = '';
                if (step.status === 'pass') statusClass = 'pass';
                if (step.status === 'fail') statusClass = 'fail';
                if (app.state.currentStepIndex === idx) statusClass = 'active';

                div.className = `step-item ${statusClass}`;

                // Screenshot thumbnail
                let thumbHtml = '';
                if (step.screenshot) {
                    thumbHtml = `<img class="screenshot-thumb" src="${step.screenshot}" onclick="app.ui.showScreenshot('${step.screenshot.replace(/'/g, "\\'")}')" title="Click to enlarge">`;
                }

                div.innerHTML = `
                    <div class="step-gutter">
                        <div class="breakpoint ${step.breakpoint ? 'active' : ''}"
                             onclick="app.handlers.toggleBreakpoint(${idx}, event)"></div>
                    </div>
                    <div class="step-content" onclick="app.ui.showStepDetails(${idx})">
                        <div class="step-title">${step.op}${thumbHtml}</div>
                        <div class="step-meta">
                            ${Object.entries(step.params || {}).map(([k,v]) => `${k}=${v}`).join(', ') || 'No params'}
                            ${step.duration_ms != null ? `<span>${step.duration_ms}ms</span>` : ''}
                        </div>
                        ${step.error ? `<div style="color:var(--error);font-size:10px">${step.error}</div>` : ''}
                    </div>
                    <div class="step-gutter">
                        <span class="material-icons" style="font-size:14px;cursor:pointer" onclick="app.handlers.deleteStep(${idx}, event)">close</span>
                    </div>
                `;
                container.appendChild(div);
            });
        },

        showScreenshot: function(dataUri) {
            document.getElementById('screenshot-modal-img').src = dataUri;
            document.getElementById('screenshot-modal').classList.add('active');
        },

        showAddStepForm: function() {
            const container = document.getElementById('details-content');
            let opsHtml = '<select id="new-step-op" class="form-control" onchange="app.ui.updateStepParamsForm()">';
            for (let op in app.schema.commands) {
                opsHtml += `<option value="${op}">${op}</option>`;
            }
            opsHtml += '</select>';

            container.innerHTML = `
                <h3 style="margin-bottom:10px">Add Step</h3>
                <div class="form-group">
                    <label class="form-label">Operation</label>
                    ${opsHtml}
                </div>
                <div id="step-params-container"></div>
                <button class="btn-primary" onclick="app.handlers.addStepFromForm()">Add Step</button>
            `;
            this.updateStepParamsForm();
        },

        updateStepParamsForm: function() {
            const op = document.getElementById('new-step-op').value;
            const schema = app.schema.commands[op];
            const container = document.getElementById('step-params-container');
            let html = '';
            (schema.params || []).forEach(p => {
                html += `
                    <div class="form-group">
                        <label class="form-label">${p.name} (${p.type})</label>
                        <input type="${p.type === 'number' ? 'number' : 'text'}"
                               class="form-control step-param-input"
                               data-name="${p.name}"
                               placeholder="${p.placeholder || ''}"
                               value="${p.value !== undefined ? p.value : ''}">
                    </div>
                `;
            });
            container.innerHTML = html;
        },

        showStepDetails: function(idx) {
            const test = app.state.tests.find(t => t.id === app.state.activeTestId);
            if (!test) return;
            const step = test.steps[idx];
            const container = document.getElementById('details-content');

            let screenshotHtml = '';
            if (step.screenshot) {
                screenshotHtml = `
                    <div class="screenshot-container">
                        <h4>Screenshot</h4>
                        <img src="${step.screenshot}" style="max-width:100%;cursor:pointer"
                             onclick="app.ui.showScreenshot(this.src)">
                    </div>`;
            }

            container.innerHTML = `
                <h3>Step ${idx + 1}: ${step.op}</h3>
                <div class="mono" style="font-size:11px; margin:10px 0; background:var(--bg-input); padding:8px; border-radius:3px; white-space:pre-wrap;">${JSON.stringify(step.params || {}, null, 2)}</div>
                ${step.status ? `<div style="margin-bottom:10px"><strong>Status:</strong> <span style="color:${step.status === 'pass' ? 'var(--success)' : 'var(--error)'}">${step.status.toUpperCase()}</span></div>` : ''}
                ${step.error ? `<div style="color:var(--error);margin-bottom:10px">${step.error}</div>` : ''}
                ${screenshotHtml}
                <h4>Last Response</h4>
                <div class="mono" style="background:var(--bg-input); padding:8px; border-radius:3px; max-height:250px; overflow:auto; font-size:11px; white-space:pre-wrap;">${step.lastResponse ? JSON.stringify(step.lastResponse, null, 2) : 'Not executed'}</div>
            `;
        }
    },

    // ─── Handlers ───
    handlers: {
        newTest: function() {
            const newTest = {
                id: Date.now(),
                name: `Test ${app.state.tests.length + 1}`,
                steps: [{ op: 'get_state', params: {}, breakpoint: false }]
            };
            app.state.tests.push(newTest);
            this.save();
            this.selectTest(newTest.id);
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
            inputs.forEach(input => {
                const val = input.value;
                if (val !== '') params[input.dataset.name] = input.type === 'number' ? parseFloat(val) : val;
            });
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

        refreshSidebar: async function() {
            try {
                const res = await app.api.post({ op: 'get_html_string' });
                if (res.data) {
                    const html = res.data.html || (res.data.value && res.data.value.html);
                    if (html) app.ui.renderTree(html);
                }
            } catch(e) { /* ignore */ }
        },

        nodeSelected: async function(idx) {
            document.querySelectorAll('.tree-node').forEach((el, i) => el.classList.toggle('selected', i === idx));
            const container = document.getElementById('details-content');
            container.innerHTML = '<div style="color:var(--text-muted)">Fetching node properties...</div>';
            try {
                const props = await app.api.post({ op: 'get_node_css_properties', node_id: idx });
                const layout = await app.api.post({ op: 'get_node_layout', node_id: idx });
                let html = `<h3>Node ${idx}</h3>`;
                if (layout.data) html += `<h4>Layout</h4><pre class="mono" style="font-size:11px">${JSON.stringify(layout.data, null, 2)}</pre>`;
                if (props.data) {
                    html += '<h4>CSS Properties</h4><div class="mono" style="font-size:11px;max-height:300px;overflow:auto;">';
                    const list = props.data.properties || (props.data.value && props.data.value.properties) || [];
                    list.forEach(p => html += `<div>${p}</div>`);
                    html += '</div>';
                }
                container.innerHTML = html;
            } catch(e) {
                container.innerHTML = '<div style="color:var(--error)">Failed to load properties.</div>';
            }
        },

        terminalEnter: async function(input) {
            try {
                const cmd = JSON.parse(input.value);
                await app.api.post(cmd);
                input.value = '';
            } catch(e) {
                app.log('Invalid JSON command', 'error');
            }
        },

        exportWorkspace: function() {
            const ws = { tests: app.state.tests, apiUrl: app.config.apiUrl };
            app.handlers._downloadJSON(ws, 'azul-workspace.json');
        },

        exportTests: function() {
            // Export in the portable E2E format (strip internal fields)
            const exported = app.state.tests.map(t => ({
                name: t.name,
                steps: t.steps.map(s => ({ op: s.op, ...s.params }))
            }));
            app.handlers._downloadJSON(exported, 'azul_e2e_tests.json');
        },

        importFile: function(input) {
            const file = input.files[0];
            if (!file) return;
            const reader = new FileReader();
            reader.onload = function(e) {
                try {
                    const imported = JSON.parse(e.target.result);
                    const arr = Array.isArray(imported) ? imported : [imported];
                    arr.forEach(t => {
                        // Normalise imported test format
                        app.state.tests.push({
                            id: Date.now() + Math.random(),
                            name: t.name || `Imported Test`,
                            steps: (t.steps || []).map(s => {
                                const { op, ...params } = s;
                                return { op, params, breakpoint: false };
                            })
                        });
                    });
                    app.handlers.save();
                    app.ui.renderTestList();
                    app.log(`Imported ${arr.length} test(s)`, 'info');
                } catch(err) {
                    alert('Invalid JSON file: ' + err.message);
                }
            };
            reader.readAsText(file);
            input.value = ''; // allow re-importing same file
        },

        save: function() {
            localStorage.setItem('azul_tests', JSON.stringify(app.state.tests));
        },

        _downloadJSON: function(data, filename) {
            const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
            const url = URL.createObjectURL(blob);
            const a = document.createElement('a');
            a.href = url; a.download = filename; a.click();
            URL.revokeObjectURL(url);
        }
    },

    // ─── Test Runner ───
    runner: {
        // Client-side runner (sends individual commands to the running app)
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
                app.log(`Test '${test.name}' completed.`, 'info');
                return;
            }

            const idx = app.state.currentStepIndex;
            const step = test.steps[idx];
            app.ui.renderSteps();

            if (step.breakpoint && idx > 0 && !step._breakHit) {
                app.state.executionStatus = 'paused';
                step._breakHit = true;
                app.log(`Paused at breakpoint: Step ${idx + 1}`, 'warning');
                return;
            }
            step._breakHit = false;

            const t0 = performance.now();
            try {
                const payload = { op: step.op, ...step.params };
                const res = await app.api.post(payload);
                step.lastResponse = res;
                step.duration_ms = Math.round(performance.now() - t0);

                if (res.status === 'error') throw new Error(res.message);

                // Capture screenshot data if returned
                if (res.data && res.data.value && res.data.value.data && res.data.type === 'screenshot') {
                    step.screenshot = res.data.value.data;
                }

                step.status = 'pass';
            } catch(e) {
                step.status = 'fail';
                step.error = e.message;
                step.duration_ms = Math.round(performance.now() - t0);
                app.state.executionStatus = 'idle';
                app.log(`Step ${idx + 1} Failed: ${e.message}`, 'error');
                app.ui.renderSteps();
                return;
            }

            app.state.currentStepIndex++;
            setTimeout(() => app.runner._loop(), 150);
        },

        pause: function() {
            if (app.state.executionStatus === 'running') {
                app.state.executionStatus = 'paused';
                app.log('Test paused.', 'warning');
            }
        },

        step: function() {
            if (app.state.executionStatus === 'paused' || (app.state.executionStatus === 'idle' && app.state.currentStepIndex === -1)) {
                if (app.state.currentStepIndex === -1) app.state.currentStepIndex = 0;
                app.state.executionStatus = 'running';
                app.runner._loop().then(() => {
                    if (app.state.executionStatus === 'running') app.state.executionStatus = 'paused';
                });
            }
        },

        reset: function() {
            app.state.executionStatus = 'idle';
            app.state.currentStepIndex = -1;
            const test = app.state.tests.find(t => t.id === app.state.activeTestId);
            if (test) test.steps.forEach(s => { delete s.status; delete s.error; delete s.screenshot; delete s.duration_ms; });
            app.ui.renderSteps();
            app.log('Test execution reset.', 'info');
        },

        // Server-side runner — sends run_e2e_tests command via POST /
        runServerSide: async function() {
            const test = app.state.tests.find(t => t.id === app.state.activeTestId);
            if (!test) { app.log('No test selected', 'error'); return; }

            const statusEl = document.getElementById('run-status');
            statusEl.classList.remove('hidden');
            app.log(`Running test "${test.name}" on server (StubWindow)...`, 'info');

            try {
                const payload = {
                    name: test.name,
                    steps: test.steps.map(s => ({ op: s.op, ...(s.params || {}) }))
                };
                const res = await app.api.postE2e(payload);

                if (res.status === 'ok' && res.results && res.results.length) {
                    const result = res.results[0];
                    test._result = result;

                    // Merge step results back into UI
                    if (result.steps) {
                        result.steps.forEach((sr, i) => {
                            if (test.steps[i]) {
                                test.steps[i].status = sr.status;
                                test.steps[i].error = sr.error;
                                test.steps[i].screenshot = sr.screenshot;
                                test.steps[i].duration_ms = sr.duration_ms;
                                test.steps[i].lastResponse = sr.response;
                            }
                        });
                    }
                    app.log(`Test "${test.name}": ${result.status.toUpperCase()} (${result.duration_ms}ms, ${result.steps_passed}/${result.step_count} passed)`, result.status === 'pass' ? 'info' : 'error');
                } else if (res.status === 'error') {
                    app.log(`Server error: ${res.message}`, 'error');
                }
            } catch(e) {
                app.log(`Failed to run test: ${e.message}`, 'error');
            }

            statusEl.classList.add('hidden');
            app.ui.renderTestList();
            app.ui.renderSteps();
        },

        runAllServerSide: async function() {
            const statusEl = document.getElementById('run-status');
            statusEl.classList.remove('hidden');
            app.log('Running ALL tests on server...', 'info');

            try {
                const payload = app.state.tests.map(t => ({
                    name: t.name,
                    steps: t.steps.map(s => ({ op: s.op, ...(s.params || {}) }))
                }));
                const res = await app.api.postE2e(payload);

                if (res.status === 'ok' && res.results) {
                    let passed = 0, failed = 0;
                    res.results.forEach((result, ti) => {
                        if (app.state.tests[ti]) {
                            app.state.tests[ti]._result = result;
                            if (result.steps) {
                                result.steps.forEach((sr, si) => {
                                    if (app.state.tests[ti].steps[si]) {
                                        app.state.tests[ti].steps[si].status = sr.status;
                                        app.state.tests[ti].steps[si].error = sr.error;
                                        app.state.tests[ti].steps[si].screenshot = sr.screenshot;
                                        app.state.tests[ti].steps[si].duration_ms = sr.duration_ms;
                                    }
                                });
                            }
                        }
                        if (result.status === 'pass') passed++; else failed++;
                    });
                    app.log(`All tests done: ${passed} passed, ${failed} failed`, failed ? 'error' : 'info');
                }
            } catch(e) {
                app.log(`Failed: ${e.message}`, 'error');
            }

            statusEl.classList.add('hidden');
            app.ui.renderTestList();
            app.ui.renderSteps();
        }
    }
};

window.onload = () => app.init();