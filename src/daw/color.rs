// Written by Jürgen Moßgraber - mossgrabers.de
// (c) 2017-2026
// Licensed under LGPLv3 - http://www.gnu.org/licenses/lgpl-3.0.txt

//! Colors: the RGB color type [`ColorEx`] and the DAW color palette [`DawColor`].

use std::cell::Cell;

/// An RGB color with components in the range of [0..1] (former `ColorEx`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ColorEx {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
}

const FACTOR: f64 = 0.7;
const FACTOR2: f64 = 0.4;

const fn rgb(red: u8, green: u8, blue: u8) -> ColorEx {
    ColorEx { red: red as f64 / 255.0, green: green as f64 / 255.0, blue: blue as f64 / 255.0 }
}

const fn even_darker_const(c: ColorEx) -> ColorEx {
    ColorEx { red: c.red * FACTOR2, green: c.green * FACTOR2, blue: c.blue * FACTOR2 }
}

impl ColorEx {
    pub const BLACK: ColorEx = rgb(0, 0, 0);
    pub const WHITE: ColorEx = rgb(255, 255, 255);
    pub const LIGHT_GRAY: ColorEx = rgb(182, 182, 182);
    pub const GRAY: ColorEx = rgb(128, 128, 128);
    pub const DARK_GRAY: ColorEx = rgb(89, 89, 89);
    pub const DARKER_GRAY: ColorEx = rgb(63, 63, 63);
    pub const RED: ColorEx = rgb(255, 0, 0);
    pub const DARK_RED: ColorEx = rgb(128, 0, 0);
    pub const DARKER_RED: ColorEx = rgb(32, 0, 0);
    pub const GREEN: ColorEx = rgb(0, 255, 0);
    pub const DARK_GREEN: ColorEx = rgb(0, 128, 0);
    pub const DARKER_GREEN: ColorEx = rgb(0, 63, 0);
    pub const BLUE: ColorEx = rgb(0, 0, 255);
    pub const DARK_BLUE: ColorEx = rgb(0, 0, 128);
    pub const DARKER_BLUE: ColorEx = rgb(0, 0, 32);
    pub const YELLOW: ColorEx = rgb(255, 255, 0);
    pub const DARK_YELLOW: ColorEx = rgb(128, 128, 0);
    pub const DARKER_YELLOW: ColorEx = rgb(32, 32, 0);
    pub const ORANGE: ColorEx = rgb(255, 80, 0);
    pub const DARK_ORANGE: ColorEx = rgb(128, 65, 0);
    pub const DARKER_ORANGE: ColorEx = rgb(32, 16, 0);
    pub const PINK: ColorEx = rgb(255, 0, 220);
    pub const DARK_PINK: ColorEx = even_darker_const(Self::PINK);
    pub const ROSE: ColorEx = rgb(255, 127, 127);
    pub const BROWN: ColorEx = rgb(183, 73, 0);
    pub const DARK_BROWN: ColorEx = rgb(127, 0, 0);
    pub const MINT: ColorEx = rgb(170, 240, 209);
    pub const DARK_MINT: ColorEx = even_darker_const(Self::MINT);
    pub const OLIVE: ColorEx = rgb(128, 128, 0);
    pub const SKY_BLUE: ColorEx = rgb(97, 238, 255);
    pub const PURPLE: ColorEx = rgb(116, 80, 164);
    pub const DARK_PURPLE: ColorEx = even_darker_const(Self::PURPLE);
    pub const RED_WINE: ColorEx = rgb(123, 42, 57);
    pub const CYAN: ColorEx = rgb(0, 255, 255);

    pub const fn new(red: f64, green: f64, blue: f64) -> Self {
        ColorEx { red, green, blue }
    }

    pub fn from_array(color: [f64; 3]) -> Self {
        ColorEx::new(color[0], color[1], color[2])
    }

    pub const fn from_rgb(red: i32, green: i32, blue: i32) -> Self {
        ColorEx { red: red as f64 / 255.0, green: green as f64 / 255.0, blue: blue as f64 / 255.0 }
    }

    pub fn from_hsb(hue: f32, saturation: f32, brightness: f32) -> Self {
        let c = |v: f32| (v * 255.0 + 0.5) as i32;
        if saturation == 0.0 {
            let value = c(brightness);
            return Self::from_rgb(value, value, value);
        }
        let h = (hue - hue.floor()) * 6.0;
        let f = h - h.floor();
        let p = brightness * (1.0 - saturation);
        let q = brightness * (1.0 - saturation * f);
        let t = brightness * (1.0 - saturation * (1.0 - f));
        match h as i32 {
            0 => Self::from_rgb(c(brightness), c(t), c(p)),
            1 => Self::from_rgb(c(q), c(brightness), c(p)),
            2 => Self::from_rgb(c(p), c(brightness), c(t)),
            3 => Self::from_rgb(c(p), c(q), c(brightness)),
            4 => Self::from_rgb(c(t), c(p), c(brightness)),
            5 => Self::from_rgb(c(brightness), c(p), c(q)),
            _ => Self::BLACK,
        }
    }

    pub fn red(&self) -> f64 {
        self.red
    }

    pub fn green(&self) -> f64 {
        self.green
    }

    pub fn blue(&self) -> f64 {
        self.blue
    }

    pub fn to_int_rgb255(&self) -> [i32; 3] {
        [(self.red * 255.0).round() as i32, (self.green * 255.0).round() as i32, (self.blue * 255.0).round() as i32]
    }

    pub fn to_int_rgb127(&self) -> [i32; 3] {
        [(self.red * 127.0).round() as i32, (self.green * 127.0).round() as i32, (self.blue * 127.0).round() as i32]
    }

    pub fn to_double_rgb(&self) -> [f64; 3] {
        [self.red, self.green, self.blue]
    }

    /// Convert to hue, saturation and brightness (all in [0..1]).
    pub fn to_hsb(&self) -> [f32; 3] {
        let [r, g, b] = self.to_int_rgb255();
        let cmax = r.max(g).max(b);
        let cmin = r.min(g).min(b);
        let brightness = cmax as f32 / 255.0;
        let saturation = if cmax != 0 { (cmax - cmin) as f32 / cmax as f32 } else { 0.0 };
        let hue = if saturation == 0.0 {
            0.0
        } else {
            let d = (cmax - cmin) as f32;
            let redc = (cmax - r) as f32 / d;
            let greenc = (cmax - g) as f32 / d;
            let bluec = (cmax - b) as f32 / d;
            let mut hue = if r == cmax {
                bluec - greenc
            } else if g == cmax {
                2.0 + redc - bluec
            } else {
                4.0 + greenc - redc
            };
            hue /= 6.0;
            if hue < 0.0 {
                hue += 1.0;
            }
            hue
        };
        [hue, saturation, brightness]
    }

    pub fn brighter(c: ColorEx) -> ColorEx {
        let (mut r, mut g, mut b) = (c.red, c.green, c.blue);
        let i = 1.0 / (1.0 - FACTOR) / 255.0;
        if r == 0.0 && g == 0.0 && b == 0.0 {
            return ColorEx::new(i, i, i);
        }
        if r > 0.0 && r < i {
            r = i;
        }
        if g > 0.0 && g < i {
            g = i;
        }
        if b > 0.0 && b < i {
            b = i;
        }
        ColorEx::new((r / FACTOR).min(1.0), (g / FACTOR).min(1.0), (b / FACTOR).min(1.0))
    }

    pub fn darker(c: ColorEx) -> ColorEx {
        ColorEx::new((c.red * FACTOR).max(0.0), (c.green * FACTOR).max(0.0), (c.blue * FACTOR).max(0.0))
    }

    pub fn even_darker(c: ColorEx) -> ColorEx {
        ColorEx::new((c.red * FACTOR2).max(0.0), (c.green * FACTOR2).max(0.0), (c.blue * FACTOR2).max(0.0))
    }

    pub fn dim(&self, hue: f64) -> ColorEx {
        ColorEx::new(self.red * hue, self.green * hue, self.blue * hue)
    }

    pub fn dim_to_gray(c: ColorEx) -> ColorEx {
        if c.red != c.green || c.green != c.blue {
            let v = (c.red + c.green + c.blue) / 3.0;
            return Self::even_darker(ColorEx::new(v, v, v));
        }
        Self::even_darker(c)
    }

    pub fn scale(&self, brightness: f64, saturation: f64) -> ColorEx {
        let hsb = self.to_hsb();
        Self::from_hsb(hsb[0], (1.0f64).min(hsb[1] as f64 * 2.0 * saturation) as f32, (hsb[2] as f64 * brightness).max(0.1) as f32)
    }

    pub fn closest_color(color: ColorEx, palette: &[ColorEx]) -> ColorEx {
        palette[Self::closest_color_index(color, palette)]
    }

    pub fn closest_color_index(color: ColorEx, palette: &[ColorEx]) -> usize {
        Self::closest_color_index_with(color, palette, true)
    }

    pub fn closest_color_index_with(color: ColorEx, palette: &[ColorEx], use_color_metric: bool) -> usize {
        let mut min_error = 5.0;
        let mut closest = 0;
        for (i, p) in palette.iter().enumerate() {
            let error = Self::calc_distance(*p, color, use_color_metric);
            if error < min_error {
                closest = i;
                min_error = error;
            }
        }
        closest
    }

    pub fn calc_distance(color1: ColorEx, color2: ColorEx, use_color_metric: bool) -> f64 {
        let dr = color1.red - color2.red;
        let dg = color1.green - color2.green;
        let db = color1.blue - color2.blue;
        if use_color_metric {
            let rmean = (color1.red + color2.red) / 2.0;
            return ((2.0 + rmean) * dr * dr + 4.0 * dg * dg + (2.99609375 - rmean) * db * db).sqrt();
        }
        dr.abs() + dg.abs() + db.abs()
    }

    pub fn calc_contrast_color(c: ColorEx) -> ColorEx {
        let l = 0.2126 * c.red + 0.7152 * c.green + 0.0722 * c.blue;
        if l > 0.179 {
            ColorEx::BLACK
        } else {
            ColorEx::WHITE
        }
    }

    pub fn encode(&self) -> i32 {
        let c = self.to_int_rgb255();
        c[0] + (c[1] << 8) + (c[2] << 16)
    }

    pub fn decode(encoded: i32) -> ColorEx {
        Self::from_rgb(encoded & 0xFF, (encoded >> 8) & 0xFF, (encoded >> 16) & 0xFF)
    }

    pub fn is_grayscale(&self) -> bool {
        self.red == self.green && self.green == self.blue
    }
}

macro_rules! daw_colors {
    ($($variant:ident = ($id:literal, $name:literal, $r:expr, $g:expr, $b:expr)),* $(,)?) => {
        /// The color palette of the DAW (former `DAWColor`).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum DawColor { $($variant),* }

        impl DawColor {
            pub const ALL: &'static [DawColor] = &[$(DawColor::$variant),*];

            /// The ID of the color as used in the color managers (the Java enum name).
            pub fn id(self) -> &'static str {
                match self { $(DawColor::$variant => $id),* }
            }

            /// The display name of the color.
            pub fn name(self) -> &'static str {
                match self { $(DawColor::$variant => $name),* }
            }

            pub fn color(self) -> ColorEx {
                match self { $(DawColor::$variant => ColorEx::new($r, $g, $b)),* }
            }
        }
    };
}

daw_colors! {
    ColorOff = ("COLOR_OFF", "Off", 0.5, 0.5, 0.5),
    DarkGray = ("DAW_COLOR_DARK_GRAY", "Dark Gray", 0.3294117748737335, 0.3294117748737335, 0.3294117748737335),
    Gray = ("DAW_COLOR_GRAY", "Gray", 0.47843137383461, 0.47843137383461, 0.47843137383461),
    GrayHalf = ("DAW_COLOR_GRAY_HALF", "Gray half", 0.5, 0.5, 0.5),
    LightGray = ("DAW_COLOR_LIGHT_GRAY", "Light Gray", 0.7882353067398071, 0.7882353067398071, 0.7882353067398071),
    Silver = ("DAW_COLOR_SILVER", "Silver", 0.5254902243614197, 0.5372549295425415, 0.6745098233222961),
    DarkBrown = ("DAW_COLOR_DARK_BROWN", "Dark Brown", 0.6392157077789307, 0.4745098054409027, 0.26274511218070984),
    Brown = ("DAW_COLOR_BROWN", "Brown", 0.7764706015586853, 0.6235294342041016, 0.43921568989753723),
    DarkBlue = ("DAW_COLOR_DARK_BLUE", "Dark Blue", 0.34117648005485535, 0.3803921639919281, 0.7764706015586853),
    PurpleBlue = ("DAW_COLOR_PURPLE_BLUE", "Purplish Blue", 0.5176470875740051, 0.5411764979362488, 0.8784313797950745),
    Purple = ("DAW_COLOR_PURPLE", "Purple", 0.5843137502670288, 0.2862745225429535, 0.7960784435272217),
    Pink = ("DAW_COLOR_PINK", "Pink", 0.8509804010391235, 0.21960784494876862, 0.4431372582912445),
    Red = ("DAW_COLOR_RED", "Red", 0.8509804010391235, 0.18039216101169586, 0.1411764770746231),
    Orange = ("DAW_COLOR_ORANGE", "Orange", 1.0, 0.34117648005485535, 0.0235294122248888),
    LightOrange = ("DAW_COLOR_LIGHT_ORANGE", "Light Orange", 0.8509804010391235, 0.615686297416687, 0.062745101749897),
    MossGreen = ("DAW_COLOR_MOSS_GREEN", "Moss Green", 0.26274511218070984, 0.8235294222831726, 0.7254902124404907),
    Green = ("DAW_COLOR_GREEN", "Green", 0.45098039507865906, 0.5960784554481506, 0.0784313753247261),
    ColdGreen = ("DAW_COLOR_COLD_GREEN", "Cold Green", 0.0, 0.615686297416687, 0.27843138575553894),
    Blue = ("DAW_COLOR_BLUE", "Blue", 0.2666666805744171, 0.7843137383460999, 1.0),
    LightPurple = ("DAW_COLOR_LIGHT_PURPLE", "Light Purple", 0.7372549176216125, 0.4627451002597809, 0.9411764740943909),
    LightPink = ("DAW_COLOR_LIGHT_PINK", "Light Pink", 0.8823529481887817, 0.4000000059604645, 0.5686274766921997),
    Rose = ("DAW_COLOR_ROSE", "Rose", 0.9254902005195618, 0.3803921639919281, 0.34117648005485535),
    ReddishBrown = ("DAW_COLOR_REDDISH_BROWN", "Redish Brown", 1.0, 0.5137255191802979, 0.24313725531101227),
    LightBrown = ("DAW_COLOR_LIGHT_BROWN", "Light Brown", 0.8941176533699036, 0.7176470756530762, 0.30588236451148987),
    LightGreen = ("DAW_COLOR_LIGHT_GREEN", "Light Green", 0.6274510025978088, 0.7529411911964417, 0.2980392277240753),
    BluishGreen = ("DAW_COLOR_BLUISH_GREEN", "Bluish Green", 0.0, 0.6509804129600525, 0.5803921818733215),
    GreenBlue = ("DAW_COLOR_GREEN_BLUE", "Greenish Blue", 0.24313725531101227, 0.7333333492279053, 0.3843137323856354),
    LightBlue = ("DAW_COLOR_LIGHT_BLUE", "Light Blue", 0.0, 0.6000000238418579, 0.8509804010391235),
}

const NEW_TRACK_COLORS: [DawColor; 9] = [
    DawColor::Purple,
    DawColor::Pink,
    DawColor::Red,
    DawColor::Orange,
    DawColor::LightOrange,
    DawColor::MossGreen,
    DawColor::Green,
    DawColor::ColdGreen,
    DawColor::Blue,
];

thread_local! {
    static NEW_TRACK_COLOR: Cell<DawColor> = const { Cell::new(DawColor::DarkBlue) };
}

impl DawColor {
    /// Look up a color by its ID (e.g. "DAW_COLOR_RED").
    pub fn from_id(id: &str) -> Option<DawColor> {
        Self::ALL.iter().copied().find(|c| c.id() == id)
    }

    /// Get the color of the entry with the given ID, `COLOR_OFF` if it does not exist.
    pub fn color_entry(id: &str) -> ColorEx {
        Self::from_id(id).unwrap_or(DawColor::ColorOff).color()
    }

    /// Get the color of the entry at the given index, `COLOR_OFF` if out of range.
    pub fn color_entry_at(index: i32) -> ColorEx {
        let i = if index >= 0 && (index as usize) < Self::ALL.len() { index as usize } else { 0 };
        Self::ALL[i].color()
    }

    /// Get the palette entry which is closest to the given color.
    pub fn closest(color: ColorEx) -> DawColor {
        let mut cid = Self::ALL[0];
        let mut min_error = 5.0;
        for c in &Self::ALL[1..] {
            let error = ColorEx::calc_distance(c.color(), color, true);
            if error < min_error {
                cid = *c;
                min_error = error;
            }
        }
        cid
    }

    /// Get the ID of the palette entry which is closest to the given color (former `getColorID`).
    pub fn color_id(color: ColorEx) -> &'static str {
        Self::closest(color).id()
    }

    /// Get the next color to use for a new track (cycles through a list of colors).
    pub fn next_color() -> DawColor {
        NEW_TRACK_COLOR.with(|current| {
            let color = current.get();
            let position = NEW_TRACK_COLORS.iter().position(|c| *c == color).map(|p| p + 1).unwrap_or(0);
            let position = if position >= NEW_TRACK_COLORS.len() { 0 } else { position };
            current.set(NEW_TRACK_COLORS[position]);
            color
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_ids() {
        assert_eq!(DawColor::color_id(DawColor::Red.color()), "DAW_COLOR_RED");
        assert_eq!(DawColor::from_id("DAW_COLOR_BLUE"), Some(DawColor::Blue));
        assert_eq!(ColorEx::decode(ColorEx::RED.encode()), ColorEx::RED);
        let hsb = ColorEx::RED.to_hsb();
        assert_eq!(ColorEx::from_hsb(hsb[0], hsb[1], hsb[2]), ColorEx::RED);
    }
}
