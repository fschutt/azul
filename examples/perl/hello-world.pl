# Full-GUI Perl hello-world: counter label + "Increase counter" button.
#
#   perl -Ilib hello-world.pl        (requires FFI::Platypus 2.x)
#
# Callbacks route through Azul.pm's host-invoker layer: register a Perl sub
# with Azul::register_callback(<Kind>, $sub) to get the matching Az<Kind>
# record, and wrap the data model with Azul::refany_create($model). The
# per-kind invoker fires the sub and memcpy's its return value back through
# libazul's out-pointer (see Azul::_writeback). We build the DOM with the raw
# Azul::FFI::* record calls -- exactly the by-value struct plumbing that the
# static thunks expect -- so no idiomatic-wrapper destructors run on the
# moved-out structs.

use strict;
use warnings;
use FindBin qw($Bin);
use lib "$Bin/lib";
use Azul;
use FFI::Platypus::Buffer qw(scalar_to_buffer);

# AzString from a Perl string. AzString_fromUtf8 copies the bytes, so the
# transient buffer is fine.
sub mk_str {
    my ($s) = @_;
    my ($ptr, $len) = scalar_to_buffer($s);
    return Azul::FFI::AzString_fromUtf8($ptr, $len);
}

# Shared data model. Any Perl ref works; refany_create pins it in the handle
# table until libazul's releaser fires. Because it's a ref, every RefAny that
# wraps it observes the same counter.
my $model = { counter => 5 };

# ButtonOnClick: bump the counter, ask for a DOM refresh.
#   $data = *const AzRefAny (raw pointer), $info = *const CallbackInfo.
# Return AzUpdate (the invoker packs it into the out-pointer as a C int).
my $on_click = sub {
    my ($data, $info) = @_;
    my $m = Azul::refany_get($data);
    return Azul::AzUpdate::DoNothing() unless defined $m;
    $m->{counter}++;
    return Azul::AzUpdate::RefreshDom();
};

# Layout: build body > [ div.font-size-32 > text(counter), button ].
# Returns a raw AzDom record; the invoker memcpy's it into the out-pointer.
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

# Install the registered LayoutCallback (host-handle cb + ctx) into
# window_state.layout_callback. The raw C _create() takes a bare fn-ptr and
# drops the host-handle ctx, so we must splice the whole Az<Kind> struct into
# the embedded slot. We CANNOT rely on FFI::Platypus record accessors for the
# nested offset: the Perl codegen currently mis-sizes tagged unions (e.g.
# AzOptionRefAny -> a 260-byte blob vs the real ~32), so every Perl record that
# embeds a union has wrong field offsets. Instead we locate the field's REAL
# C-ABI byte offset by planting a sentinel fn-ptr via _create() and scanning
# the returned struct for it, then overwrite that slot with the real callback
# bytes. This is layout-bug-proof: it uses libazul's own byte layout.
my $wco = Azul::FFI::AzWindowCreateOptions_default();
{
    no warnings 'portable';               # the 64-bit sentinel literal is fine on LP64
    my $SENTINEL = 0x0123456789ABCDEF;    # improbable-as-real-data pointer value
    my $probe = Azul::FFI::AzWindowCreateOptions_create($SENTINEL);
    my $off = index($$probe, pack('Q', $SENTINEL));
    die "could not locate layout_callback slot" if $off < 0;
    # Real AzLayoutCallback = fn-ptr(8) + AzOptionRefAny(32) = 40 bytes; the
    # host-handle record's first 40 bytes are exactly that cb + ctx.
    my $LC_SIZE = 40;
    substr($$wco, $off, $LC_SIZE) = substr($$layout_cb, 0, $LC_SIZE);
}

my $app = Azul::FFI::AzApp_create($app_data, Azul::FFI::AzAppConfig_create());
# AzApp_run takes *mut App: hand it a pointer into the live record buffer
# (scalar_to_buffer aliases $$app's storage, which stays valid while $app does).
my ($app_ptr, $app_len) = scalar_to_buffer($$app);
Azul::FFI::AzApp_run($app_ptr, $wco);
