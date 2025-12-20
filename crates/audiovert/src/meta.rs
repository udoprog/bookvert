use core::str::FromStr;

use std::borrow::Cow;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use anyhow::Result;
use jiff::civil::Date;
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, FileType, TaggedFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::{ItemKey, ItemValue, Tag, TagItem, TagType};

use crate::config::{Db, Source};
use crate::format::Format;
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
        tagged: &mut Option<Meta>,
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

        let meta = tagged.get_or_insert(Meta { file });

        let Some(tag) = meta.file.primary_tag() else {
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
    pub(super) file: TaggedFile,
}

impl Meta {
    /// Get the total number of tags.
    pub(crate) fn len(&self) -> u32 {
        self.file.tags().iter().map(|tag| tag.item_count()).sum()
    }

    /// Dump tags to output.
    pub(crate) fn dump(&self, o: &mut Out<'_>) -> Result<()> {
        for tag in self.file.tags() {
            info!(o, "tag: {}", repr_tag_type(tag.tag_type()));
            let mut o = o.indent(1);

            for item in tag.items() {
                dump_tag_item(&mut o, item)?;
            }
        }

        Ok(())
    }

    pub(crate) fn tag_file(&self, to: Format, path: &Path) -> Result<()> {
        // First try to copy tags immediately.
        let Some(source_tag) = self.file.primary_tag() else {
            return Ok(());
        };

        let mut probe = Probe::open(path)?;
        probe = probe.set_file_type(format_file_type(to));

        let mut existing = probe.read()?;

        let tag_type = existing.primary_tag_type();

        existing.clear();

        'done: {
            // Primary method: try to insert the primary tag directly if it is
            // identical to the source tag type.
            if source_tag.tag_type() == tag_type {
                existing.insert_tag(source_tag.clone());
                break 'done;
            }

            // Fallback: copy items one by one, which will cause unsupported
            // tags to be skipped.
            let mut tag = Tag::new(tag_type);

            for item in source_tag.items() {
                tag.insert(item.clone());
            }

            existing.insert_tag(tag);
        };

        let mut options = WriteOptions::default();
        options.use_id3v23(true);
        existing.save_to_path(path, options)?;
        Ok(())
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

fn format_file_type(format: Format) -> FileType {
    match format {
        Format::Aac => FileType::Aac,
        Format::Flac => FileType::Flac,
        Format::Mp3 => FileType::Mpeg,
        Format::Ogg => FileType::Vorbis,
        Format::Wav => FileType::Wav,
    }
}

fn repr_tag_type(ty: TagType) -> &'static str {
    match ty {
        TagType::Ape => "APE",
        TagType::Id3v1 => "ID3v1",
        TagType::Id3v2 => "ID3v2",
        TagType::Mp4Ilst => "MP4 ilst",
        TagType::VorbisComments => "Vorbis Comments",
        TagType::RiffInfo => "RIFF INFO",
        TagType::AiffText => "AIFF TEXT",
        _ => "Unknown",
    }
}
