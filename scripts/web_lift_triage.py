import subprocess, re, sys
RL='/Users/fschutt/Development/azul/third_party/remill-install/build/remill/bin/lift/remill-lift-17'
DY='target/aarch64-apple-darwin/release/libazul.dylib'
names = """_ZN11azul_layout7solver32fc20position_table_cells17ha28674a1d28737b2E
_ZN11azul_layout7solver32fc39calculate_column_widths_auto_with_width17h1b9749f60b68b926E
_ZN11azul_layout7solver32fc39collect_and_measure_inline_content_impl17hb27f0d66c9295a0eE
_ZN5taffy7compute4grid12track_sizing22track_sizing_algorithm17hfdbaa36a61d3a9f4E
_ZN5taffy7compute7flexbox19compute_preliminary17h27f80bd3a70f2596E
_ZN101_$LT$allsorts_azul..tables..cmap..CmapSubtable$u20$as$u20$allsorts_azul..binary..read..ReadBinary$GT$4read17h483dd7411633a839E
_ZN102_$LT$core..iter..adapters..map..Map$LT$I$C$F$GT$$u20$as$u20$core..iter..traits..iterator..Iterator$GT$4fold17h1c0921102e868008E
_ZN102_$LT$core..iter..adapters..map..Map$LT$I$C$F$GT$$u20$as$u20$core..iter..traits..iterator..Iterator$GT$4fold17hb4ec69fad907b895E
_ZN103_$LT$allsorts_azul..woff2..TransformedGlyphTable$u20$as$u20$allsorts_azul..binary..read..ReadBinary$GT$4read17h33f1c1aced8c90c4E
_ZN111_$LT$alloc..vec..Vec$LT$T$GT$$u20$as$u20$alloc..vec..spec_from_iter_nested..SpecFromIterNested$LT$T$C$I$GT$$GT$9from_iter17h59f5c660d7bf7169E
_ZN111_$LT$alloc..vec..Vec$LT$T$GT$$u20$as$u20$alloc..vec..spec_from_iter_nested..SpecFromIterNested$LT$T$C$I$GT$$GT$9from_iter17hba1cf5e665265c2eE
_ZN11azul_layout6window12LayoutWindow20layout_dom_recursive17hbd026356c46edeb1E
_ZN11azul_layout7solver311positioning25adjust_relative_positions17h505334b49fe9edc4E
_ZN11azul_layout7solver311positioning35find_absolute_containing_block_rect17hd0a095250212cbb3E
_ZN11azul_layout7solver32fc29calculate_column_widths_fixed17h16ab0a4cf681bbfdE
_ZN15rust_fontconfig11FcFontCache14get_font_bytes17h274979300761d545E
_ZN4core5slice4sort6shared9smallsort12sort8_stable17hb4bbb22f586691caE
_ZN4core5slice4sort6shared9smallsort12sort8_stable17hfd09d56e3e55a252E
_ZN4core5slice4sort6shared9smallsort31small_sort_general_with_scratch17haf5060aca001832fE
_ZN5taffy7compute4grid5types5named26NamedLineResolver$LT$S$GT$3new17h5a138411d728fb7aE
_ZN5taffy7compute7flexbox29determine_container_main_size28_$u7b$$u7b$closure$u7d$$u7d$28_$u7b$$u7b$closure$u7d$$u7d$17h328d87e9e52456e9E
_ZN9azul_core7compact57_$LT$impl$u20$azul_core..prop_cache..CssPropertyCache$GT$42build_compact_cache_with_inheritance_debug17hadb7c865e1641b99E""".strip().split('\n')

# nm output: addr t _name  (one leading underscore added by macOS)
sym2addr = {}
addrs_sorted = []
for line in open('/tmp/azul_nm_sorted.txt'):
    parts = line.split()
    if len(parts) >= 3:
        try: a = int(parts[0], 16)
        except ValueError: continue
        sym2addr[parts[2]] = a
        addrs_sorted.append(a)
addrs_sorted = sorted(set(addrs_sorted))
import bisect
dy = open(DY,'rb').read()
real_gaps, benign = [], []
for n in names:
    mac = '_' + n
    a = sym2addr.get(mac)
    if a is None:
        print(f'?? no symbol: {n[:60]}'); continue
    i = bisect.bisect_right(addrs_sorted, a)
    b = addrs_sorted[i] if i < len(addrs_sorted) else a + 0x4000
    byts = dy[a:b].hex()
    r = subprocess.run([RL,'--arch','aarch64','--address',hex(a),'--bytes',byts,'--ir_out','/tmp/t.ll'],
                       capture_output=True, text=True, timeout=300)
    decode_errs = sorted(set(re.findall(r'\] (Unrecognized[^\n]*|Unable[^\n]*|Could not decode[^\n]*)', r.stderr)))
    nerr = open('/tmp/t.ll').read().count('@__remill_error(')
    short = n.split('17h')[0][-50:]
    if decode_errs:
        real_gaps.append((short, hex(a), decode_errs))
        print(f'REAL  {short} @{hex(a)} err_sites={nerr}')
        for d in decode_errs[:3]: print(f'      {d}')
    else:
        benign.append(short)
        print(f'ok    {short} @{hex(a)} err_sites={nerr} (no decode error -> noreturn-tail)')
print(f'\n==> {len(real_gaps)} real decode gaps, {len(benign)} benign')
