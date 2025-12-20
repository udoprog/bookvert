use core::str::FromStr;

use std::borrow::Cow;
use std::io::Cursor;
use std::path::PathBuf;

use anyhow::Result;
use jiff::civil::Date;
use lofty::file::{FileType, TaggedFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{ItemKey, ItemValue, TagItem};

use crate::config::{Db, Source};
use crate::out::{Out, blank, info};

pub(crate) struct Parts {
    year: i16,
    artist: String,
    album: String,
    track: u32,
    title: String,
    media_type: Option<String>,
    set: Option<(u32, u32)>,
}

impl Parts {
    pub(crate) fn from_path(
        source: &Source,
        db: &Db,
        errors: &mut Vec<String>,
        items: &mut Vec<TagItem>,
    ) -> Result<Option<Self>> {
        let file: TaggedFile = match source {
            Source::File { file } => {
                let path = db.file(*file)?;
                lofty::read_from_path(path)?
            }
            Source::Archive { archive, path } => {
                let contents = db.archive_contents(*archive, path)?;
                let mut probe = Probe::new(Cursor::new(contents));

                if let Some(file_type) = db.ext(source)?.and_then(FileType::from_ext) {
                    probe = probe.set_file_type(file_type);
                }

                probe.read()?
            }
        };

        let Some(tag) = file.primary_tag() else {
            errors.push("missing primary tag".to_string());
            return Ok(None);
        };

        /// A priority container.
        struct Prio<T> {
            /// Current value.
            value: Option<T>,
            /// Priority of the current value. Lower is better.
            prio: u32,
        }

        impl<T> Prio<T> {
            fn new() -> Self {
                Self {
                    value: None,
                    prio: u32::MAX,
                }
            }

            fn update(&mut self, new: Option<T>, prio: u32) {
                if let Some(new) = new
                    && prio < self.prio
                {
                    self.value = Some(new);
                    self.prio = prio;
                }
            }
        }

        macro_rules! parse {
            (
                $(
                    $name:ident = $parse:ident {
                        $($key:ident = $priority:expr),* $(,)?
                    }
                ),* $(,)?
            ) => {
                $(let mut $name = Prio::new();)*

                for item in tag.items() {
                    items.push(item.clone());

                    let value = item.value();

                    match item.key() {
                        $($(ItemKey::$key =>  {
                            $name.update($parse(value), $priority);
                        })*)*
                        _ => {},
                    };
                }
            };
        }

        parse! {
            year = year_like {
                OriginalReleaseDate = 1,
                ReleaseDate = 2,
                Year = 3,
                RecordingDate = 4,
            },
            album = text {
                AlbumTitle = 1,
            },
            artist = text {
                AlbumArtist = 1,
                TrackArtist = 2,
            },
            title = text {
                TrackTitle = 1,
            },
            track = parse {
                TrackNumber = 1,
            },
            media_type = text {
                OriginalMediaType = 1,
            },
            disc_number = parse {
                DiscNumber = 1,
            },
            disc_total = parse {
                DiscTotal = 1,
            },
        }

        fn text(value: &ItemValue) -> Option<&str> {
            let s = value.text()?.trim();
            (!s.is_empty()).then_some(s)
        }

        fn year_like(value: &ItemValue) -> Option<i16> {
            let s = value.text()?;
            let s = s.trim();

            if let Ok(date) = s.parse::<Date>() {
                return Some(date.year());
            }

            if let Ok(year) = s.parse::<i16>() {
                return Some(year);
            }

            None
        }

        fn parse<T>(value: &ItemValue) -> Option<T>
        where
            T: FromStr,
        {
            let s = value.text()?;
            T::from_str(s).ok()
        }

        let mut value = || {
            if year.value.is_none() {
                errors.push("missing year".to_string());
            }

            if album.value.is_none() {
                errors.push("missing album".to_string());
            }

            if artist.value.is_none() {
                errors.push("missing artist".to_string());
            }

            if title.value.is_none() {
                errors.push("missing title".to_string());
            }

            if track.value.is_none() {
                errors.push("missing track number".to_string());
            }

            let set = match (disc_number.value, disc_total.value) {
                (Some(n), Some(total)) => Some((n, total)),
                _ => None,
            };

            Some(Self {
                year: year.value?,
                artist: artist.value?.to_owned(),
                album: album.value?.to_owned(),
                track: track.value?,
                title: title.value?.to_owned(),
                media_type: media_type.value.map(str::to_owned),
                set,
            })
        };

        Ok(value())
    }

    /// Append parts to a buffer.
    pub(crate) fn append_to(&self, path: &mut PathBuf) {
        use core::fmt::Write;

        let mut s = String::new();

        macro_rules! s {
            ($($arg:tt)*) => {{
                s.clear();
                _ = write!(s, $($arg)*);
                s.as_str()
            }};
        }

        push_sanitized(path, s!("{}", self.artist));
        push_sanitized(path, s!("{} ({})", &self.album, self.year));

        if let Some((n, total)) = self.set
            && total > 1
        {
            s.clear();

            if let Some(media_type) = &self.media_type {
                s.push_str(media_type);
                s.push(' ');
            }

            _ = write!(s, "{n:02}");
            push_sanitized(path, &s);
        }

        push_sanitized(
            path,
            s!(
                "{} - {} - {:02} - {}",
                self.artist,
                self.album,
                self.track,
                &self.title
            ),
        );
    }
}

fn push_sanitized(path: &mut PathBuf, s: &str) {
    path.push(sanitize(s).as_ref());
}

fn sanitize(s: &str) -> Cow<'_, str> {
    let mut out = String::new();

    let rest = 'normalize: {
        for (n, c) in s.char_indices() {
            match c {
                ':' => {
                    out.push_str(&s[..n]);
                    break 'normalize &s[n..];
                }
                c => {
                    if map(c).is_some() {
                        out.push_str(&s[..n]);
                        break 'normalize &s[n..];
                    }
                }
            }
        }

        return Cow::Borrowed(s);
    };

    fn map(c: char) -> Option<&'static str> {
        match c {
            '\\' => Some("+"),
            '/' => Some("+"),
            '<' => Some(""),
            '>' => Some(""),
            '?' => Some(""),
            '*' => Some("-"),
            '|' => Some(""),
            '"' => Some(""),
            _ => None,
        }
    }

    let mut last_whitespace = false;
    let mut it = rest.chars();

    while let Some(c) = it.next() {
        match c {
            ':' => {
                if it.clone().next().is_some_and(|c| c.is_whitespace()) {
                    out.push_str(" - ");
                    it.next();
                } else {
                    out.push('-');
                }
            }
            c => {
                if let Some(repl) = map(c) {
                    out.push_str(repl);
                    continue;
                }

                if last_whitespace && c.is_whitespace() {
                    continue;
                }

                out.push(c);
                last_whitespace = c.is_whitespace();
            }
        }
    }

    Cow::Owned(out)
}

pub(super) struct Meta {
    pub(super) items: Vec<TagItem>,
}

impl Meta {
    pub(crate) fn dump(&self, o: &mut Out<'_>) -> Result<()> {
        for item in &self.items {
            dump_tag_item(o, item)?;
        }

        Ok(())
    }
}

impl FromIterator<TagItem> for Meta {
    #[inline]
    fn from_iter<T>(iter: T) -> Self
    where
        T: IntoIterator<Item = TagItem>,
    {
        Self {
            items: iter.into_iter().collect(),
        }
    }
}

fn dump_tag_item(o: &mut Out<'_>, item: &TagItem) -> Result<()> {
    info!(o, "{:?}:", item.key());
    let mut o = o.indent(1);

    match item.value() {
        ItemValue::Text(text) => {
            blank!(o, "text: {text:?}");
        }
        ItemValue::Locator(link) => {
            blank!(o, "link: {link:?}");
        }
        ItemValue::Binary(data) => {
            blank!(o, "binary: {} bytes", data.len());
        }
    }

    Ok(())
}
