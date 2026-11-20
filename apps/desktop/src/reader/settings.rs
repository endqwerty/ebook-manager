//! The reader's text settings: what they are, the file they are kept in, and
//! the custom properties they become on the page. See
//! docs/design/interface-reader.md, "Text settings" and "Page colours".

use std::path::Path;

/// A colour as red, green and blue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    pub fn parse(text: &str) -> Option<Rgb> {
        let digits = text.strip_prefix('#')?;
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let part = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
        Some(Rgb(part(0)?, part(2)?, part(4)?))
    }

    /// Relative luminance, as WCAG 2 defines it.
    fn luminance(self) -> f64 {
        let channel = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(self.0) + 0.7152 * channel(self.1) + 0.0722 * channel(self.2)
    }

    /// This colour moved `part` of the way to `other`, in sRGB.
    fn towards(self, other: Rgb, part: f64) -> Rgb {
        let mix =
            |a: u8, b: u8| (f64::from(a) + (f64::from(b) - f64::from(a)) * part).round() as u8;
        Rgb(
            mix(self.0, other.0),
            mix(self.1, other.1),
            mix(self.2, other.2),
        )
    }
}

/// The contrast ratio of two colours, from 1 to 21.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (a, b) = (a.luminance(), b.luminance());
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// The colour of everything on a page that is not the book's text: the text
/// colour mixed toward the page colour 5% at a time, as far as it can go and
/// still have 4.5:1 against the page.
pub fn secondary(page: Rgb, ink: Rgb) -> Rgb {
    let mut chosen = ink;
    for step in 1..20 {
        let mixed = ink.towards(page, f64::from(step) * 0.05);
        if contrast(mixed, page) < 4.5 {
            break;
        }
        chosen = mixed;
    }
    chosen
}

/// A page colour: page, text and secondary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page {
    pub page: Rgb,
    pub ink: Rgb,
    pub secondary: Rgb,
}

pub const WHITE: Page = Page {
    page: Rgb(0xff, 0xff, 0xff),
    ink: Rgb(0x1a, 0x1a, 0x1a),
    secondary: Rgb(0x76, 0x76, 0x76),
};
pub const PAPER: Page = Page {
    page: Rgb(0xf4, 0xec, 0xd8),
    ink: Rgb(0x3b, 0x2f, 0x21),
    secondary: Rgb(0x73, 0x68, 0x58),
};
pub const GREY: Page = Page {
    page: Rgb(0x2b, 0x2b, 0x2e),
    ink: Rgb(0xd6, 0xd6, 0xd8),
    secondary: Rgb(0x92, 0x92, 0x94),
};
pub const BLACK: Page = Page {
    page: Rgb(0x00, 0x00, 0x00),
    ink: Rgb(0xc9, 0xc9, 0xc9),
    secondary: Rgb(0x79, 0x79, 0x79),
};

/// Which page colour the reader chose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Colour {
    /// Paper when the window is light and Grey when it is dark.
    System,
    White,
    Paper,
    Grey,
    Black,
    /// The reader's own two colours.
    Custom,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Margin {
    Narrow,
    Medium,
    Wide,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    Slide,
    Fade,
    None,
}

/// Size steps: twelve, from 0.8 to 2.5 times the default.
pub const SIZES: [f32; 12] = [0.8, 0.9, 1.0, 1.1, 1.2, 1.35, 1.5, 1.7, 1.9, 2.1, 2.3, 2.5];
pub const WEIGHTS: [(u16, &str); 4] = [
    (300, "Light"),
    (400, "Regular"),
    (500, "Medium"),
    (700, "Bold"),
];
pub const LINES: [f32; 5] = [1.2, 1.35, 1.5, 1.7, 2.0];
/// Extra space between letters, in ems.
pub const LETTERS: [f32; 4] = [0.0, 0.02, 0.05, 0.1];
/// Extra space between words, in ems.
pub const WORDS: [f32; 4] = [0.0, 0.1, 0.2, 0.35];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub colour: Colour,
    /// The reader's own page and text colours, used when `colour` is `Custom`.
    pub custom: (Rgb, Rgb),
    /// The face's name, or `None` for the reader's default.
    pub face: Option<String>,
    /// An index into `SIZES`.
    pub size: usize,
    /// An index into `WEIGHTS`.
    pub weight: usize,
    /// An index into `LINES`.
    pub line: usize,
    /// An index into `LETTERS`.
    pub letter: usize,
    /// An index into `WORDS`.
    pub word: usize,
    pub margin: Margin,
    pub justify: bool,
    pub hyphenate: bool,
    pub two_columns: bool,
    pub scrolling: bool,
    pub turn: Turn,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            colour: Colour::System,
            custom: (PAPER.page, PAPER.ink),
            face: None,
            size: 2,
            weight: 1,
            line: 2,
            letter: 0,
            word: 0,
            margin: Margin::Medium,
            justify: false,
            hyphenate: false,
            two_columns: false,
            scrolling: false,
            turn: Turn::Slide,
        }
    }
}

impl Settings {
    /// The page colour in force. `dark` is the window's appearance and
    /// `more_contrast` the system's setting, under which White and Black lose
    /// their softened text and Paper and Grey give way to them.
    pub fn page(&self, dark: bool, more_contrast: bool) -> Page {
        let chosen = match self.colour {
            Colour::System if dark => Colour::Grey,
            Colour::System => Colour::Paper,
            other => other,
        };
        let stark = |page: Rgb, ink: Rgb| Page {
            page,
            ink,
            secondary: secondary(page, ink),
        };
        let (white, black) = (Rgb(255, 255, 255), Rgb(0, 0, 0));
        match (chosen, more_contrast) {
            (Colour::Custom, _) => stark(self.custom.0, self.custom.1),
            (Colour::White | Colour::Paper | Colour::System, true) => stark(white, black),
            (Colour::Grey | Colour::Black, true) => stark(black, white),
            (Colour::White, false) => WHITE,
            (Colour::Grey, false) => GREY,
            (Colour::Black, false) => BLACK,
            (Colour::Paper | Colour::System, false) => PAPER,
        }
    }

    /// Forget a chosen face that is not among `offered`, the faces the
    /// system has that the panel offers. A face removed from the system
    /// since it was chosen is then the default everywhere: on the page, in
    /// the panel, and in what is saved next. The file keeps its name until
    /// then.
    pub fn keep_offered_face(&mut self, offered: &[String]) {
        if self.face.as_ref().is_some_and(|f| !offered.contains(f)) {
            self.face = None;
        }
    }

    /// The custom properties for the page. `default_face` is the face used
    /// when the reader has chosen none, as a CSS font list.
    pub fn properties(
        &self,
        dark: bool,
        more_contrast: bool,
        default_face: &str,
    ) -> Vec<(&'static str, String)> {
        let page = self.page(dark, more_contrast);
        let pick = |list: &[f32], at: usize| list[at.min(list.len() - 1)];
        let face = match &self.face {
            // A name is letters, digits and spaces, checked when it was read.
            Some(name) => format!("'{name}', serif"),
            None => default_face.to_owned(),
        };
        vec![
            ("--page", page.page.hex()),
            ("--ink", page.ink.hex()),
            ("--secondary", page.secondary.hex()),
            ("--font", face),
            ("--size", pick(&SIZES, self.size).to_string()),
            (
                "--weight",
                WEIGHTS[self.weight.min(WEIGHTS.len() - 1)].0.to_string(),
            ),
            ("--line", pick(&LINES, self.line).to_string()),
            ("--letter", format!("{}em", pick(&LETTERS, self.letter))),
            ("--word", format!("{}em", pick(&WORDS, self.word))),
            (
                "--margin",
                match self.margin {
                    Margin::Narrow => "0.75",
                    Margin::Medium => "1",
                    Margin::Wide => "1.5",
                }
                .to_owned(),
            ),
            (
                "--align",
                if self.justify { "justify" } else { "start" }.to_owned(),
            ),
            (
                "--hyphens",
                if self.hyphenate { "auto" } else { "manual" }.to_owned(),
            ),
            (
                "--columns",
                if self.two_columns { "2" } else { "1" }.to_owned(),
            ),
            (
                "--flow",
                if self.scrolling { "scroll" } else { "paged" }.to_owned(),
            ),
            (
                "--turn",
                match self.turn {
                    Turn::Slide => "slide",
                    Turn::Fade => "fade",
                    Turn::None => "none",
                }
                .to_owned(),
            ),
        ]
    }

    /// The settings as `name=value` lines.
    pub fn to_text(&self) -> String {
        let colour = match self.colour {
            Colour::System => "system",
            Colour::White => "white",
            Colour::Paper => "paper",
            Colour::Grey => "grey",
            Colour::Black => "black",
            Colour::Custom => "custom",
        };
        let margin = match self.margin {
            Margin::Narrow => "narrow",
            Margin::Medium => "medium",
            Margin::Wide => "wide",
        };
        let turn = match self.turn {
            Turn::Slide => "slide",
            Turn::Fade => "fade",
            Turn::None => "none",
        };
        format!(
            "colour={colour}\ncustom-page={}\ncustom-text={}\nface={}\nsize={}\nweight={}\nline={}\nletter={}\nword={}\nmargin={margin}\njustify={}\nhyphenate={}\ntwo-columns={}\nscrolling={}\nturn={turn}\n",
            self.custom.0.hex(),
            self.custom.1.hex(),
            self.face.as_deref().unwrap_or(""),
            self.size,
            self.weight,
            self.line,
            self.letter,
            self.word,
            self.justify,
            self.hyphenate,
            self.two_columns,
            self.scrolling,
        )
    }

    /// Read settings written by `to_text`. A line that is missing, unknown
    /// or out of range leaves that setting at its default, so a damaged or
    /// older file never stops a book from opening.
    pub fn from_text(text: &str) -> Settings {
        let mut s = Settings::default();
        let index = |value: &str, len: usize| value.parse::<usize>().ok().filter(|n| *n < len);
        let flag = |value: &str| match value {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        };
        for line in text.lines() {
            let Some((name, value)) = line.split_once('=') else {
                continue;
            };
            match name {
                "colour" => {
                    s.colour = match value {
                        "white" => Colour::White,
                        "paper" => Colour::Paper,
                        "grey" => Colour::Grey,
                        "black" => Colour::Black,
                        "custom" => Colour::Custom,
                        _ => Colour::System,
                    }
                }
                "custom-page" => s.custom.0 = Rgb::parse(value).unwrap_or(s.custom.0),
                "custom-text" => s.custom.1 = Rgb::parse(value).unwrap_or(s.custom.1),
                "face" => s.face = face_name(value),
                "size" => s.size = index(value, SIZES.len()).unwrap_or(s.size),
                "weight" => s.weight = index(value, WEIGHTS.len()).unwrap_or(s.weight),
                "line" => s.line = index(value, LINES.len()).unwrap_or(s.line),
                "letter" => s.letter = index(value, LETTERS.len()).unwrap_or(s.letter),
                "word" => s.word = index(value, WORDS.len()).unwrap_or(s.word),
                "margin" => {
                    s.margin = match value {
                        "narrow" => Margin::Narrow,
                        "wide" => Margin::Wide,
                        _ => Margin::Medium,
                    }
                }
                "justify" => s.justify = flag(value).unwrap_or(s.justify),
                "hyphenate" => s.hyphenate = flag(value).unwrap_or(s.hyphenate),
                "two-columns" => s.two_columns = flag(value).unwrap_or(s.two_columns),
                "scrolling" => s.scrolling = flag(value).unwrap_or(s.scrolling),
                "turn" => {
                    s.turn = match value {
                        "fade" => Turn::Fade,
                        "none" => Turn::None,
                        _ => Turn::Slide,
                    }
                }
                _ => {}
            }
        }
        s
    }

    /// The settings in `file`, or the defaults when it cannot be read.
    pub fn load(file: &Path) -> Settings {
        use std::io::Read;
        // The file is read on the window's thread. One of the size this
        // app writes is a few hundred bytes, so anything past the limit is
        // not this file, and is not read past it.
        let mut text = String::new();
        let read = std::fs::File::open(file)
            .and_then(|f| f.take(SETTINGS_LIMIT + 1).read_to_string(&mut text));
        match read {
            Ok(n) if n as u64 <= SETTINGS_LIMIT => Settings::from_text(&text),
            _ => Settings::default(),
        }
    }

    /// Write the settings to `file`, by way of a file beside it so a crash
    /// part-way leaves the old settings whole.
    pub fn save(&self, file: &Path) -> std::io::Result<()> {
        if let Some(folder) = file.parent() {
            std::fs::create_dir_all(folder)?;
        }
        let beside = file.with_extension("conf.new");
        std::fs::write(&beside, self.to_text())?;
        std::fs::rename(&beside, file)
    }
}

/// The largest settings file that is read, in bytes. A larger one is left
/// alone and the defaults are used.
pub const SETTINGS_LIMIT: u64 = 64 * 1024;

/// A face's name as the page may be given it: letters, digits, spaces and
/// hyphens, so it cannot end the CSS it is written into.
pub fn face_name(name: &str) -> Option<String> {
    let name = name.trim();
    let plain = name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == ' ' || c == '-');
    (!name.is_empty() && name.len() <= 80 && plain).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(properties: &[(&'static str, String)], name: &str) -> String {
        properties
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.clone())
            .unwrap_or_default()
    }

    #[test]
    fn the_built_in_colours_are_the_designs() {
        // The table in docs/design/interface-reader.md, with its contrasts.
        for (page, text, second) in [
            (WHITE, 17.4, 4.5),
            (PAPER, 11.0, 4.7),
            (GREY, 9.7, 4.5),
            (BLACK, 12.7, 4.8),
        ] {
            assert!(
                (contrast(page.page, page.ink) - text).abs() < 0.1,
                "{page:?}"
            );
            assert!(
                (contrast(page.page, page.secondary) - second).abs() < 0.11,
                "{page:?}"
            );
            assert!(contrast(page.page, page.secondary) >= 4.5, "{page:?}");
        }
    }

    #[test]
    fn the_page_follows_the_system_until_the_reader_chooses() {
        let mut s = Settings::default();
        assert_eq!(s.page(false, false), PAPER);
        assert_eq!(s.page(true, false), GREY);
        s.colour = Colour::White;
        assert_eq!(s.page(true, false), WHITE);
        s.colour = Colour::Black;
        assert_eq!(s.page(false, false), BLACK);
    }

    #[test]
    fn more_contrast_gives_pure_black_and_white() {
        let mut s = Settings::default();
        let (white, black) = (Rgb(255, 255, 255), Rgb(0, 0, 0));
        for (colour, dark, page, ink) in [
            (Colour::System, false, white, black),
            (Colour::System, true, black, white),
            (Colour::Paper, true, white, black),
            (Colour::Grey, false, black, white),
            (Colour::White, false, white, black),
            (Colour::Black, false, black, white),
        ] {
            s.colour = colour;
            let got = s.page(dark, true);
            assert_eq!((got.page, got.ink), (page, ink), "{colour:?}");
            assert!(contrast(got.page, got.secondary) >= 4.5);
        }
    }

    #[test]
    fn a_colour_of_the_readers_own_gets_a_readable_secondary() {
        let s = Settings {
            colour: Colour::Custom,
            custom: (Rgb(0x10, 0x20, 0x40), Rgb(0xf0, 0xe0, 0xc0)),
            ..Settings::default()
        };
        let page = s.page(false, false);
        assert_eq!((page.page, page.ink), s.custom);
        let ratio = contrast(page.page, page.secondary);
        assert!(ratio >= 4.5, "{ratio}");
        // One more step toward the page would fall under 4.5.
        let further = page.secondary.towards(page.page, 0.08);
        assert!(contrast(page.page, further) < contrast(page.page, page.secondary));
        // Two colours with too little contrast keep the text colour itself.
        assert_eq!(
            secondary(Rgb(100, 100, 100), Rgb(120, 120, 120)),
            Rgb(120, 120, 120)
        );
    }

    #[test]
    fn each_setting_is_a_property_of_the_page() {
        let mut s = Settings::default();
        let p = s.properties(false, false, "'Noto Serif', serif");
        for (name, expected) in [
            ("--page", "#f4ecd8"),
            ("--ink", "#3b2f21"),
            ("--secondary", "#736858"),
            ("--font", "'Noto Serif', serif"),
            ("--size", "1"),
            ("--weight", "400"),
            ("--line", "1.5"),
            ("--letter", "0em"),
            ("--word", "0em"),
            ("--margin", "1"),
            ("--align", "start"),
            ("--hyphens", "manual"),
            ("--columns", "1"),
            ("--flow", "paged"),
            ("--turn", "slide"),
        ] {
            assert_eq!(value(&p, name), expected, "{name}");
        }
        s.face = Some("DejaVu Sans".into());
        s.size = 11;
        s.weight = 3;
        s.line = 4;
        s.letter = 2;
        s.word = 3;
        s.margin = Margin::Wide;
        s.justify = true;
        s.hyphenate = true;
        s.two_columns = true;
        s.scrolling = true;
        s.turn = Turn::None;
        let p = s.properties(true, false, "serif");
        for (name, expected) in [
            ("--page", "#2b2b2e"),
            ("--font", "'DejaVu Sans', serif"),
            ("--size", "2.5"),
            ("--weight", "700"),
            ("--line", "2"),
            ("--letter", "0.05em"),
            ("--word", "0.35em"),
            ("--margin", "1.5"),
            ("--align", "justify"),
            ("--hyphens", "auto"),
            ("--columns", "2"),
            ("--flow", "scroll"),
            ("--turn", "none"),
        ] {
            assert_eq!(value(&p, name), expected, "{name}");
        }
    }

    #[test]
    fn settings_survive_their_file() {
        let mut s = Settings::default();
        assert_eq!(Settings::from_text(&s.to_text()), s);
        s.colour = Colour::Custom;
        s.custom = (Rgb(1, 2, 3), Rgb(250, 251, 252));
        s.face = Some("Liberation Serif".into());
        s.size = 7;
        s.weight = 0;
        s.line = 0;
        s.letter = 3;
        s.word = 1;
        s.margin = Margin::Narrow;
        s.justify = true;
        s.hyphenate = true;
        s.two_columns = true;
        s.scrolling = true;
        s.turn = Turn::Fade;
        assert_eq!(Settings::from_text(&s.to_text()), s);
        let folder = std::env::temp_dir().join(format!("ebook-settings-{}", std::process::id()));
        let file = folder.join("deep").join("reader.conf");
        s.save(&file).unwrap();
        assert_eq!(Settings::load(&file), s);
        assert_eq!(
            Settings::load(&folder.join("absent.conf")),
            Settings::default()
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_damaged_file_gives_defaults_for_what_it_damaged() {
        let s = Settings::from_text(
            "size=99\nweight=-1\nline=abc\nface='; } html { display: none\ncolour=mauve\ncustom-page=#12\nmargin=vast\njustify=yes\nturn=spin\nunknown=1\nno equals sign\nletter=1\n",
        );
        let expected = Settings {
            letter: 1,
            ..Settings::default()
        };
        assert_eq!(s, expected);
        assert_eq!(face_name("Noto Serif"), Some("Noto Serif".into()));
        assert_eq!(face_name("a'b"), None);
        assert_eq!(face_name(""), None);
    }

    #[test]
    fn a_file_past_the_limit_is_not_read() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("reader.conf");
        std::fs::write(&file, "size=5\n").unwrap();
        assert_eq!(Settings::load(&file).size, 5);
        // Every line is a good one, so only the size can refuse it.
        let lines = "size=5\n".repeat(SETTINGS_LIMIT as usize / 7 + 1);
        assert!(lines.len() as u64 > SETTINGS_LIMIT);
        std::fs::write(&file, &lines).unwrap();
        assert_eq!(Settings::load(&file), Settings::default());
        // Exactly at the limit is still read.
        let mut at_limit = "size=5\n".to_owned();
        at_limit.push_str(&"#".repeat(SETTINGS_LIMIT as usize - at_limit.len()));
        std::fs::write(&file, &at_limit).unwrap();
        assert_eq!(Settings::load(&file).size, 5);
    }

    #[test]
    fn a_face_the_system_lacks_is_the_default() {
        let offered = vec!["Noto Serif".to_owned()];
        let mut s = Settings {
            face: Some("Gone Serif".into()),
            ..Settings::default()
        };
        s.keep_offered_face(&offered);
        assert_eq!(s.face, None);
        assert_eq!(
            value(&s.properties(false, false, "serif"), "--font"),
            "serif"
        );
        assert!(!s.to_text().contains("Gone Serif"));
        let mut s = Settings {
            face: Some("Noto Serif".into()),
            ..Settings::default()
        };
        s.keep_offered_face(&offered);
        assert_eq!(s.face.as_deref(), Some("Noto Serif"));
        assert_eq!(
            value(&s.properties(false, false, "serif"), "--font"),
            "'Noto Serif', serif"
        );
    }
}
