"""Which Windows API is the unmatched dispatch PC?

`intercepted_import_labels` resolves each wanted API with GetModuleHandleA +
GetProcAddress IN THE HOST PROCESS and uses `addr & 0xFFFFFFFF` as the
dispatcher case label. System DLLs share a base across processes for a boot
session, so resolving them here yields the same labels the lift used.

The unmatched PC is 0xd5fddf90, which is outside the AzWriter image span and
therefore a raw IAT target. If it matches one of the WANTED entries, the
interception should have fired and did not - and the reason is one of the three
guards in that function (label collision with an existing case, the
is_synth_in_image_span filter, or the module not being loaded). If it matches
something NOT in the list, the list simply needs that entry.
"""
import ctypes
import ctypes.wintypes as w

TARGET = 0xd5fddf90

k32 = ctypes.WinDLL('kernel32', use_last_error=True)
k32.GetModuleHandleA.restype = w.HMODULE
k32.GetModuleHandleA.argtypes = [w.LPCSTR]
k32.GetProcAddress.restype = ctypes.c_void_p
k32.GetProcAddress.argtypes = [w.HMODULE, w.LPCSTR]
k32.LoadLibraryA.restype = w.HMODULE
k32.LoadLibraryA.argtypes = [w.LPCSTR]

# The exact WANTED list from intercepted_import_labels, plus a few near
# neighbours that a heap path could plausibly reach.
WANTED = [
    ('KERNEL32.DLL', 'GetProcessHeap'), ('KERNEL32.DLL', 'HeapAlloc'),
    ('KERNEL32.DLL', 'HeapFree'), ('KERNEL32.DLL', 'HeapReAlloc'),
    ('ntdll.dll', 'RtlAllocateHeap'), ('ntdll.dll', 'RtlFreeHeap'),
    ('ntdll.dll', 'RtlReAllocateHeap'),
    ('VCRUNTIME140.dll', 'memcmp'), ('VCRUNTIME140.dll', 'memcpy'),
    ('VCRUNTIME140.dll', 'memmove'), ('VCRUNTIME140.dll', 'memset'),
    ('ucrtbase.dll', 'trunc'), ('ucrtbase.dll', 'floor'),
    ('ucrtbase.dll', 'ceil'), ('ucrtbase.dll', 'fabs'),
    ('ucrtbase.dll', 'sqrt'), ('ucrtbase.dll', 'truncf'),
    ('ucrtbase.dll', 'floorf'), ('ucrtbase.dll', 'ceilf'),
    ('ucrtbase.dll', 'fabsf'), ('ucrtbase.dll', 'sqrtf'),
]
EXTRA = [
    ('KERNEL32.DLL', 'HeapSize'), ('KERNEL32.DLL', 'HeapDestroy'),
    ('KERNEL32.DLL', 'HeapCreate'), ('KERNEL32.DLL', 'HeapValidate'),
    ('ntdll.dll', 'RtlSizeHeap'), ('ntdll.dll', 'RtlpAllocateHeap'),
    ('KERNEL32.DLL', 'GetLastError'), ('KERNEL32.DLL', 'SetLastError'),
    ('KERNEL32.DLL', 'VirtualAlloc'), ('KERNEL32.DLL', 'VirtualFree'),
]


def resolve(dll, func):
    m = k32.GetModuleHandleA(dll.encode())
    if not m:
        m = k32.LoadLibraryA(dll.encode())
    if not m:
        return None
    p = k32.GetProcAddress(m, func.encode())
    return p or None


print('target PC (masked) = 0x%08x' % TARGET)
print('')
hit = []
for group, label in ((WANTED, 'WANTED'), (EXTRA, 'extra')):
    for dll, func in group:
        a = resolve(dll, func)
        if a is None:
            continue
        low = a & 0xFFFFFFFF
        mark = '   <<<< MATCH' if low == TARGET else ''
        if low == TARGET:
            hit.append((group is WANTED, dll, func, a))
        print('  %-7s %-18s %-20s full=0x%012x  low32=0x%08x%s'
              % (label, dll, func, a, low, mark))

print('')
if not hit:
    print('NO MATCH in either list.')
    print('The PC is some other imported function - identify it before adding')
    print('anything to WANTED.')
else:
    in_wanted, dll, func, a = hit[0]
    print('MATCH: %s!%s at 0x%012x' % (dll, func, a))
    if in_wanted:
        print('It IS in WANTED, so the interception should have emitted a case.')
        print('Check the three guards: label collision (`used.insert`),')
        print('is_synth_in_image_span, or the module not loaded at lift time.')
    else:
        print('It is NOT in WANTED - the list needs this entry.')
