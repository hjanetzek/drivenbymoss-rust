// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The paths of the RGB colors to the palettes of all controllers (see `framework::color`): on
//! Bitwig the DAW color table as before, otherwise the nearest palette entry.

use crate::config::push::PushVersion;
use crate::daw::color::{ColorEx, DawColor};
use crate::framework::color::{ColorManager, ColorSource};

/// The color managers of all controllers.
fn all_color_managers() -> Vec<(&'static str, ColorManager)> {
    vec![
        ("Beatstep", super::arturia::beatstep::colors::create_color_manager()),
        ("Launchpad", super::launchpad::colors::create_color_manager()),
        ("OSC", super::osc::colors::create_color_manager()),
        ("Push 1", super::push::colors::create_color_manager(PushVersion::Version1)),
        ("Push 2", super::push::colors::create_color_manager(PushVersion::Version2)),
        ("Push 3", super::push::colors::create_color_manager(PushVersion::Version3)),
        ("APC40", super::akai::apc::colors::create_color_manager(false)),
        ("APC40 mkII", super::akai::apc::colors::create_color_manager(true)),
        ("APCmini", super::akai::apcmini::colors::create_mk1_color_manager()),
        ("APCmini mk2", super::akai::apcmini::colors::create_mk2_color_manager()),
        ("ACVS", super::akai::acvs::colors::create_color_manager()),
        ("Fire", super::akai::fire::colors::create_color_manager()),
        ("MCU", super::mackie::mcu::setup::create_color_manager()),
        ("Electra", super::electra::colors::create_color_manager()),
        ("Maschine", super::maschine::core::colors::create_color_manager()),
        ("Launch Control", super::launchcontrol::colors::create_color_manager()),
        ("Launchkey", super::launchkey::colors::create_color_manager(false)),
        ("Launchkey Mini", super::launchkey::colors::create_color_manager(true)),
    ]
}

/// Some RGB colors: the DAW colors, a grid through the RGB cube and colors of Electra presets.
fn test_colors() -> Vec<ColorEx> {
    let mut colors: Vec<ColorEx> = DawColor::ALL.iter().map(|c| c.color()).collect();
    for r in (0..=255).step_by(51) {
        for g in (0..=255).step_by(51) {
            for b in (0..=255).step_by(51) {
                colors.push(ColorEx::from_rgb(r, g, b));
            }
        }
    }
    colors.extend([0x03A598, 0xF49500, 0x529DEC, 0xC44795].map(|c| ColorEx::from_rgb(c >> 16, (c >> 8) & 0xFF, c & 0xFF)));
    colors
}

#[test]
fn host_decides_the_source() {
    assert_eq!(ColorSource::for_host("Bitwig Studio 5.3.13"), ColorSource::Bitwig);
    assert_eq!(ColorSource::for_host("Bitwig Studio 6.1"), ColorSource::Bitwig);
    assert_eq!(ColorSource::for_host("zynthian"), ColorSource::Direct);
    assert_eq!(ColorSource::for_host("DrivenByMoss reference host"), ColorSource::Direct);
    assert_eq!(ColorSource::for_host("7.22/linux-x86_64"), ColorSource::Direct);
    assert_eq!(ColorSource::for_host(""), ColorSource::Direct);
    // The default of a color manager (and of the tests without an app) is Bitwig
    assert_eq!(ColorManager::new().color_source(), ColorSource::Bitwig);
}

/// On Bitwig every RGB color takes the path of before (the closest DAW color, then its table
/// entry), for every controller: the conformance runs and replays stay the same.
#[test]
fn bitwig_path_is_unchanged() {
    for (name, cm) in all_color_managers() {
        assert_eq!(cm.color_source(), ColorSource::Bitwig);
        assert!(!cm.is_direct(), "{name}");
        for color in test_colors() {
            let old_id = DawColor::color_id(color);
            assert_eq!(cm.color_id(color), old_id, "{name}: {color:?}");
            if cm.has_color_index(old_id) {
                assert_eq!(cm.index_for_color(color), cm.color_index(old_id), "{name}: {color:?}");
            }
        }
    }
}

/// The controllers with a palette table take the nearest palette entry on other hosts; the
/// others keep the DAW color table.
#[test]
fn direct_path_where_there_is_a_palette() {
    let with_palette = ["Launchpad", "Push 1", "Push 2", "Push 3", "APC40 mkII", "Maschine", "Launchkey", "Launchkey Mini"];
    for (name, mut cm) in all_color_managers() {
        assert_eq!(cm.has_palette_rgb(), with_palette.contains(&name), "{name}");
        cm.set_color_source(ColorSource::Direct);
        for color in test_colors() {
            if with_palette.contains(&name) {
                let index = cm.nearest_palette_index(color).unwrap();
                assert_eq!(cm.index_for_color(color), index, "{name}: {color:?}");
                assert_ne!(index, 0, "{name}: an RGB color is never off");
            } else {
                assert_eq!(cm.color_id(color), DawColor::color_id(color), "{name}: {color:?}");
            }
        }
    }
}

/// NI's palette: hue * 4 + level (hue 17 and 72.. are white and the greys).
fn ni_hue(index: i32) -> i32 {
    if index >= 68 {
        17
    } else {
        index / 4
    }
}

fn rgb(hex: i32) -> ColorEx {
    ColorEx::from_rgb(hex >> 16, (hex >> 8) & 0xFF, hex & 0xFF)
}

#[test]
fn ni_palette_nearest() {
    use super::maschine::core::colors::*;
    let mut cm = create_color_manager();
    cm.set_color_source(ColorSource::Direct);
    let index = |hex: i32| cm.index_for_color(rgb(hex));
    // Orange stays orange (hue 2/3), not the lime green 22 of the DAW color table
    // (F49500 -> LightOrange -> COLOR_YELLOW)
    assert!(matches!(ni_hue(index(0xF49500)), 2 | 3), "{}", index(0xF49500));
    assert_eq!(cm.color_index(DawColor::color_id(rgb(0xF49500))), COLOR_YELLOW);
    // Yellow is NI's yellow 18 (255, 224, 0), not 22 (96, 255, 0)
    assert_eq!(index(0xFFFF00), 18);
    assert_eq!(index(0xFFE000), 18);
    // The greys stay grey
    for grey in [0x202020, 0x404040, 0x606060, 0x808080, 0xA0A0A0, 0xC0C0C0, 0xFFFFFF] {
        assert_eq!(ni_hue(index(grey)), 17, "{grey:06X}: {}", index(grey));
        assert_ne!(index(grey), 71, "{grey:06X}: not the pale cyan");
    }
    // Black (an existing item) is a dark grey, not off
    assert_eq!(ni_hue(index(0x000000)), 17);
    // Dark colors keep their hue
    assert!(matches!(ni_hue(index(0x640000)), 1 | 16), "{}", index(0x640000));
    assert_eq!(ni_hue(index(0x00005A)), 11);
    assert_eq!(ni_hue(index(0x783C00)), 2);
    assert_eq!(ni_hue(index(0x003000)), 7);
    // Colors of Electra presets
    assert_eq!(ni_hue(index(0x03A598)), 9, "turquoise");
    assert_eq!(ni_hue(index(0x529DEC)), 10, "sky blue");
    assert_eq!(ni_hue(index(0xC44795)), 14, "magenta pink");
    assert_eq!(index(0xFF0000), COLOR_RED);
    assert_eq!(index(0x0000FF), 46);
}

#[test]
fn ni_dim_keeps_the_hue() {
    use super::maschine::core::colors::*;
    let mut cm = create_color_manager();
    // Bitwig: Java's formula
    assert_eq!(dim_or_highlight_color(&cm, DawColor::Red.color(), false), 5);
    cm.set_color_source(ColorSource::Direct);
    assert_eq!(dim_or_highlight_color(&cm, rgb(0xFFFF00), true), 18);
    assert_eq!(dim_or_highlight_color(&cm, rgb(0xFFFF00), false), 17, "the medium yellow, not the green 21");
    assert_eq!(dim_or_highlight_color(&cm, rgb(0x808080), false), COLOR_DARK_GREY);
    assert_eq!(dim_or_highlight_color(&cm, rgb(0x303030), true), COLOR_WHITE);
}

#[test]
fn velocity_palette_nearest() {
    let mut cm = super::launchpad::colors::create_color_manager();
    cm.set_color_source(ColorSource::Direct);
    let index = |hex: i32| cm.index_for_color(rgb(hex));
    let palette = super::push::colors::DEFAULT_PALETTE;
    let hue = |i: i32| {
        let [r, g, b] = palette[i as usize];
        ColorEx::from_rgb(r, g, b).to_hsb()
    };
    // Orange: an orange entry (hue about 30 degrees), not the yellow of LightOrange
    let orange = hue(index(0xF49500));
    assert!((0.05..0.13).contains(&orange[0]) && orange[1] > 0.8, "{} {orange:?}", index(0xF49500));
    assert_eq!(index(0xFFFF00), 13);
    for grey in [0x202020, 0x404040, 0x808080, 0xC0C0C0, 0xFFFFFF] {
        assert!(hue(index(grey))[1] < 0.15, "{grey:06X}: {}", index(grey));
    }
    let turquoise = hue(index(0x03A598));
    assert!((0.42..0.53).contains(&turquoise[0]), "{} {turquoise:?}", index(0x03A598));
    // Push 2/3 have the DAW colors at 70..96: a DAW color is found exactly
    let mut push2 = super::push::colors::create_color_manager(PushVersion::Version2);
    push2.set_color_source(ColorSource::Direct);
    for (i, color) in DawColor::ALL.iter().enumerate().skip(1) {
        let index = push2.index_for_color(color.color());
        assert_eq!(super::push::colors::palette_color(index).to_int_rgb255(), color.color().to_int_rgb255(), "{color:?} {i}");
    }
}
