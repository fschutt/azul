#!/usr/bin/env python3
"""Generate css/src/codegen/lower_types.rs from api.json.

The generated file holds the EXHAUSTIVE lowering of every api.json type that is
reachable from `CssProperty`, `CssPropertyWithConditions(Vec)` and `Css` into the
codegen IR (`css/src/codegen/ir.rs`):

  * one `impl Lower for <type>` per struct / enum / Vec (no `_` match arms, so a
    new variant in the Rust source is a compile error in the generated file);
  * `lower_css_property`, one match arm per `CssProperty` variant, which knows
    the api.json constructor (`CssProperty::width`) and the monomorphized value
    alias (`LayoutWidthValue`) of each variant.

The special cases (primitives, `String`, `PixelValue`/`FloatValue`/
`PercentageValue` sugar, `CssPropertyValue<T>`, `BoxOrStatic<T>`, `FontRef`,
`GridMinMax`, `CssPropertyWithConditions`) are hand-written in
`css/src/codegen/lower.rs`.

Run from the repo root:   python3 css/tools/gen_codegen_lowering.py
It also cross-checks api.json against the Rust definitions under css/src and
prints (and exits 1 on) any mismatch that would not compile.
"""
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, '..', '..'))
API = os.path.join(REPO, 'api.json')
SRC = os.path.join(REPO, 'css', 'src')
OUT = os.path.join(SRC, 'codegen', 'lower_types.rs')

ROOTS = ['CssProperty', 'CssPropertyWithConditions', 'CssPropertyWithConditionsVec', 'Css']
PRIMS = {'u8', 'u16', 'u32', 'u64', 'usize', 'i8', 'i16', 'i32', 'i64', 'isize', 'f32', 'f64', 'bool', 'char'}

# Hand-written in lower.rs (or never lowered on their own).
HAND_WRITTEN = {
    'CssProperty',                 # lower_css_property below + `impl Lower` in lower.rs
    'CssPropertyValue',            # generic, lowered through prop_value()
    'BoxOrStatic',                 # generic pointer wrapper
    'String', 'U8Vec',             # Expr::Str
    'FontRef', 'FontRefDestructorCallbackType',   # opaque runtime handle
    'GridMinMax',                  # two raw pointers + a destructor flag
    'PixelValue', 'FloatValue', 'PercentageValue',   # constructor sugar
    'CssPropertyWithConditions',   # `simple` / `on_hover` / ... sugar
    'c_void', 'T',
}


def load_api():
    d = json.load(open(API))
    d = d[sorted(d.keys())[-1]]['api']
    allc = {}
    for mod, mv in d.items():
        for c, cv in mv.get('classes', {}).items():
            allc[c] = (mod, cv)
    return allc


ALLC = load_api()


def is_vec(t, cv):
    return t.endswith('Vec') and any('ptr' in f for f in (cv.get('struct_fields') or []))


def vec_elem(cv):
    for f in cv['struct_fields']:
        if 'ptr' in f:
            return f['ptr']['type']
    raise KeyError


def closure(roots):
    seen = {}
    stack = list(roots)
    while stack:
        t = stack.pop()
        if t in seen or t in PRIMS:
            continue
        if t not in ALLC:
            seen[t] = None
            continue
        seen[t] = ALLC[t]
        _, cv = ALLC[t]
        if is_vec(t, cv):
            stack.append(vec_elem(cv))
            continue
        for f in cv.get('struct_fields') or []:
            for _, v in f.items():
                stack.append(v['type'])
        for vm in cv.get('enum_fields') or []:
            for _, v in vm.items():
                if 'type' in v:
                    stack.append(v['type'])
        ta = cv.get('type_alias')
        if ta:
            stack.append(ta['target'])
            stack += ta.get('generic_args', [])
    return seen


def rust_path(cv):
    ext = cv.get('external')
    if not ext:
        raise SystemExit(f'no external path for {cv}')
    return 'crate::' + ext.split('::', 1)[1] if ext.startswith('azul_css::') else ext


def enum_variants(cv):
    return [(k, v.get('type')) for vm in cv['enum_fields'] for k, v in vm.items()]


def struct_fields(cv):
    return [(k, v) for f in cv['struct_fields'] for k, v in f.items()]


# ------------------------------------------------------------ rust cross-check
def strip_comments(s):
    s = re.sub(r'//[^\n]*', '', s)
    return re.sub(r'/\*.*?\*/', '', s, flags=re.S)


_RS = None


def rs_files():
    global _RS
    if _RS is None:
        _RS = {}
        for dp, _, fs in os.walk(SRC):
            if os.sep + 'codegen' in dp:
                continue
            for f in fs:
                if f.endswith('.rs'):
                    p = os.path.join(dp, f)
                    _RS[p] = strip_comments(open(p).read())
    return _RS


def split_top(body):
    out, depth, cur = [], 0, ''
    for c in body:
        if c in '{(<[':
            depth += 1
        elif c in '})>]':
            depth -= 1
        if c == ',' and depth == 0:
            out.append(cur)
            cur = ''
        else:
            cur += c
    if cur.strip():
        out.append(cur)
    return out


def strip_attrs(s):
    s = s.strip()
    while s.startswith('#['):
        depth = 0
        for i, c in enumerate(s):
            if c == '[':
                depth += 1
            elif c == ']':
                depth -= 1
                if depth == 0:
                    s = s[i + 1:].strip()
                    break
    return s


def rust_def(name):
    pat = re.compile(r'pub\s+(enum|struct)\s+' + re.escape(name) + r'\b\s*(<[^>{]*>)?\s*(\{|\(|;)')
    for p, s in rs_files().items():
        m = pat.search(s)
        if not m:
            continue
        kind, opener = m.group(1), m.group(3)
        if opener == ';':
            return kind, []
        start = m.end() - 1
        depth, i = 0, start
        while i < len(s):
            if s[i] in '{([':
                depth += 1
            elif s[i] in '})]':
                depth -= 1
                if depth == 0:
                    break
            i += 1
        body = s[start + 1:i]
        parts = [strip_attrs(x) for x in split_top(body)]
        parts = [x for x in parts if x]
        if kind == 'enum':
            vs = []
            for part in parts:
                m2 = re.match(r'(\w+)\s*(.*)$', part, re.S)
                rest = m2.group(2).strip()
                arity = 0
                if rest.startswith('('):
                    arity = len([x for x in split_top(rest[1:rest.rfind(')')]) if x.strip()])
                elif rest.startswith('{'):
                    arity = -1
                vs.append((m2.group(1), arity))
            return kind, vs
        fs = []
        for part in parts:
            m2 = re.match(r'(pub(\([^)]*\))?\s+)?(\w+)\s*:', part)
            fs.append((m2.group(3), (m2.group(1) or '').strip()) if m2 else ('?', ''))
        return kind, fs
    return None  # macro-generated (define_*_property!, impl_option!, ...): trust api.json


def cross_check(types):
    errors = []
    for t in types:
        _, cv = ALLC[t]
        d = rust_def(t)
        if d is None:
            continue
        kind, items = d
        if cv.get('enum_fields') is not None:
            av = enum_variants(cv)
            if kind != 'enum':
                errors.append(f'{t}: api.json enum, Rust {kind}')
                continue
            if [a for a, _ in av] != [r for r, _ in items]:
                errors.append(f'{t}: variants differ\n  api  {[a for a, _ in av]}\n  rust {[r for r, _ in items]}')
                continue
            for (an, at), (rn, ar) in zip(av, items):
                if (at is None) != (ar == 0) or (at is not None and ar != 1):
                    errors.append(f'{t}::{an}: api payload {at}, Rust arity {ar}')
        else:
            af = struct_fields(cv)
            if kind != 'struct':
                errors.append(f'{t}: api.json struct, Rust {kind}')
                continue
            if [a for a, _ in af] != [r for r, _ in items]:
                errors.append(f'{t}: fields differ\n  api  {[a for a, _ in af]}\n  rust {[r for r, _ in items]}')
                continue
            for (rn, vis) in items:
                if not vis.startswith('pub'):
                    errors.append(f'{t}.{rn}: private field (hand-write it in lower.rs)')
    return errors


# ------------------------------------------------------------------- emitters
def shape_of(cv):
    vs = enum_variants(cv)
    return 'EnumShape::CLike' if all(v is None for _, v in vs) else 'EnumShape::Tagged'


def gen_enum(t, cv):
    path = rust_path(cv)
    vs = enum_variants(cv)
    out = [f'impl Lower for {path} {{', '    fn lower(&self) -> Expr {']
    if all(v is None for _, v in vs):
        out.append('        let variant = match self {')
        for n, _ in vs:
            out.append(f'            Self::{n} => "{n}",')
        out.append('        };')
        out.append(f'        Expr::unit("{t}", EnumShape::CLike, variant)')
    else:
        out.append('        match self {')
        for n, ty in vs:
            if ty is None:
                out.append(f'            Self::{n} => Expr::unit("{t}", EnumShape::Tagged, "{n}"),')
            else:
                out.append(f'            Self::{n}(v) => Expr::variant("{t}", EnumShape::Tagged, "{n}", vec![v.lower()]),')
        out.append('        }')
    out += ['    }', '}', '']
    return out


def gen_struct(t, cv):
    path = rust_path(cv)
    fs = struct_fields(cv)
    out = [f'impl Lower for {path} {{', '    fn lower(&self) -> Expr {']
    if not fs:
        out.append(f'        Expr::strukt("{t}", Vec::new())')
    else:
        out.append(f'        Expr::strukt(')
        out.append(f'            "{t}",')
        out.append('            vec![')
        for n, v in fs:
            out.append(f'                ("{n}", self.{n}.lower()),')
        out.append('            ],')
        out.append('        )')
    out += ['    }', '}', '']
    return out


def gen_vec(t, cv):
    path = rust_path(cv)
    elem = vec_elem(cv)
    return [
        f'impl Lower for {path} {{',
        '    fn lower(&self) -> Expr {',
        f'        Expr::vec("{t}", "{elem}", self.as_slice().iter().map(Lower::lower).collect())',
        '    }',
        '}',
        '',
    ]


def snake(s):
    return re.sub(r'(?<!^)(?=[A-Z])', '_', s).lower()


def gen_css_property():
    _, cp = ALLC['CssProperty']
    ctors = cp.get('constructors') or {}
    out = [
        '/// Lower one [`CssProperty`](crate::props::property::CssProperty).',
        '///',
        '/// Exhaustive on purpose: a property added to the Rust enum is a compile',
        '/// error here until api.json (and this generated file) know it.',
        '#[allow(clippy::too_many_lines)]',
        'pub(crate) fn lower_css_property(p: &crate::props::property::CssProperty) -> Expr {',
        '    use crate::props::property::CssProperty as P;',
        '    match p {',
    ]
    for n, alias in enum_variants(cp):
        _, acv = ALLC[alias]
        inner = acv['type_alias']['generic_args'][0]
        s = snake(n)
        ctor = None
        if s in ctors:
            arg = list(ctors[s]['fn_args'][0].values())[0]
            ctor = s
            # the constructor takes the unwrapped value (BoxOrStatic<T> -> T)
            if arg != inner and not inner.startswith('BoxOrStatic'):
                raise SystemExit(f'CssProperty::{s} takes {arg}, variant holds {inner}')
        if inner.startswith('BoxOrStatic') and ctor is None:
            out.append(f'        P::{n}(v) => prop_boxed_without_ctor(v, "{n}", "{alias}", "{inner}"),')
        else:
            c = f'Some("{ctor}")' if ctor else 'None'
            out.append(f'        P::{n}(v) => prop_value(v, "{n}", "{alias}", "{inner}", {c}),')
    out += ['    }', '}', '']
    return out


def gen_api_modules(cl):
    names = sorted(set(t for t, v in cl.items() if v is not None) | {'CssPropertyType'})
    out = [
        '/// The api.json module of every type the lowering can produce, sorted by',
        '/// name (binary-searchable). Printers whose bindings are split into',
        '/// modules (Rust `azul::css`, `azul::vec`, ...) import from it.',
        'pub(crate) static API_MODULES: &[(&str, &str)] = &[',
    ]
    for t in names:
        out.append(f'    ("{t}", "{ALLC[t][0]}"),')
    c_like = sorted(
        t for t, v in cl.items()
        if v is not None and v[1].get('enum_fields') is not None and not v[1].get('generic_params')
        and all('type' not in vv for vm in v[1]['enum_fields'] for vv in vm.values())
    )
    out += [
        '];',
        '',
        '/// Every C-like (unit-only) enum the lowering can produce, sorted.',
        'pub(crate) static C_LIKE_ENUMS: &[&str] = &[',
    ]
    out += [f'    "{t}",' for t in c_like]
    css_prop_variants = [k for k, _ in enum_variants(ALLC['CssProperty'][1])]
    out += [
        '];',
        '',
        '/// `CssProperty` variants in declaration order: the index is the C tag',
        '/// (bindings that build a `CssProperty` union by hand need it).',
        'pub(crate) static CSS_PROPERTY_VARIANTS: &[&str] = &[',
    ]
    out += [f'    "{v}",' for v in css_prop_variants]
    out += [
        '];',
        '',
        '/// The C tag of a variant of a tagged union the printers build by hand:',
        '/// `CssProperty` variants by declaration order, `CssPropertyValue<T>`',
        '/// aliases Auto=0 None=1 Initial=2 Inherit=3 Revert=4 Unset=5 Exact=6.',
        '#[must_use]',
        'pub fn union_tag(ty: &str, variant: &str) -> Option<usize> {',
        '    if ty == "CssProperty" {',
        '        return CSS_PROPERTY_VARIANTS.iter().position(|v| *v == variant);',
        '    }',
        '    ["Auto", "None", "Initial", "Inherit", "Revert", "Unset", "Exact"]',
        '        .iter()',
        '        .position(|v| *v == variant)',
        '}',
        '',
        '/// `true` if `ty` is a C-like enum (a plain C enum; some bindings spell',
        '/// those without the `Az` prefix or pass them as integers).',
        '#[must_use]',
        'pub fn is_c_like_enum(ty: &str) -> bool {',
        '    C_LIKE_ENUMS.binary_search(&ty).is_ok()',
        '}',
        '',
        '/// The api.json module of `ty` (`"css"`, `"vec"`, `"option"`, `"str"`, ...).',
        '#[must_use]',
        'pub fn api_module(ty: &str) -> Option<&\'static str> {',
        '    API_MODULES',
        '        .binary_search_by(|(name, _)| (*name).cmp(ty))',
        '        .ok()',
        '        .map(|i| API_MODULES[i].1)',
        '}',
        '',
    ]
    return out


def main():
    cl = closure(ROOTS)
    unknown = [t for t, v in cl.items() if v is None and t not in HAND_WRITTEN]
    if unknown:
        raise SystemExit(f'types missing from api.json: {unknown}')
    todo = []
    for t in sorted(cl):
        v = cl[t]
        if v is None or t in HAND_WRITTEN:
            continue
        _, cv = v
        if 'type_alias' in cv or cv.get('generic_params'):
            continue
        todo.append(t)
    errs = cross_check([t for t in todo if not is_vec(t, ALLC[t][1])])
    if errs:
        print('\n'.join(errs))
        sys.exit(1)

    lines = [
        '// @generated by css/tools/gen_codegen_lowering.py from api.json - DO NOT EDIT BY HAND.',
        '// Regenerate after an api.json change:  python3 css/tools/gen_codegen_lowering.py',
        '//',
        '// Exhaustive lowering of every api.json type reachable from CssProperty,',
        '// CssPropertyWithConditions(Vec) and Css into the codegen IR. The special',
        '// cases live in lower.rs (see HAND_WRITTEN in the generator).',
        '',
        '#![allow(clippy::too_many_lines, clippy::match_same_arms, clippy::enum_glob_use)]',
        '',
        'use alloc::vec::Vec;',
        '',
        'use super::{',
        '    ir::{EnumShape, Expr},',
        '    lower::{prop_boxed_without_ctor, prop_value, Lower},',
        '};',
        '',
    ]
    lines += gen_css_property()
    lines += gen_api_modules(cl)
    n = {'enum': 0, 'struct': 0, 'vec': 0}
    for t in todo:
        _, cv = ALLC[t]
        if is_vec(t, cv):
            lines += gen_vec(t, cv)
            n['vec'] += 1
        elif cv.get('enum_fields') is not None:
            lines += gen_enum(t, cv)
            n['enum'] += 1
        else:
            lines += gen_struct(t, cv)
            n['struct'] += 1
    open(OUT, 'w').write('\n'.join(lines).rstrip() + '\n')
    print(f'wrote {os.path.relpath(OUT, REPO)}: {n} + lower_css_property')


if __name__ == '__main__':
    main()
