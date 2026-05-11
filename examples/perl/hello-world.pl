#!/usr/bin/env perl
#
# Minimal Perl smoke test for the Azul C ABI bindings.
#
# ── Why this is a smoke test, not the full Azul app ─────────────────
# The auto-generated `Azul.pm` (target/codegen/Azul.pm) lays out every
# struct as a `FFI::Platypus::Record`. Records that reference other
# records as fields (e.g. `AzDpiScaleFactor.inner = AzFloatValue`)
# trigger an `alignof undef` regression in FFI::Platypus's
# `record_layout_1`: even though `AzFloatValue` is registered with
# its body before `AzDpiScaleFactor` runs, `$type->alignof` comes back
# undef, so loading Azul.pm fails with:
#   Use of uninitialized value $align in numeric gt (>) at
#   .../FFI/Platypus/Record.pm line 93.
#   Illegal modulus zero at .../FFI/Platypus/Record.pm line 96.
#
# That's an upstream-broken issue in FFI::Platypus 2.11 (current
# stable on CPAN), independent of azul-doc. Documented in commit
# eb27a1d56. We skip loading Azul.pm and exercise the binding via
# a hand-rolled FFI::Platypus instance with primitive-only signatures
# so the smoke test still proves linkage works.
#
# Run with:
#     /opt/homebrew/bin/perl hello-world.pl
# (the system Perl on macOS lacks write permission to its site_perl;
#  install FFI::Platypus into Homebrew Perl instead.)

use strict;
use warnings;
use FindBin qw($Bin);
use FFI::Platypus 2.00;

print "[azul] Perl FFI smoke test starting.\n";

my $libname = -e "$Bin/libazul.dylib" ? "$Bin/libazul.dylib"
            : -e "$Bin/libazul.so"    ? "$Bin/libazul.so"
            : 'azul';

my $ffi = FFI::Platypus->new(api => 2);
$ffi->lib($libname);

print "[azul] FFI library loaded ($libname).\n";

# AzString is a Vec<u8> wrapper. We pass it by pointer to avoid
# struct-by-value marshalling. The buffer holds an AzString-sized
# blob; AzString_fromUtf8 fills it; AzString_delete frees the heap
# allocation it points at. This matches the same pattern the Haskell
# / PHP smoke tests use.
$ffi->attach('AzString_fromUtf8' => ['opaque', 'size_t'] => 'opaque' => sub {
    my ($xsub, $bytes, $len) = @_;
    return $xsub->($bytes, $len);
});

$ffi->attach('AzString_delete' => ['opaque'] => 'void' => sub {
    my ($xsub, $ptr) = @_;
    $xsub->($ptr);
});

print "[azul] FFI symbol resolution succeeded (AzString_fromUtf8 / _delete reachable).\n";
print "[azul] Perl binding init phase completed successfully.\n";
print "[azul] (Full app wiring requires the Azul.pm record load —\n";
print "[azul]  FFI::Platypus 2.11 has an upstream alignof regression that\n";
print "[azul]  prevents that today. Smoke test exercises raw FFI only.)\n";
