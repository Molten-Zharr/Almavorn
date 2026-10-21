use super::{item_value_text, native::Native, value_from_text};
use anyhow::{Context, Result, bail, ensure};
use lofty::{
    TextEncoding,
    config::WriteOptions,
    id3::v2::{BinaryFrame, ExtendedTextFrame, Frame, FrameId, Id3v2Tag},
    mp4::{Atom, AtomData, AtomIdent},
    tag::{ItemKey, ItemValue, MergeTag, SplitTag, Tag, TagExt, TagType},
};
use std::borrow::Cow;

#[derive(Clone)]
enum Source {
    Text,
    Ape(lofty::ape::ApeItem),
    Id3(Frame<'static>),
    Mp4(AtomIdent<'static>, AtomData),
}

#[derive(Clone)]
pub struct Extra {
    pub name: String,
    pub value: String,
    pub value_kind: usize,
    pub remove: bool,
    source: Source,
    original_value: String,
}

impl Extra {
    pub fn create(kind: TagType, name: &str, value: &str) -> Result<Self> {
        ensure!(
            !name.trim().is_empty(),
            "Введите название дополнительного поля."
        );
        ensure!(
            ItemKey::from_key(kind, name).is_none(),
            "Это стандартное поле. Добавьте его на вкладке Поля."
        );
        let source = match kind {
            TagType::Id3v2 => Source::Id3(Frame::UserText(ExtendedTextFrame::new(
                TextEncoding::UTF8,
                name.to_owned(),
                value.to_owned(),
            ))),
            TagType::VorbisComments | TagType::RiffInfo => Source::Text,
            TagType::Ape => Source::Ape(lofty::ape::ApeItem::new(
                name.to_owned(),
                ItemValue::Text(value.to_owned()),
            )?),
            TagType::Mp4Ilst => Source::Mp4(
                AtomIdent::Freeform {
                    mean: Cow::Borrowed("com.apple.iTunes"),
                    name: Cow::Owned(name.to_owned()),
                },
                AtomData::UTF8(value.to_owned()),
            ),
            _ => bail!("Этот формат не поддерживает произвольные поля."),
        };
        validate_name(kind, name)?;
        Ok(Self {
            name: name.into(),
            value: value.into(),
            original_value: value.into(),
            value_kind: 0,
            remove: false,
            source,
        })
    }

    fn id3_frame(&self) -> Result<Frame<'static>> {
        let Source::Id3(original) = &self.source else {
            bail!("Неверный тип поля ID3.");
        };
        if self.value == self.original_value {
            return Ok(original.clone());
        }
        ensure!(
            !original.flags().compression && original.flags().encryption.is_none(),
            "Сжатое или зашифрованное поле можно сохранить либо удалить целиком; редактирование его содержимого недоступно."
        );
        let mut frame = original.clone();
        match &mut frame {
            Frame::UserText(item) => item.content = Cow::Owned(self.value.clone()),
            Frame::UserUrl(item) => item.content = Cow::Owned(self.value.clone()),
            Frame::Text(item) => item.value = Cow::Owned(self.value.clone()),
            Frame::Url(item) => {
                ensure!(item.set_url(self.value.clone()), "Некорректный URL.");
            }
            Frame::Binary(item) => item.data = Cow::Owned(hex(&self.value)?),
            _ => {
                let mut binary = BinaryFrame::new(
                    FrameId::Valid(Cow::Owned(original.id_str().into())),
                    hex(&self.value)?,
                );
                // dump_to has already applied any frame flags to the payload.
                binary.set_flags(original.flags());
                frame = Frame::Binary(binary);
            }
        }
        if frame.flags().data_length_indicator.is_some() {
            let length: u32 = (payload(&frame)?.len() / 2)
                .try_into()
                .context("Поле ID3 слишком большое.")?;
            frame.set_flags(lofty::id3::v2::FrameFlags {
                data_length_indicator: Some(length),
                ..frame.flags()
            });
        }
        Ok(frame)
    }

    fn mp4_data(&self) -> Result<AtomData> {
        let Source::Mp4(_, original) = &self.source else {
            bail!("Неверный тип поля MP4.");
        };
        if self.value == self.original_value {
            return Ok(original.clone());
        }
        Ok(match original {
            AtomData::UTF8(_) => AtomData::UTF8(self.value.clone()),
            AtomData::UTF16(_) => AtomData::UTF16(self.value.clone()),
            AtomData::SignedInteger(_) => {
                AtomData::SignedInteger(self.value.parse().context("Введите целое число.")?)
            }
            AtomData::UnsignedInteger(_) => AtomData::UnsignedInteger(
                self.value
                    .parse()
                    .context("Введите целое неотрицательное число.")?,
            ),
            AtomData::Bool(_) => AtomData::Bool(match self.value.as_str() {
                "1" | "true" => true,
                "0" | "false" => false,
                _ => bail!("Введите true или false."),
            }),
            AtomData::Unknown { code, .. } => AtomData::Unknown {
                code: *code,
                data: hex(&self.value)?,
            },
            AtomData::Picture(_) => AtomData::Picture(lofty::picture::Picture::from_reader(
                &mut std::io::Cursor::new(hex(&self.value)?),
            )?),
        })
    }
}

fn hex(value: &str) -> Result<Vec<u8>> {
    value_from_text(2, value)?
        .into_binary()
        .context("Ожидались двоичные данные.")
}

fn validate_name(kind: TagType, name: &str) -> Result<()> {
    if kind == TagType::VorbisComments {
        ensure!(
            name.bytes()
                .all(|b| (0x20..=0x7d).contains(&b) && b != b'='),
            "Имя Vorbis должно состоять из ASCII-символов без знака =."
        );
    }
    if kind == TagType::RiffInfo {
        ensure!(
            name.len() == 4
                && name
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()),
            "Имя RIFF: четыре заглавные латинские буквы или цифры."
        );
    }
    Ok(())
}

fn payload(frame: &Frame<'static>) -> Result<String> {
    let mut tag = Id3v2Tag::new();
    let mut frame = frame.clone();
    frame.set_flags(lofty::id3::v2::FrameFlags {
        grouping_identity: None,
        encryption: None,
        compression: false,
        data_length_indicator: None,
        ..frame.flags()
    });
    tag.insert(frame);
    let mut bytes = Vec::new();
    tag.dump_to(&mut bytes, WriteOptions::new().preferred_padding(0))?;
    ensure!(
        bytes.len() >= 20,
        "Не удалось представить специальное поле ID3."
    );
    let length = bytes[14..18]
        .iter()
        .fold(0usize, |n, b| (n << 7) | usize::from(*b & 0x7f));
    let bytes = bytes
        .get(20..20 + length)
        .context("Неверный размер поля ID3.")?;
    Ok(item_value_text(&ItemValue::Binary(bytes.to_vec())))
}

impl Native {
    pub fn extras(&self) -> Result<Vec<Extra>> {
        let mut fields = Vec::new();
        match self.clone() {
            Native::Vorbis(tag) => {
                let remainder: lofty::ogg::tag::VorbisComments = tag.split_tag().0.into();
                fields.extend(remainder.items().map(|(name, value)| Extra {
                    name: name.into(),
                    value: value.into(),
                    original_value: value.into(),
                    value_kind: 0,
                    remove: false,
                    source: Source::Text,
                }));
            }
            Native::Riff(tag) => {
                fields.extend(
                    tag.into_iter()
                        .filter(|(key, _)| ItemKey::from_key(TagType::RiffInfo, key).is_none())
                        .map(|(name, value)| Extra {
                            name,
                            original_value: value.clone(),
                            value,
                            value_kind: 0,
                            remove: false,
                            source: Source::Text,
                        }),
                );
            }
            Native::Ape(tag) => {
                let remainder: lofty::ape::ApeTag = tag.split_tag().0.into();
                fields.extend(remainder.into_iter().map(|item| Extra {
                    name: item.key().into(),
                    value: item_value_text(item.value()),
                    original_value: item_value_text(item.value()),
                    value_kind: if matches!(item.value(), ItemValue::Binary(_)) {
                        2
                    } else {
                        0
                    },
                    remove: false,
                    source: Source::Ape(item),
                }));
            }
            Native::Id3v2(tag) => {
                let remainder: Id3v2Tag = tag.split_tag().0.into();
                for frame in remainder {
                    let (name, value, value_kind) = match &frame {
                        Frame::UserText(item) => {
                            (item.description.to_string(), item.content.to_string(), 0)
                        }
                        Frame::UserUrl(item) => {
                            (item.description.to_string(), item.content.to_string(), 1)
                        }
                        Frame::Text(item) => (frame.id_str().into(), item.value.to_string(), 0),
                        Frame::Url(item) => (frame.id_str().into(), item.url().to_string(), 1),
                        Frame::Binary(item) => (
                            frame.id_str().into(),
                            item_value_text(&ItemValue::Binary(item.data.to_vec())),
                            2,
                        ),
                        _ => (frame.id_str().into(), payload(&frame)?, 2),
                    };
                    fields.push(Extra {
                        name,
                        original_value: value.clone(),
                        value,
                        value_kind,
                        remove: false,
                        source: Source::Id3(frame),
                    });
                }
            }
            Native::Mp4(tag) => {
                let remainder: lofty::mp4::Ilst = tag.split_tag().0.into();
                for atom in remainder {
                    for data in atom.data() {
                        let (value, value_kind) = match data {
                            AtomData::UTF8(value) | AtomData::UTF16(value) => (value.clone(), 0),
                            AtomData::SignedInteger(value) => (value.to_string(), 0),
                            AtomData::UnsignedInteger(value) => (value.to_string(), 0),
                            AtomData::Bool(value) => (value.to_string(), 0),
                            AtomData::Unknown { data, .. } => {
                                (item_value_text(&ItemValue::Binary(data.clone())), 2)
                            }
                            AtomData::Picture(picture) => (
                                item_value_text(&ItemValue::Binary(picture.data().to_vec())),
                                2,
                            ),
                        };
                        fields.push(Extra {
                            name: format!("{:?}", atom.ident()),
                            original_value: value.clone(),
                            value,
                            value_kind,
                            remove: false,
                            source: Source::Mp4(atom.ident().clone().into_owned(), data.clone()),
                        });
                    }
                }
            }
            _ => {}
        }
        Ok(fields)
    }

    pub fn merge_extras(&self, generic: Tag, extras: &[Extra]) -> Result<Native> {
        let extras: Vec<_> = extras.iter().filter(|field| !field.remove).collect();
        Ok(match self.clone() {
            Native::Vorbis(original) => {
                let mut remainder: lofty::ogg::tag::VorbisComments = original.split_tag().0.into();
                remainder.set_vendor(
                    generic
                        .get_string(ItemKey::EncoderSoftware)
                        .unwrap_or("")
                        .into(),
                );
                remainder.take_items().for_each(drop);
                for field in extras {
                    validate_name(TagType::VorbisComments, &field.name)?;
                    remainder.push(field.name.clone(), field.value.clone());
                }
                Native::Vorbis(remainder.split_tag().0.merge_tag(generic))
            }
            Native::Riff(_) => {
                let mut tag: lofty::iff::wav::RiffInfoList = generic.into();
                for field in extras {
                    validate_name(TagType::RiffInfo, &field.name)?;
                    tag.insert(field.name.clone(), field.value.clone());
                }
                Native::Riff(tag)
            }
            Native::Ape(original) => {
                let mut remainder: lofty::ape::ApeTag = original.split_tag().0.into();
                remainder.clear();
                for field in extras {
                    let mut item = lofty::ape::ApeItem::new(
                        field.name.clone(),
                        value_from_text(field.value_kind, &field.value)?,
                    )?;
                    if let Source::Ape(original) = &field.source {
                        item.read_only = original.read_only;
                    }
                    remainder.push(item);
                }
                Native::Ape(remainder.split_tag().0.merge_tag(generic))
            }
            Native::Id3v2(original) => {
                let mut remainder: Id3v2Tag = original.split_tag().0.into();
                remainder.clear();
                for field in extras {
                    remainder.insert(field.id3_frame()?);
                }
                Native::Id3v2(remainder.split_tag().0.merge_tag(generic))
            }
            Native::Mp4(original) => {
                let mut remainder: lofty::mp4::Ilst = original.split_tag().0.into();
                remainder.clear();
                for field in extras {
                    let Source::Mp4(ident, _) = &field.source else {
                        bail!("Неверный тип поля MP4.");
                    };
                    remainder.insert(
                        Atom::from_collection(ident.clone(), vec![field.mp4_data()?])
                            .context("Пустое поле MP4.")?,
                    );
                }
                Native::Mp4(remainder.split_tag().0.merge_tag(generic))
            }
            Native::Id3v1(tag) => Native::Id3v1(tag.split_tag().0.merge_tag(generic)),
            Native::Aiff(tag) => Native::Aiff(tag.split_tag().0.merge_tag(generic)),
        })
    }

    pub fn save_direct(&self, file: &mut std::fs::File, options: WriteOptions) -> Result<()> {
        match self {
            Native::Id3v2(tag) => tag.save_to(file, options)?,
            Native::Id3v1(tag) => tag.save_to(file, options)?,
            Native::Vorbis(tag) => tag.save_to(file, options)?,
            Native::Mp4(tag) => tag.save_to(file, options)?,
            Native::Ape(tag) => tag.save_to(file, options)?,
            Native::Riff(tag) => tag.save_to(file, options)?,
            Native::Aiff(tag) => tag.save_to(file, options)?,
        }
        Ok(())
    }
}

pub(super) fn signatures(fields: &[Extra]) -> Vec<(String, String, usize)> {
    fields
        .iter()
        .map(|field| (field.name.clone(), field.value.clone(), field.value_kind))
        .collect()
}
