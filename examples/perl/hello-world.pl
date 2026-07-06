# Full-GUI Perl hello-world: counter label + "Increase counter" button.
#   perl -Ilib hello-world.pl        (requires FFI::Platypus 2.x)
#
# Callbacks route through Azul.pm's host-invoker layer (register_callback).
# Build the DOM with raw Azul::FFI::* record calls so no idiomatic-wrapper
# destructors run on the moved-out by-value structs.

use strict;
use warnings;
use FindBin qw($Bin);
use lib "$Bin/lib";
use Azul;
use FFI::Platypus::Buffer qw(scalar_to_buffer);

# AzString from a Perl string (AzString_fromUtf8 copies the bytes).
sub mk_str {
    my ($s) = @_;
    my ($ptr, $len) = scalar_to_buffer($s);
    return Azul::FFI::AzString_fromUtf8($ptr, $len);
}

# Shared data model. refany_create pins the ref in the handle table until
# libazul's releaser fires; every RefAny wrapping it observes the same counter.
my $model = { counter => 5 };

# ButtonOnClick: bump the counter, ask for a DOM refresh. Returns AzUpdate.
my $on_click = sub {
    my ($data, $info) = @_;
    my $m = Azul::refany_get($data);
    return Azul::AzUpdate::DoNothing() unless defined $m;
    $m->{counter}++;
    return Azul::AzUpdate::RefreshDom();
};

# Layout: build body > [ div.font-size-32 > text(counter), button ].
my $layout = sub {
    my ($data, $info) = @_;
    my $m = Azul::refany_get($data);
    my $body = Azul::FFI::AzDom_createBody();
    return $body unless defined $m;

    my $label = Azul::FFI::AzDom_createDiv();
    $label = Azul::FFI::AzDom_withCss($label, mk_str('font-size: 32px;'));
    $label = Azul::FFI::AzDom_withChild(
        $label, Azul::FFI::AzDom_createText(mk_str($m->{counter})));

    my $click_cb   = Azul::register_callback('ButtonOnClickCallback', $on_click);
    my $click_data = Azul::refany_create($model);
    my $btn = Azul::FFI::AzButton_create(mk_str('Increase counter'));
    $btn = Azul::FFI::AzButton_withButtonType($btn, Azul::AzButtonType::Primary());
    $btn = Azul::FFI::AzButton_withOnClick($btn, $click_data, $click_cb);

    $body = Azul::FFI::AzDom_withChild($body, $label);
    $body = Azul::FFI::AzDom_withChild($body, Azul::FFI::AzButton_dom($btn));
    return $body;
};

print "[azul] Perl full-GUI hello-world starting.\n";

my $app_data  = Azul::refany_create($model);
my $layout_cb = Azul::register_callback('LayoutCallback', $layout);

# Splice the registered LayoutCallback (cb + ctx) into window_state; the raw C
# _create() takes a bare fn-ptr and drops the ctx. FFI::Platypus record offsets
# can't be trusted (the Perl codegen mis-sizes union-embedding records), so we
# find the slot's REAL C-ABI offset by planting a sentinel fn-ptr and scanning
# for it, then overwrite that slot with the real callback bytes.
my $wco = Azul::FFI::AzWindowCreateOptions_default();
{
    no warnings 'portable';               # the 64-bit sentinel literal is fine on LP64
    my $SENTINEL = 0x0123456789ABCDEF;    # improbable-as-real-data pointer value
    my $probe = Azul::FFI::AzWindowCreateOptions_create($SENTINEL);
    my $off = index($$probe, pack('Q', $SENTINEL));
    die "could not locate layout_callback slot" if $off < 0;
    # AzLayoutCallback = fn-ptr(8) + AzOptionRefAny(32) = 40 bytes.
    my $LC_SIZE = 40;
    substr($$wco, $off, $LC_SIZE) = substr($$layout_cb, 0, $LC_SIZE);
}

my $app = Azul::FFI::AzApp_create($app_data, Azul::FFI::AzAppConfig_create());
# AzApp_run takes *mut App: hand it a pointer into the live record buffer.
my ($app_ptr, $app_len) = scalar_to_buffer($$app);
Azul::FFI::AzApp_run($app_ptr, $wco);
