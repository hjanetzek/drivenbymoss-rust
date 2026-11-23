// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! The paths of the RGB colors to the palettes of all controllers (see `framework::color`): on
//! Bitwig the DAW color table as before, otherwise the nearest palette entry.

use crate::config::push::PushVersion;
use crate::daw::color::{ColorEx, DawColor};
use crate::framework::color::{ColorManager, ColorRole, ColorSource};

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
                let index = cm.palette_index_for_role(color, ColorRole::Normal).unwrap();
                assert_eq!(cm.index_for_color(color), index, "{name}: {color:?}");
                assert_ne!(index, 0, "{name}: an RGB color is never off");
            } else {
                assert_eq!(cm.color_id(color), DawColor::color_id(color), "{name}: {color:?}");
            }
        }
    }
}

fn rgb(hex: i32) -> ColorEx {
    ColorEx::from_rgb(hex >> 16, (hex >> 8) & 0xFF, hex & 0xFF)
}

#[test]
fn ni_palette_hue_by_color_level_by_role() {
    use super::maschine::core::colors::*;
    use ColorRole::*;
    let mut cm = create_color_manager();
    assert!(cm.has_role_levels());
    cm.set_color_source(ColorSource::Direct);
    let roles = |hex: i32| [Disabled, Inactive, Normal, Highlight].map(|role| cm.index_for_color_role(rgb(hex), role));
    // The hue from the color, the level from the role: dim, medium, full, pale
    assert_eq!(roles(0xF49500), [12, 13, 14, 15], "orange stays orange (hue 3), not the lime green 22 of the DAW color table");
    assert_eq!(cm.color_index(DawColor::color_id(rgb(0xF49500))), COLOR_YELLOW);
    assert_eq!(roles(0xFFFF00), [16, 17, 18, 19], "yellow is NI's yellow (hue 4), not 22");
    assert_eq!(roles(0x03A598), [36, 37, 38, 39], "teal (hue 9)");
    assert_eq!(roles(0x529DEC), [40, 41, 42, 43], "sky blue (hue 10)");
    assert_eq!(roles(0xC44795), [56, 57, 58, 59], "magenta pink (hue 14)");
    assert_eq!(roles(0x640000), [4, 5, 6, 7], "a dark red is red at the level of its role");
    assert_eq!(roles(0xFF0000), [4, 5, 6, 7]);
    assert_eq!(roles(0x0000FF), [44, 45, 46, 47]);
    // Achromatic colors: the whites (dark grey, grey, white, white; all one white on the JAM)
    for grey in [0x808080, 0x202020, 0x000000, 0xFFFFFF, 0x8C8080] {
        assert_eq!(roles(grey), [COLOR_DARK_GREY, COLOR_GREY, COLOR_WHITE, 79], "{grey:06X}");
    }
    // index_for_color is the normal role: the full level of the hue
    assert_eq!(cm.index_for_color(rgb(0x03A598)), 38);
    for hex in [0x00005A, 0x783C00, 0x003000, 0x123456, 0xFEDCBA, 0x7F3F1F] {
        assert_eq!(cm.index_for_color(rgb(hex)) % 4, 2, "{hex:06X}: full");
    }
    assert_eq!(cm.index_for_color(rgb(0x00005A)), 46);
    assert_eq!(cm.index_for_color(rgb(0x783C00)), 14);
    assert_eq!(cm.index_for_color(rgb(0x003000)), 30);
    // Bitwig's track colors on other hosts: the hue of the Java table for most of them
    assert_eq!(cm.index_for_color(DawColor::Red.color()), COLOR_RED);
    assert_eq!(cm.index_for_color(DawColor::Blue.color()), COLOR_BLUE);
    assert_eq!(cm.index_for_color(DawColor::Pink.color()), COLOR_PINK);
    assert_eq!(cm.index_for_color(DawColor::Brown.color()), COLOR_AMBER);
    assert_eq!(cm.index_for_color(DawColor::Silver.color()), COLOR_WHITE);
    // The roles are ignored on the Bitwig path (as Java)
    cm.set_color_source(ColorSource::Bitwig);
    for role in [Disabled, Inactive, Normal, Highlight] {
        assert_eq!(cm.index_for_color_role(rgb(0xF49500), role), COLOR_YELLOW);
    }
}

/// The 128-colour palettes are no hue × level palette: the nearest entry, whatever the role.
#[test]
fn velocity_palette_ignores_the_role() {
    let mut cm = super::launchpad::colors::create_color_manager();
    assert!(!cm.has_role_levels());
    cm.set_color_source(ColorSource::Direct);
    for hex in [0xF49500, 0x03A598, 0x808080] {
        let nearest = cm.nearest_palette_index(rgb(hex)).unwrap();
        for role in [ColorRole::Disabled, ColorRole::Inactive, ColorRole::Normal, ColorRole::Highlight] {
            assert_eq!(cm.index_for_color_role(rgb(hex), role), nearest);
        }
    }
}

#[test]
fn ni_dim_keeps_the_hue() {
    use super::maschine::core::colors::*;
    let mut cm = create_color_manager();
    // Bitwig: Java's formula
    assert_eq!(dim_or_highlight_color(&cm, DawColor::Red.color(), false), 5);
    assert_eq!(dim_or_highlight_color(&cm, DawColor::Red.color(), true), COLOR_RED);
    assert_eq!(dim_or_highlight_color(&cm, DawColor::DarkGray.color(), true), COLOR_WHITE);
    // Other hosts: selected full, the others medium (the inactive role)
    cm.set_color_source(ColorSource::Direct);
    assert_eq!(dim_or_highlight_color(&cm, rgb(0xFFFF00), true), 18);
    assert_eq!(dim_or_highlight_color(&cm, rgb(0xFFFF00), false), 17, "the medium yellow, not the green 21");
    assert_eq!(dim_or_highlight_color(&cm, rgb(0x640000), true), COLOR_RED);
    assert_eq!(dim_or_highlight_color(&cm, rgb(0x640000), false), COLOR_RED_LO);
    assert_eq!(dim_or_highlight_color(&cm, rgb(0x808080), false), COLOR_GREY);
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

