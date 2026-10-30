//! Keep the format-specific remainder during every generic edit.
use anyhow::{Result, bail};
use lofty::{
    config::{ParseOptions, ParsingMode},
    file::{AudioFile, FileType},
    tag::{SplitTag, Tag, TagExt, TagType},
};
use std::path::Path;

#[derive(Clone)]
pub(super) enum Native {
    Id3v2(lofty::id3::v2::Id3v2Tag),
    Id3v1(lofty::id3::v1::Id3v1Tag),
    Vorbis(lofty::ogg::tag::VorbisComments),
    Mp4(lofty::mp4::Ilst),
    Ape(lofty::ape::ApeTag),
    Riff(lofty::iff::wav::RiffInfoList),
    Aiff(lofty::iff::aiff::AiffTextChunks),
}

macro_rules! dispatch {
    ($self:expr, $value:ident, $operation:expr) => {
        match $self {
            Native::Id3v2($value) => $operation,
            Native::Id3v1($value) => $operation,
            Native::Vorbis($value) => $operation,
            Native::Mp4($value) => $operation,
            Native::Ape($value) => $operation,
            Native::Riff($value) => $operation,
            Native::Aiff($value) => $operation,
        }
    };
}

impl Native {
    pub fn kind(&self) -> TagType {
        dispatch!(self, tag, tag.tag_type())
    }
    pub fn generic(&self) -> Tag {
        dispatch!(self.clone(), tag, tag.split_tag().1)
    }
    pub fn encoded(&self) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        dispatch!(
            self,
            tag,
            tag.dump_to(&mut bytes, lofty::config::WriteOptions::new())
        )?;
        Ok(bytes)
    }
    pub fn empty(kind: TagType) -> Option<Self> {
        Some(match kind {
            TagType::Id3v2 => Self::Id3v2(Default::default()),
            TagType::Id3v1 => Self::Id3v1(Default::default()),
            TagType::VorbisComments => Self::Vorbis(Default::default()),
            TagType::Mp4Ilst => Self::Mp4(Default::default()),
            TagType::Ape => Self::Ape(Default::default()),
            TagType::RiffInfo => Self::Riff(Default::default()),
            TagType::AiffText => Self::Aiff(Default::default()),
            _ => return None,
        })
    }
}

pub(super) fn read(path: &Path, kind: FileType) -> Result<Vec<Native>> {
    let mut reader = super::reader::Reader::open(path, kind == FileType::Mpeg)?;
    let blank_year = reader.preserved_id3v1.is_some();
    let options = ParseOptions::new().parsing_mode(ParsingMode::Strict);
    let mut tags = Vec::new();
    macro_rules! read {
        ($ty:ty, $($get:ident => $variant:ident),+) => {{
            let file = <$ty>::read_from(&mut reader, options)?;
            $(if let Some(tag) = file.$get() { tags.push(Native::$variant(tag.clone())); })+
        }};
    }
    match kind {
        FileType::Mpeg => read!(lofty::mpeg::MpegFile, id3v2 => Id3v2, id3v1 => Id3v1, ape => Ape),
        FileType::Aac => read!(lofty::aac::AacFile, id3v2 => Id3v2, id3v1 => Id3v1),
        FileType::Flac => read!(lofty::flac::FlacFile, id3v2 => Id3v2, vorbis_comments => Vorbis),
        FileType::Mp4 => read!(lofty::mp4::Mp4File, ilst => Mp4),
        FileType::Wav => read!(lofty::iff::wav::WavFile, id3v2 => Id3v2, riff_info => Riff),
        FileType::Aiff => read!(lofty::iff::aiff::AiffFile, id3v2 => Id3v2, text_chunks => Aiff),
        FileType::Ape => read!(lofty::ape::ApeFile, id3v2 => Id3v2, id3v1 => Id3v1, ape => Ape),
        FileType::WavPack => read!(lofty::wavpack::WavPackFile, id3v1 => Id3v1, ape => Ape),
        FileType::Mpc => read!(lofty::musepack::MpcFile, id3v1 => Id3v1, ape => Ape),
        FileType::Vorbis => tags.push(Native::Vorbis(
            lofty::ogg::VorbisFile::read_from(&mut reader, options)?
                .vorbis_comments()
                .clone(),
        )),
        FileType::Opus => tags.push(Native::Vorbis(
            lofty::ogg::OpusFile::read_from(&mut reader, options)?
                .vorbis_comments()
                .clone(),
        )),
        FileType::Speex => tags.push(Native::Vorbis(
            lofty::ogg::SpeexFile::read_from(&mut reader, options)?
                .vorbis_comments()
                .clone(),
        )),
        _ => bail!("Формат не поддерживает безопасное редактирование всех блоков тегов."),
    }
    if blank_year {
        for tag in &mut tags {
            if let Native::Id3v1(tag) = tag {
                tag.year = None;
            }
        }
    }
    Ok(tags)
}
