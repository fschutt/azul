# Plan 2: Reusable JS Components for Component Editability

## Goal

Refactor `debugger.js` so that the new component editing features (type-aware
field editors, live preview, data binding UI) are built from **reusable,
composable JS widgets** — not monolithic HTML string concatenation.

The current `showComponentDetail()` is ~130 lines of `innerHTML +=` string
building. That pattern does not scale to interactive, type-aware field editors.
This plan extracts a widget library that other views can reuse too.

---

## Current Architecture (Problems)

### Two inconsistent DOM creation patterns

1. **`innerHTML` string concatenation** — used in `showComponentDetail()`,
   `renderNodeDetail()`, sidebar rendering. Fast to write, impossible to
   attach event listeners cleanly, no reuse.

2. **`document.createElement()`** — used in `renderDomTree()`, `app.json.render()`.
   Properly structured, supports event listeners, composable.

### Monolithic renderers

- `showComponentDetail(idx)`: reads `app.state.componentData.components[idx]`
  and emits ~130 lines of HTML for data model, callbacks, CSS editor, template,
  preview. All in one function. No way to update a single field without
  re-rendering everything.

- `renderNodeDetail(node)`: similar pattern for the Inspector view.

### Good reuse precedent: `app.json`

The `app.json` namespace (JSON tree editor) is well-abstracted:
- `app.json.render(container, data, path, options)` — renders any JSON value
  as an expandable/collapsible tree with inline editing
- Handles strings, numbers, booleans, arrays, objects
- Supports grouping, collapse/expand, edit mode toggle
- Used in the App State panel

**This is the pattern to follow** for all new widgets.

---

## Proposed Widget Library: `app.widgets`

Create a new namespace `app.widgets` in `debugger.js` (or a separate
`widgets.js` file loaded before `debugger.js`). All widgets follow the
same contract:

```javascript
app.widgets.WidgetName = {
    /**
     * Create a widget DOM element.
     * @param {Object} config - Widget configuration
     * @param {Object} state - Current value/state
     * @param {Object} callbacks - Event handlers { onChange, onFocus, ... }
     * @returns {HTMLElement} - The widget's root DOM element
     */
    render: function(config, state, callbacks) { ... },

    /**
     * Update an existing widget with new state (optional — for perf).
     * @param {HTMLElement} el - The widget's root element (from render)
     * @param {Object} newState - Updated state
     */
    update: function(el, newState) { ... }
};
```

### Design Principles

1. **Always use `createElement`** — never `innerHTML` for interactive widgets.
   Static read-only content can still use `innerHTML` for speed.

2. **Return DOM elements** — widgets return `HTMLElement`, not HTML strings.
   Callers append them to the DOM. This allows proper event listener attachment.

3. **Callbacks via config** — widgets never touch global state directly.
   They receive `onChange`, `onFocus`, `onBlur` callbacks from the caller.

4. **Flat state** — widget state is a plain object. No classes, no `this`
   binding issues. Works with the existing `app.state` pattern.

5. **CSS classes for styling** — widgets use descriptive CSS class names
   (`azd-field-editor`, `azd-type-badge`, etc.). All styling in `debugger.css`.

---

## Widget Catalog

### W1: `app.widgets.FieldEditor` — Type-Aware Field Input

The core widget. Given a `ComponentFieldType` and a current value, renders
the appropriate input control.

```javascript
app.widgets.FieldEditor = {
    render: function(config, state, callbacks) {
        // config: { name, fieldType, required, description, readOnly }
        // state:  { value, source, expanded }
        // callbacks: { onChange(name, newValue), onSourceChange(name, newSource) }

        var row = document.createElement('div');
        row.className = 'azd-field-row';

        // Label
        var label = document.createElement('label');
        label.className = 'azd-field-label';
        label.textContent = config.name;
        if (config.required) label.classList.add('azd-required');
        row.appendChild(label);

        // Type badge
        var badge = app.widgets.TypeBadge.render({ fieldType: config.fieldType });
        row.appendChild(badge);

        // Input control — dispatch on fieldType.type
        var input = app.widgets.FieldInput.render(config, state, callbacks);
        row.appendChild(input);

        return row;
    }
};
```

**Field type → control mapping:**

| `fieldType.type` | Control | Behavior |
|---|---|---|
| `String` | `<input type="text">` | Free text, shows placeholder from default |
| `Bool` | `<input type="checkbox">` | Toggle |
| `I32`, `I64`, `U32`, `U64`, `Usize` | `<input type="number">` | Integer, step=1 |
| `F32`, `F64` | `<input type="number">` | Float, step=0.1 |
| `ColorU` | `<input type="color">` + hex display | Color picker |
| `Option { inner }` | Checkbox ("has value?") + inner control | Null toggle + value |
| `Vec { inner }` | List with + / - buttons + inner controls per item | Dynamic array |
| `StyledDom` | Drop zone ("drag component here") | Drag & drop target |
| `Callback` | Read-only signature badge + fn pointer name | Not editable in preview |
| `RefAny` | Read-only type hint badge | Not editable |
| `EnumRef` | `<select>` dropdown with variant names | From `ComponentEnumModel` |
| `StructRef` | Nested `FieldEditor` group (expandable) | Recursive |
| `ImageRef` | File picker + thumbnail | Image upload |
| `FontRef` | Font name input + preview | Font selector |
| `CssProperty` | CSS property editor (property + value) | Specialized |

### W2: `app.widgets.TypeBadge` — Type Indicator

Small, color-coded badge showing the field type. Used inline next to field names.

```javascript
app.widgets.TypeBadge = {
    render: function(config) {
        // config: { fieldType }
        var el = document.createElement('span');
        el.className = 'azd-type-badge azd-type-' + config.fieldType.type.toLowerCase();

        switch (config.fieldType.type) {
            case 'String':   el.textContent = 'Str'; break;
            case 'Bool':     el.textContent = 'Bool'; break;
            case 'I32':      el.textContent = 'i32'; break;
            case 'F64':      el.textContent = 'f64'; break;
            case 'ColorU':   el.textContent = '■'; el.style.color = '#f0f'; break;
            case 'Option':   el.textContent = config.fieldType.inner.type + '?'; break;
            case 'Vec':      el.textContent = '[' + config.fieldType.inner.type + ']'; break;
            case 'StyledDom':el.textContent = '◻ Slot'; break;
            case 'Callback': el.textContent = 'fn()'; break;
            case 'EnumRef':  el.textContent = config.fieldType.name; break;
            case 'StructRef':el.textContent = config.fieldType.name; break;
            default:         el.textContent = config.fieldType.type; break;
        }
        return el;
    }
};
```

### W3: `app.widgets.FieldInput` — Primitive Input Controls

Renders the actual input control based on field type. Called by `FieldEditor`.

```javascript
app.widgets.FieldInput = {
    render: function(config, state, callbacks) {
        var ft = config.fieldType;

        switch (ft.type) {
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
        input.value = state.value || '';
        input.placeholder = config.default || '';
        input.disabled = config.readOnly;
        input.addEventListener('input', function() {
            callbacks.onChange(config.name, { type: 'String', value: input.value });
        });
        return input;
    },

    _renderBool: function(config, state, callbacks) {
        var label = document.createElement('label');
        label.className = 'azd-input azd-input-bool';
        var cb = document.createElement('input');
        cb.type = 'checkbox';
        cb.checked = !!state.value;
        cb.disabled = config.readOnly;
        cb.addEventListener('change', function() {
            callbacks.onChange(config.name, { type: 'Bool', value: cb.checked });
        });
        label.appendChild(cb);
        label.appendChild(document.createTextNode(cb.checked ? 'true' : 'false'));
        return label;
    },

    _renderColor: function(config, state, callbacks) {
        var wrap = document.createElement('div');
        wrap.className = 'azd-input azd-input-color';
        var picker = document.createElement('input');
        picker.type = 'color';
        picker.value = state.value ? app.widgets._colorUToHex(state.value) : '#000000';
        picker.disabled = config.readOnly;
        var hex = document.createElement('span');
        hex.className = 'azd-color-hex';
        hex.textContent = picker.value;
        picker.addEventListener('input', function() {
            hex.textContent = picker.value;
            callbacks.onChange(config.name, {
                type: 'ColorU',
                value: app.widgets._hexToColorU(picker.value)
            });
        });
        wrap.appendChild(picker);
        wrap.appendChild(hex);
        return wrap;
    },

    _renderEnum: function(config, state, callbacks) {
        // config.enumModel: { name, variants: [{ name, ... }] }
        var select = document.createElement('select');
        select.className = 'azd-input azd-input-enum';
        select.disabled = config.readOnly;
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
            callbacks.onChange(config.name, {
                type: 'Enum', variant: select.value, fields: []
            });
        });
        return select;
    },

    _renderOption: function(config, state, callbacks) {
        var wrap = document.createElement('div');
        wrap.className = 'azd-input azd-input-option';
        var hasValue = state.value !== null && state.value !== undefined;

        var toggle = document.createElement('input');
        toggle.type = 'checkbox';
        toggle.checked = hasValue;
        toggle.disabled = config.readOnly;

        var innerWrap = document.createElement('div');
        innerWrap.className = 'azd-option-inner';

        if (hasValue) {
            var innerConfig = Object.assign({}, config, {
                fieldType: config.fieldType.inner,
                name: config.name
            });
            var innerInput = this.render(innerConfig, { value: state.value }, callbacks);
            innerWrap.appendChild(innerInput);
        } else {
            innerWrap.textContent = 'None';
            innerWrap.classList.add('azd-muted');
        }

        toggle.addEventListener('change', function() {
            if (toggle.checked) {
                callbacks.onChange(config.name, { type: 'Some', value: null });
            } else {
                callbacks.onChange(config.name, { type: 'None' });
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
        items.forEach(function(item, idx) {
            var itemRow = document.createElement('div');
            itemRow.className = 'azd-vec-item';

            var innerConfig = Object.assign({}, config, {
                fieldType: config.fieldType.inner,
                name: config.name + '[' + idx + ']'
            });
            var innerInput = app.widgets.FieldInput.render(innerConfig, { value: item }, {
                onChange: function(_, newVal) {
                    var newItems = items.slice();
                    newItems[idx] = newVal.value;
                    callbacks.onChange(config.name, { type: 'Vec', value: newItems });
                }
            });
            itemRow.appendChild(innerInput);

            var removeBtn = document.createElement('button');
            removeBtn.className = 'azd-btn-icon';
            removeBtn.textContent = '×';
            removeBtn.title = 'Remove';
            removeBtn.addEventListener('click', function() {
                var newItems = items.slice();
                newItems.splice(idx, 1);
                callbacks.onChange(config.name, { type: 'Vec', value: newItems });
            });
            itemRow.appendChild(removeBtn);
            list.appendChild(itemRow);
        });
        wrap.appendChild(list);

        var addBtn = document.createElement('button');
        addBtn.className = 'azd-btn-small';
        addBtn.textContent = '+ Add';
        addBtn.disabled = config.readOnly;
        addBtn.addEventListener('click', function() {
            var newItems = items.slice();
            newItems.push(app.widgets._defaultForType(config.fieldType.inner));
            callbacks.onChange(config.name, { type: 'Vec', value: newItems });
        });
        wrap.appendChild(addBtn);
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

        // Drag & drop handlers
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
            var data = JSON.parse(e.dataTransfer.getData('text/plain'));
            callbacks.onChange(config.name, {
                type: 'ComponentInstance',
                library: data.library,
                component: data.component
            });
        });

        return zone;
    },

    _renderCallback: function(config, state, callbacks) {
        var wrap = document.createElement('div');
        wrap.className = 'azd-input azd-input-callback';

        var sig = config.fieldType.signature;
        var sigText = 'fn(';
        if (sig && sig.args) {
            sigText += sig.args.map(function(a) { return a.name + ': ' + a.arg_type.type; }).join(', ');
        }
        sigText += ') → ' + (sig ? sig.return_type || 'Update' : 'Update');

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

    _renderStruct: function(config, state, callbacks) {
        // Renders each field of the struct as a nested FieldEditor
        var wrap = document.createElement('div');
        wrap.className = 'azd-input azd-input-struct';

        var header = document.createElement('div');
        header.className = 'azd-struct-header';
        header.textContent = config.fieldType.name;
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
                        callbacks.onChange(config.name, { type: 'Struct', value: newStruct });
                    }
                });
                wrap.appendChild(fieldEl);
            });
        }
        return wrap;
    }
};
```

### W4: `app.widgets.ValueSourceToggle` — Default / Literal / Binding

A three-state toggle for `ComponentFieldValueSource`. Shown next to each
field in the Application Composition View.

```javascript
app.widgets.ValueSourceToggle = {
    render: function(config, state, callbacks) {
        // state: { source: 'default' | 'literal' | 'binding' }
        var wrap = document.createElement('div');
        wrap.className = 'azd-source-toggle';

        var options = [
            { value: 'default', label: 'D', title: 'Use default value' },
            { value: 'literal', label: 'L', title: 'Set literal value' },
            { value: 'binding', label: 'B', title: 'Bind to app state' }
        ];

        options.forEach(function(opt) {
            var btn = document.createElement('button');
            btn.className = 'azd-source-btn';
            btn.textContent = opt.label;
            btn.title = opt.title;
            if (state.source === opt.value) btn.classList.add('azd-active');
            btn.addEventListener('click', function() {
                callbacks.onSourceChange(config.name, opt.value);
            });
            wrap.appendChild(btn);
        });

        return wrap;
    }
};
```

### W5: `app.widgets.BindingInput` — App State Path Autocomplete

A text input with autocomplete for binding paths (e.g. `app_state.user.name`).

```javascript
app.widgets.BindingInput = {
    render: function(config, state, callbacks) {
        // config: { expectedType, appStateSchema }
        // state:  { path }
        var wrap = document.createElement('div');
        wrap.className = 'azd-binding-input';

        var input = document.createElement('input');
        input.type = 'text';
        input.className = 'azd-input azd-input-binding';
        input.value = state.path || '';
        input.placeholder = 'app_state.field.path';

        var suggestions = document.createElement('ul');
        suggestions.className = 'azd-binding-suggestions';
        suggestions.style.display = 'none';

        input.addEventListener('input', function() {
            var partial = input.value;
            var matches = app.widgets.BindingInput._getSuggestions(
                partial, config.appStateSchema, config.expectedType
            );
            suggestions.innerHTML = '';
            if (matches.length > 0) {
                suggestions.style.display = 'block';
                matches.forEach(function(m) {
                    var li = document.createElement('li');
                    li.className = 'azd-suggestion';
                    li.textContent = m.path;
                    var typeBadge = app.widgets.TypeBadge.render({ fieldType: m.fieldType });
                    li.appendChild(typeBadge);
                    li.addEventListener('click', function() {
                        input.value = m.path;
                        suggestions.style.display = 'none';
                        callbacks.onChange(config.name, { type: 'binding', path: m.path });
                    });
                    suggestions.appendChild(li);
                });
            } else {
                suggestions.style.display = 'none';
            }
            callbacks.onChange(config.name, { type: 'binding', path: partial });
        });

        input.addEventListener('blur', function() {
            setTimeout(function() { suggestions.style.display = 'none'; }, 200);
        });

        wrap.appendChild(input);
        wrap.appendChild(suggestions);
        return wrap;
    },

    _getSuggestions: function(partial, schema, expectedType) {
        // Walk the app state schema to find paths matching the partial input
        // Returns [{ path: 'app_state.user.name', fieldType: { type: 'String' } }, ...]
        if (!schema) return [];
        var results = [];
        // ... recursive traversal of schema ...
        return results;
    }
};
```

### W6: `app.widgets.CssEditor` — CSS Template Editor with Preview

Wraps the CSS textarea with template expression autocomplete and live
error display.

```javascript
app.widgets.CssEditor = {
    render: function(config, state, callbacks) {
        // config: { readOnly, dataModelFields }
        // state:  { css, errors, expandedCss }
        // callbacks: { onChange(newCss), onPreviewRequest() }

        var wrap = document.createElement('div');
        wrap.className = 'azd-css-editor';

        // Header
        var header = document.createElement('div');
        header.className = 'azd-css-header';
        header.textContent = 'Scoped CSS';
        wrap.appendChild(header);

        // Textarea
        var textarea = document.createElement('textarea');
        textarea.className = 'azd-css-textarea';
        textarea.value = state.css || '';
        textarea.disabled = config.readOnly;
        textarea.spellcheck = false;
        textarea.setAttribute('data-lang', 'css');

        var debounceTimer = null;
        textarea.addEventListener('input', function() {
            clearTimeout(debounceTimer);
            debounceTimer = setTimeout(function() {
                callbacks.onChange(textarea.value);
            }, 150);
        });

        // Template expression autocomplete on '{'
        textarea.addEventListener('keydown', function(e) {
            if (e.key === '{') {
                app.widgets.CssEditor._showFieldAutocomplete(
                    textarea, config.dataModelFields
                );
            }
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

        // Save button (for user_defined components)
        if (!config.readOnly) {
            var saveBtn = document.createElement('button');
            saveBtn.className = 'azd-btn';
            saveBtn.textContent = 'Save CSS';
            saveBtn.addEventListener('click', function() {
                callbacks.onSave(textarea.value);
            });
            wrap.appendChild(saveBtn);
        }

        return wrap;
    },

    _showFieldAutocomplete: function(textarea, fields) {
        // Show a popup with field names that can be inserted as {field_name}
        // Only show fields whose types make sense in CSS values:
        // String, I32, U32, F32, F64, Bool, ColorU
        var cssCompatible = (fields || []).filter(function(f) {
            return ['String','Bool','I32','I64','U32','U64','F32','F64','ColorU']
                .indexOf(f.field_type.type) >= 0;
        });
        // ... render popup near cursor position ...
    }
};
```

### W7: `app.widgets.PreviewPanel` — Component Preview with OS/Theme Switcher

Renders the live preview area with an `<img>` for the screenshot and
dropdowns for OS/theme/language switching.

```javascript
app.widgets.PreviewPanel = {
    render: function(config, state, callbacks) {
        // config: { componentName }
        // state:  { screenshotBase64, os, theme, language, loading }
        // callbacks: { onContextChange(ctx) }

        var wrap = document.createElement('div');
        wrap.className = 'azd-preview-panel';

        // Preview image
        var img = document.createElement('img');
        img.className = 'azd-preview-img';
        if (state.screenshotBase64) {
            img.src = 'data:image/png;base64,' + state.screenshotBase64;
        }
        if (state.loading) wrap.classList.add('azd-loading');
        wrap.appendChild(img);

        // OS/Theme/Language switcher bar
        var bar = document.createElement('div');
        bar.className = 'azd-preview-bar';

        bar.appendChild(this._renderDropdown('OS', [
            { value: 'macos', label: 'macOS' },
            { value: 'windows', label: 'Windows' },
            { value: 'linux', label: 'Linux' },
            { value: 'ios', label: 'iOS' },
            { value: 'android', label: 'Android' }
        ], state.os, function(val) {
            callbacks.onContextChange({ os: val });
        }));

        bar.appendChild(this._renderDropdown('Theme', [
            { value: 'light', label: 'Light' },
            { value: 'dark', label: 'Dark' }
        ], state.theme, function(val) {
            callbacks.onContextChange({ theme: val });
        }));

        bar.appendChild(this._renderDropdown('Lang', [
            { value: 'en-US', label: 'English' },
            { value: 'de-DE', label: 'Deutsch' },
            { value: 'ja-JP', label: '日本語' },
            { value: 'zh-CN', label: '中文' },
            { value: 'ar-SA', label: 'العربية' }
        ], state.language, function(val) {
            callbacks.onContextChange({ language: val });
        }));

        wrap.appendChild(bar);
        return wrap;
    },

    _renderDropdown: function(label, options, current, onChange) {
        var wrap = document.createElement('div');
        wrap.className = 'azd-preview-dropdown';

        var lbl = document.createElement('span');
        lbl.className = 'azd-preview-dropdown-label';
        lbl.textContent = label + ':';
        wrap.appendChild(lbl);

        var select = document.createElement('select');
        select.className = 'azd-preview-dropdown-select';
        options.forEach(function(opt) {
            var o = document.createElement('option');
            o.value = opt.value;
            o.textContent = opt.label;
            if (current === opt.value) o.selected = true;
            select.appendChild(o);
        });
        select.addEventListener('change', function() { onChange(select.value); });
        wrap.appendChild(select);
        return wrap;
    }
};
```

### W8: `app.widgets.DataModelEditor` — Full Data Model Editing Panel

Combines `FieldEditor` rows for all fields in a `ComponentDataModel`.
Supports adding/removing fields (for user-defined components).

```javascript
app.widgets.DataModelEditor = {
    render: function(config, state, callbacks) {
        // config: { dataModel, enumModels, structModels, readOnly, mode }
        //   mode: 'preview' | 'composition'
        // state:  { fieldValues: { name: value, ... } }
        // callbacks: { onFieldChange(name, value), onAddField(), onRemoveField(name) }

        var wrap = document.createElement('div');
        wrap.className = 'azd-data-model-editor';

        // Header
        var header = document.createElement('div');
        header.className = 'azd-dm-header';
        header.textContent = config.dataModel.name || 'Data Model';
        wrap.appendChild(header);

        // Field rows
        var fields = config.dataModel.fields || [];
        fields.forEach(function(field) {
            var fieldState = {
                value: state.fieldValues[field.name] || null,
                source: (state.fieldSources && state.fieldSources[field.name]) || 'default'
            };

            var row = document.createElement('div');
            row.className = 'azd-dm-field-row';

            // In composition mode, add source toggle
            if (config.mode === 'composition') {
                var toggle = app.widgets.ValueSourceToggle.render(
                    { name: field.name },
                    { source: fieldState.source },
                    { onSourceChange: callbacks.onSourceChange }
                );
                row.appendChild(toggle);
            }

            // Resolve enum/struct models for this field type
            var fieldConfig = {
                name: field.name,
                fieldType: field.field_type,
                required: field.required,
                description: field.description,
                readOnly: config.readOnly,
                enumModel: field.field_type.type === 'EnumRef'
                    ? app.widgets._findModel(config.enumModels, field.field_type.name) : null,
                structModel: field.field_type.type === 'StructRef'
                    ? app.widgets._findModel(config.structModels, field.field_type.name) : null
            };

            var fieldEl = app.widgets.FieldEditor.render(fieldConfig, fieldState, {
                onChange: function(name, newVal) {
                    callbacks.onFieldChange(name, newVal);
                }
            });
            row.appendChild(fieldEl);

            // Remove button (if editable)
            if (!config.readOnly) {
                var removeBtn = document.createElement('button');
                removeBtn.className = 'azd-btn-icon azd-btn-danger';
                removeBtn.textContent = '×';
                removeBtn.title = 'Remove field "' + field.name + '"';
                removeBtn.addEventListener('click', function() {
                    callbacks.onRemoveField(field.name);
                });
                row.appendChild(removeBtn);
            }

            wrap.appendChild(row);
        });

        // Add field button
        if (!config.readOnly) {
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
};
```

### W9: `app.widgets.AddFieldDialog` — Dialog for Adding a Data Model Field

Modal dialog for specifying a new field's name, type, and default value.

```javascript
app.widgets.AddFieldDialog = {
    render: function(config, callbacks) {
        // config: { enumModels, structModels }
        // callbacks: { onConfirm(field), onCancel() }

        var overlay = document.createElement('div');
        overlay.className = 'azd-dialog-overlay';

        var dialog = document.createElement('div');
        dialog.className = 'azd-dialog';

        // Title
        var title = document.createElement('h3');
        title.textContent = 'Add Field';
        dialog.appendChild(title);

        // Name input
        var nameInput = app.widgets.AddFieldDialog._labeledInput('Name', 'text', 'field_name');
        dialog.appendChild(nameInput.wrap);

        // Type selector
        var typeSelect = document.createElement('select');
        typeSelect.className = 'azd-input';
        var types = [
            'String', 'Bool', 'I32', 'I64', 'U32', 'U64', 'F32', 'F64',
            'ColorU', 'StyledDom', 'ImageRef', 'FontRef'
        ];
        types.forEach(function(t) {
            var opt = document.createElement('option');
            opt.value = t; opt.textContent = t;
            typeSelect.appendChild(opt);
        });
        // Add enum/struct refs
        if (config.enumModels) {
            config.enumModels.forEach(function(e) {
                var opt = document.createElement('option');
                opt.value = 'EnumRef:' + e.name;
                opt.textContent = 'enum ' + e.name;
                typeSelect.appendChild(opt);
            });
        }
        if (config.structModels) {
            config.structModels.forEach(function(s) {
                var opt = document.createElement('option');
                opt.value = 'StructRef:' + s.name;
                opt.textContent = 'struct ' + s.name;
                typeSelect.appendChild(opt);
            });
        }
        dialog.appendChild(app.widgets.AddFieldDialog._labeled('Type', typeSelect));

        // Required checkbox
        var reqCb = document.createElement('input');
        reqCb.type = 'checkbox';
        dialog.appendChild(app.widgets.AddFieldDialog._labeled('Required', reqCb));

        // Description
        var descInput = app.widgets.AddFieldDialog._labeledInput('Description', 'text', '');
        dialog.appendChild(descInput.wrap);

        // Buttons
        var btnRow = document.createElement('div');
        btnRow.className = 'azd-dialog-buttons';

        var cancelBtn = document.createElement('button');
        cancelBtn.className = 'azd-btn';
        cancelBtn.textContent = 'Cancel';
        cancelBtn.addEventListener('click', function() {
            document.body.removeChild(overlay);
            callbacks.onCancel();
        });

        var confirmBtn = document.createElement('button');
        confirmBtn.className = 'azd-btn azd-btn-primary';
        confirmBtn.textContent = 'Add';
        confirmBtn.addEventListener('click', function() {
            var typeVal = typeSelect.value;
            var fieldType;
            if (typeVal.startsWith('EnumRef:')) {
                fieldType = { type: 'EnumRef', name: typeVal.substring(8) };
            } else if (typeVal.startsWith('StructRef:')) {
                fieldType = { type: 'StructRef', name: typeVal.substring(10) };
            } else {
                fieldType = { type: typeVal };
            }

            var field = {
                name: nameInput.input.value,
                field_type: fieldType,
                required: reqCb.checked,
                description: descInput.input.value,
                default: null
            };
            document.body.removeChild(overlay);
            callbacks.onConfirm(field);
        });

        btnRow.appendChild(cancelBtn);
        btnRow.appendChild(confirmBtn);
        dialog.appendChild(btnRow);

        overlay.appendChild(dialog);
        return overlay;
    },

    _labeledInput: function(label, type, placeholder) {
        var wrap = document.createElement('div');
        wrap.className = 'azd-dialog-field';
        var lbl = document.createElement('label');
        lbl.textContent = label;
        var input = document.createElement('input');
        input.type = type;
        input.className = 'azd-input';
        input.placeholder = placeholder;
        wrap.appendChild(lbl);
        wrap.appendChild(input);
        return { wrap: wrap, input: input };
    },

    _labeled: function(label, el) {
        var wrap = document.createElement('div');
        wrap.className = 'azd-dialog-field';
        var lbl = document.createElement('label');
        lbl.textContent = label;
        wrap.appendChild(lbl);
        wrap.appendChild(el);
        return wrap;
    }
};
```

### W10: `app.widgets.ComponentDragHandle` — Draggable Component Badge

Used in the component list sidebar. Makes library components draggable so
they can be dropped into `StyledDom` slot fields.

```javascript
app.widgets.ComponentDragHandle = {
    render: function(config) {
        // config: { library, tag, displayName }
        var el = document.createElement('div');
        el.className = 'azd-component-drag';
        el.textContent = config.displayName;
        el.draggable = true;
        el.addEventListener('dragstart', function(e) {
            e.dataTransfer.setData('text/plain', JSON.stringify({
                library: config.library,
                component: config.tag
            }));
            e.dataTransfer.effectAllowed = 'copy';
        });
        return el;
    }
};
```

---

## Helper Utilities: `app.widgets._*`

```javascript
// Color conversion helpers
app.widgets._colorUToHex = function(c) {
    return '#' + [c.r, c.g, c.b].map(function(v) {
        return ('0' + v.toString(16)).slice(-2);
    }).join('');
};

app.widgets._hexToColorU = function(hex) {
    var r = parseInt(hex.slice(1, 3), 16);
    var g = parseInt(hex.slice(3, 5), 16);
    var b = parseInt(hex.slice(5, 7), 16);
    return { r: r, g: g, b: b, a: 255 };
};

// Default value for a given ComponentFieldType
app.widgets._defaultForType = function(ft) {
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
};

// Find a model by name in a list
app.widgets._findModel = function(models, name) {
    if (!models) return null;
    for (var i = 0; i < models.length; i++) {
        if (models[i].name === name) return models[i];
    }
    return null;
};
```

---

## Refactoring `showComponentDetail()`

### Current structure (monolithic):

```javascript
showComponentDetail: function(idx) {
    var c = app.state.componentData.components[idx];
    var leftHtml = '';     // ~60 lines of += string building
    var rightHtml = '';    // ~30 lines of += string building
    document.getElementById('component-detail-left').innerHTML = leftHtml;
    document.getElementById('component-detail-right').innerHTML = rightHtml;
    // Inline event listener attachment for CSS save button
}
```

### New structure (widget-based):

```javascript
showComponentDetail: function(idx) {
    var c = app.state.componentData.components[idx];
    var isEditable = c.source === 'user_defined';

    var leftPanel = document.getElementById('component-detail-left');
    var rightPanel = document.getElementById('component-detail-right');
    leftPanel.innerHTML = '';
    rightPanel.innerHTML = '';

    // --- Left panel ---

    // Header (still innerHTML — static, no interaction)
    var header = document.createElement('div');
    header.className = 'azd-component-header';
    header.innerHTML = '<h3>' + app.ui.esc(c.display_name) + '</h3>'
        + '<span class="azd-muted">' + app.ui.esc(c.qualified_name) + '</span>'
        + '<p>' + app.ui.esc(c.description || '') + '</p>';
    leftPanel.appendChild(header);

    // Badges (still innerHTML — static)
    var badges = document.createElement('div');
    badges.className = 'azd-badges';
    badges.innerHTML = '<span class="azd-badge">' + c.source + '</span>'
        + '<span class="azd-badge">' + c.child_policy + '</span>';
    leftPanel.appendChild(badges);

    // Data Model Editor (NEW — widget-based, interactive)
    var dmEditor = app.widgets.DataModelEditor.render({
        dataModel: c.data_model,
        enumModels: app.state.componentData.enum_models || [],
        structModels: app.state.componentData.struct_models || [],
        readOnly: !isEditable,
        mode: 'preview'
    }, {
        fieldValues: app.state.previewFieldValues || {},
        fieldSources: {}
    }, {
        onFieldChange: function(name, newVal) {
            app.state.previewFieldValues = app.state.previewFieldValues || {};
            app.state.previewFieldValues[name] = newVal;
            app.handlers._requestPreview();
        },
        onAddField: function() {
            var dialog = app.widgets.AddFieldDialog.render({
                enumModels: app.state.componentData.enum_models || [],
                structModels: app.state.componentData.struct_models || []
            }, {
                onConfirm: function(field) {
                    app.handlers._addFieldToComponent(c, field);
                },
                onCancel: function() {}
            });
            document.body.appendChild(dialog);
        },
        onRemoveField: function(name) {
            app.handlers._removeFieldFromComponent(c, name);
        }
    });
    leftPanel.appendChild(dmEditor);

    // CSS Editor (NEW — widget-based)
    var cssEditor = app.widgets.CssEditor.render({
        readOnly: !isEditable,
        dataModelFields: c.data_model.fields || []
    }, {
        css: c.scoped_css || '',
        errors: []
    }, {
        onChange: function(newCss) {
            app.handlers._requestPreview({ cssOverride: newCss });
        },
        onSave: function(newCss) {
            app.handlers._saveComponentCss(c, newCss);
        }
    });
    leftPanel.appendChild(cssEditor);

    // --- Right panel ---

    // Template (read-only, static)
    var templateSection = document.createElement('details');
    templateSection.innerHTML = '<summary>Template</summary>'
        + '<pre class="azd-code">' + app.ui.esc(c.template || 'No template') + '</pre>';
    rightPanel.appendChild(templateSection);

    // Preview Panel (NEW — widget-based)
    var preview = app.widgets.PreviewPanel.render({
        componentName: c.qualified_name
    }, {
        screenshotBase64: null,
        os: app.state.previewContext ? app.state.previewContext.os : 'macos',
        theme: app.state.previewContext ? app.state.previewContext.theme : 'light',
        language: app.state.previewContext ? app.state.previewContext.language : 'en-US',
        loading: false
    }, {
        onContextChange: function(ctx) {
            app.state.previewContext = Object.assign(app.state.previewContext || {}, ctx);
            app.handlers._requestPreview();
        }
    });
    rightPanel.appendChild(preview);
}
```

---

## New Handlers in `app.handlers`

```javascript
// Debounced preview request
app.handlers._previewTimer = null;
app.handlers._requestPreview = function(opts) {
    clearTimeout(app.handlers._previewTimer);
    app.handlers._previewTimer = setTimeout(function() {
        var c = app.state.componentData.components[app.state.selectedComponentIdx];
        var body = {
            op: 'preview_component',
            library: app.state.selectedLibrary,
            component: c.tag,
            field_values: app.state.previewFieldValues || {},
            dynamic_selector_context: app.state.previewContext || {}
        };
        if (opts && opts.cssOverride) body.css_override = opts.cssOverride;

        app.api.post(body, function(res) {
            if (res.status === 'ok') {
                // Update preview image
                var img = document.querySelector('.azd-preview-img');
                if (img && res.data.value.screenshot_base64) {
                    img.src = 'data:image/png;base64,' + res.data.value.screenshot_base64;
                }
            }
        });
    }, 150);
};

// Add field to component
app.handlers._addFieldToComponent = function(component, field) {
    var newFields = (component.data_model.fields || []).slice();
    newFields.push(field);
    app.api.post({
        op: 'update_component',
        library: app.state.selectedLibrary,
        name: component.tag,
        data_model: { fields: newFields }
    }, function() {
        app.handlers.selectLibrary(app.state.selectedLibrary);
    });
};

// Remove field from component
app.handlers._removeFieldFromComponent = function(component, fieldName) {
    var newFields = (component.data_model.fields || []).filter(function(f) {
        return f.name !== fieldName;
    });
    app.api.post({
        op: 'update_component',
        library: app.state.selectedLibrary,
        name: component.tag,
        data_model: { fields: newFields }
    }, function() {
        app.handlers.selectLibrary(app.state.selectedLibrary);
    });
};
```

---

## File Organization

### Option A: Single file (recommended)

Keep everything in `debugger.js`, add `app.widgets` namespace after `app.json`:

```
app.config      — constants
app.state       — global state
app.schema      — API command definitions
app.init        — initialization
app.api         — HTTP requests
app.ui          — HTML utilities (esc, etc.)
app.json        — JSON tree widget (existing)
app.widgets     — NEW: type-aware field editors
app.handlers    — event handlers (updated)
app.runner      — E2E test runner
app.resizer     — panel resizing
```

**Why**: the debugger is a self-contained tool embedded in the Rust binary
via `include_str!`. A single file avoids multi-file embedding complexity.
The `app.json` precedent already shows this works well.

### CSS additions

All new widget styles go in `debugger.css` under a `/* === Component Widgets === */`
section. Prefix all classes with `azd-` (azul-debugger) to avoid conflicts.

---

## Migration Path

### Step 1: Add `app.widgets` skeleton

Add the empty namespace and utility functions. All existing code continues working.

### Step 2: Implement W1-W3 (FieldEditor, TypeBadge, FieldInput)

These are the core building blocks. Test with:
```javascript
var el = app.widgets.FieldEditor.render(
    { name: 'test', fieldType: { type: 'String' }, readOnly: false },
    { value: 'hello' },
    { onChange: function(n, v) { console.log(n, v); } }
);
document.body.appendChild(el);
```

### Step 3: Implement W6-W7 (CssEditor, PreviewPanel)

Needed for the preview feature. Depends on the `preview_component` API
endpoint (Plan 1, Phase 6).

### Step 4: Refactor `showComponentDetail()`

Replace innerHTML blocks with widget calls. Old component list rendering
and library selection code remains unchanged — only the detail view changes.

### Step 5: Implement W4-W5 (ValueSourceToggle, BindingInput)

Needed for the Application Composition View. Can be deferred until the
composition view is built.

### Step 6: Implement W8-W10 (DataModelEditor, AddFieldDialog, ComponentDragHandle)

Full editing support for user-defined components.

---

## What NOT to Build

- **No reactive framework** — the manual render/update pattern works fine for
  this use case. React/Vue/Preact add dependencies and build complexity.
  The `render()` + `update()` contract is sufficient.

- **No virtual DOM** — widgets are created/destroyed on navigation, not
  diffed. The component detail view is re-rendered on selection change.
  For field edits, only the preview image updates (no DOM diff needed).

- **No ES modules** — everything in one file, IIFE-style. The debugger is
  embedded as a string in the Rust binary via `include_str!`.

- **No TypeScript** — the debugger JS stays vanilla. Type safety comes from
  the structured `ComponentFieldType` JSON schema, not from a compile step.

---

## Reuse Across Views

| Widget | Component View | Inspector View | App State View |
|---|---|---|---|
| `FieldEditor` | ✅ data model fields | ✅ node properties | ❌ |
| `TypeBadge` | ✅ field types | ✅ property types | ❌ |
| `FieldInput` | ✅ field values | ✅ override values | ❌ |
| `CssEditor` | ✅ scoped CSS | ❌ | ❌ |
| `PreviewPanel` | ✅ component preview | ❌ (has Inspector preview) | ❌ |
| `ValueSourceToggle` | ✅ composition mode | ❌ | ❌ |
| `BindingInput` | ✅ composition mode | ❌ | ❌ |
| `DataModelEditor` | ✅ full data model | ❌ | ❌ |
| `ComponentDragHandle` | ✅ sidebar list | ✅ insert node panel | ❌ |
| `AddFieldDialog` | ✅ add field | ❌ | ❌ |
| `app.json.render` | ❌ | ❌ | ✅ JSON tree |

The `FieldEditor` and `TypeBadge` widgets are the most reusable — they can
also serve the Inspector view when it needs to show CSS property types or
node attribute types.
