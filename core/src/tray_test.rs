#[cfg(test)]
mod tests {
    use super::*;

    fn img(w: u32, h: u32) -> TrayIconImage {
        TrayIconImage::new(w, h, vec![0u8; (w * h * 4) as usize].into())
            .into_option()
            .expect("w*h*4 buffer must be accepted")
    }

    #[test]
    fn new_rejects_a_buffer_that_is_not_w_times_h_times_4() {
        assert!(TrayIconImage::new(2, 2, vec![0u8; 16].into()).is_some());
        assert!(TrayIconImage::new(2, 2, vec![0u8; 15].into()).is_none());
        assert!(TrayIconImage::new(2, 2, vec![0u8; 17].into()).is_none());
        // A zero dimension would make `expected` 0 and let an empty buffer
        // through, which every backend would then read as a valid icon.
        assert!(TrayIconImage::new(0, 4, U8Vec::from_const_slice(&[])).is_none());
        assert!(TrayIconImage::new(4, 0, U8Vec::from_const_slice(&[])).is_none());
    }

    #[test]
    fn argb32_be_reorders_rgba_to_a_r_g_b() {
        // One pixel: R=1 G=2 B=3 A=4  ->  A=4 R=1 G=2 B=3
        let i = TrayIconImage::new(1, 1, vec![1, 2, 3, 4].into())
            .into_option()
            .expect("1x1 RGBA is 4 bytes");
        assert_eq!(i.to_argb32_be().as_ref(), &[4u8, 1, 2, 3]);
    }

    #[test]
    fn best_icon_prefers_the_smallest_that_still_covers_the_target() {
        let d = TrayIconData {
            icon: TrayIconSource::Rgba(TrayIconImageVec::from_vec(vec![
                img(16, 16),
                img(32, 32),
                img(64, 64),
            ])),
            ..Default::default()
        };
        // Exact match wins.
        assert_eq!(d.best_icon(32).into_option().unwrap().width, 32);
        // Between sizes: scale DOWN from 32, never up from 16.
        assert_eq!(d.best_icon(20).into_option().unwrap().width, 32);
        // Larger than everything we have: take the biggest.
        assert_eq!(d.best_icon(256).into_option().unwrap().width, 64);
        // No icons at all is not a panic.
        assert!(TrayIconData::default().best_icon(16).is_none());
        // A named icon has no bitmap to pick from — it is rasterized at the
        // exact size instead, so best_icon must not invent one.
        assert!(TrayIconData::default()
            .with_named_icon(AzString::from_const_str("settings"))
            .best_icon(16)
            .is_none());
    }

    #[test]
    fn named_icon_stores_the_spec_verbatim() {
        // The spec is passed through untouched: parsing pack-qualification and
        // fallback lists is the icon registry's job, not the tray's, so that
        // `<icon>` and a tray icon can never disagree about what a spec means.
        for spec in ["settings", "mypack:logo", "missing:x, settings"] {
            let d = TrayIconData::new(
                AzString::from_const_str("org.example.app"),
                AzString::from_const_str("App"),
            )
            .with_named_icon(AzString::from(String::from(spec)));
            let TrayIconSource::Named(ref s) = d.icon else {
                panic!("expected a named icon source");
            };
            assert_eq!(s.as_str(), spec);
        }
    }

    #[test]
    fn sni_names_match_the_spec_spelling_exactly() {
        // These strings go on the wire; a typo silently breaks the item.
        assert_eq!(
            TrayCategory::ApplicationStatus.sni_name(),
            "ApplicationStatus"
        );
        assert_eq!(TrayCategory::SystemServices.sni_name(), "SystemServices");
        assert_eq!(TrayStatus::NeedsAttention.sni_name(), "NeedsAttention");
        assert_eq!(TrayStatus::Passive.sni_name(), "Passive");
    }
}
