#!/usr/bin/env python3
"""Which NeverLift-classified functions are ADDRESS-TAKEN?

A NeverLift function is not lifted, so an indirect call to it can only trap. That
is correct when the function is genuinely unreachable on a web path (COM drop
glue, native file dialogs, GL context loaders) and a bug when it is not - so
after adding or widening a NeverLift rule, check what it removed.

Reads ONE run's log: `intercept: sub_X -> synth=0xY class=Some(NeverLift)` marks
an address-taken function whose class is NeverLift; the `resolved=` lines name
it. Splits the result by which rule is responsible, so a new rule's damage is
separated from the long-standing panic family.

Usage: neverlift-address-taken.py <server.log>
"""
import io
import re
import sys
import collections

log = sys.argv[1]

INT = re.compile(r'intercept: sub_([0-9a-f]+)\s*(?:→|->)\s*synth=0x([0-9a-f]+)\s+class=Some\((\w+)\)')
RES = re.compile(r'sub_([0-9a-f]+)\s*(?:→|->)\s*resolved=(.+?)@0x([0-9a-fA-F]+)\s+class=(\S+)')

name_of = {}
never = set()
for line in io.open(log, encoding='utf-8', errors='replace'):
    m = RES.search(line)
    if m:
        try:
            s = int(m.group(1), 16)
        except ValueError:
            continue
        name_of[s] = m.group(2).strip()
        if m.group(4).startswith('NeverLift'):
            never.add(s)
    m = INT.search(line)
    if m and m.group(3) == 'NeverLift':
        try:
            never.add(int(m.group(2), 16))
        except ValueError:
            pass

# The rules I added, mirrored from symbol_table.rs
PLATFORM = ['Win32Window', 'X11Window', 'XlibWindow', 'WaylandWindow', 'MacWindow',
            'NSWindow', 'CocoaWindow', 'AppKitWindow',
            'shell2::windows::', 'shell2::x11::', 'shell2::wayland::',
            'shell2::macos::', 'shell2::appkit::', 'shell2::cocoa::',
            'windows::Win32::', 'windows::UI::', 'windows::Media::',
            'windows::Graphics::', 'windows::Foundation::', 'windows::ApplicationModel::',
            'windows_core::', 'objc::', 'objc2::', 'cocoa::', 'core_foundation::']
EXCLUDED = {'turso_core', 'turso_parser', 'turso_ext', 'turso_macros', 'regex',
            'regex_automata', 'regex_syntax', 'aho_corasick', 'accesskit',
            'accesskit_windows', 'accesskit_macos', 'accesskit_unix', 'rustls',
            'webpki', 'ring', 'der', 'spki', 'pkcs8', 'ash', 'gl_context_loader',
            'glutin', 'khronos_egl', 'gilrs', 'gilrs_core', 'tinyfiledialogs',
            'tfd', 'x11_clipboard', 'brotli', 'brotli_decompressor',
            'iced_x86', 'goblin'}
PANIC = re.compile(r'panicking|begin_panic|rust_begin_unwind|handle_alloc_error|'
                   r'capacity_overflow|assert_failed|expect_failed|unwrap_failed|'
                   r'panic_bounds_check|_fail$|_fail::|panic_fmt|panic_nounwind|'
                   r'already_borrowed|slice_.*_fail|panic_access_error')


def crate_of(n):
    lead = n.lstrip('_<')
    out = []
    for ch in lead:
        if ch.isalnum() or ch == '_':
            out.append(ch)
        else:
            break
    return ''.join(out)


mine_platform, mine_excluded, panic_family, other = [], [], [], []
for s in sorted(never):
    n = name_of.get(s)
    if not n:
        other.append((s, '(unresolved)'))
        continue
    if any(p in n for p in PLATFORM):
        mine_platform.append((s, n))
    elif crate_of(n) in EXCLUDED:
        mine_excluded.append((s, n))
    elif PANIC.search(n):
        panic_family.append((s, n))
    else:
        other.append((s, n))

print('log: %s' % log)
print('address-taken NeverLift functions: %d' % len(never))
print('')
print('  from is_platform_native (MINE)        : %d' % len(mine_platform))
print('  from is_browser_excluded_crate (MINE) : %d' % len(mine_excluded))
print('  pre-existing panic family             : %d' % len(panic_family))
print('  other / unresolved                    : %d' % len(other))
print('')
if mine_platform or mine_excluded:
    print('!! MY CLASSIFIERS REMOVED ADDRESS-TAKEN FUNCTIONS - each is a missing')
    print('   dispatcher case and a candidate for the boot trap:')
    for s, n in (mine_platform + mine_excluded)[:20]:
        print('   0x%08x  %s' % (s, n[:88]))
else:
    print('None of my classifiers removed an address-taken function.')
    print('The boot trap is NOT caused by is_platform_native or')
    print('is_browser_excluded_crate - look elsewhere.')
