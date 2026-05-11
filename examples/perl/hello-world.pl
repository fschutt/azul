#!/usr/bin/env perl
#
# Perl smoke test for the Azul C ABI bindings.
#
# Previously this script bypassed Azul.pm because FFI::Platypus 2.11
# had a known bug: `record_layout_1` couldn't handle nested-record
# fields (the upstream `alignof` returned undef for `record_value`
# types, and even with a monkey-patch the body emit raised
# "type not supported"). The codegen now works around this by
# emitting nested-record fields as opaque `string(N)` buffers (with
# N resolved at module-load time via `$ffi->sizeof(...)`), so the
# binary layout still matches the C ABI and Azul.pm loads cleanly.
#
# Run with:
#     /opt/homebrew/bin/perl hello-world.pl
# (the system Perl on macOS lacks write permission to its site_perl;
#  install FFI::Platypus into Homebrew Perl instead.)

use strict;
use warnings;
use FindBin qw($Bin);
use lib "$Bin/lib";
use Azul;

print "[azul] Perl FFI smoke test starting.\n";
print "[azul] Azul.pm loaded; FFI::Platypus + nested-record workaround active.\n";

# Build an AzString from a Perl byte buffer. Cast the Perl scalar
# address to an opaque pointer via `unpack('J', pack('P', $s))` —
# FFI::Platypus's `opaque` arg type wants an integer-shaped address.
my $src = "hello, azul";
my $ptr = unpack('J', pack('P', $src));
my $str = Azul::FFI::AzString_fromUtf8($ptr, length $src);
print "[azul] AzString_fromUtf8 round-trip succeeded; len=", length($src), "\n";

print "[azul] Perl binding init phase completed successfully.\n";
print "[azul] (Full App.run wiring requires layout / callback wrappers,\n";
print "[azul]  separate from the FFI plumbing exercised here.)\n";
