use super::{Cover, Document, Extra, Field};
use anyhow::{Result, ensure};
use lofty::{
    picture::PictureType,
    tag::{ItemKey, TagType},
};

#[derive(Clone, Default)]
pub struct BatchEdits {
    pub fields: Vec<(TagType, ItemKey)>,
    pub extras: Vec<(TagType, String)>,
    pub covers: Vec<(TagType, PictureType)>,
}

impl BatchEdits {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.extras.is_empty() && self.covers.is_empty()
    }
}

impl Document {
    /// Apply only the edited keys and explicit cleanup choices, preserving each file's other tags.
    pub fn apply_batch(&self, edits: &BatchEdits, target: &mut Document) -> Result<()> {
        if self.blocks.iter().all(|block| block.remove) {
            for block in &mut target.blocks {
                block.remove = true;
            }
            return Ok(());
        }
        for (index, source) in self.blocks.iter().enumerate() {
            let forced = edits.fields.iter().any(|(kind, _)| *kind == source.kind)
                || edits.extras.iter().any(|(kind, _)| *kind == source.kind)
                || edits.covers.iter().any(|(kind, _)| *kind == source.kind);
            if !source.changed()
                && !forced
                && !source.remove
                && !source.clear_fields
                && !source.clear_covers
            {
                continue;
            }
            // The main block maps to the other file's main format (e.g. ID3v2 to Vorbis).
            let destination = if index == 0 {
                Some(0)
            } else {
                target
                    .blocks
                    .iter()
                    .position(|block| block.kind == source.kind)
            };
            let Some(destination) = destination else {
                if source.remove || source.clear_fields || source.clear_covers {
                    continue;
                }
                anyhow::bail!(
                    "Файл не поддерживает блок {}.",
                    super::type_name(source.kind)
                );
            };
            let block = &mut target.blocks[destination];
            ensure!(
                block.writable,
                "Блок {} недоступен для записи.",
                super::type_name(block.kind)
            );
            if source.remove {
                block.remove = true;
                continue;
            }
            block.clear_fields = source.clear_fields;
            block.clear_covers = source.clear_covers;

            let mut keys = Vec::new();
            for key in source
                .original
                .items()
                .map(|item| item.key())
                .chain(source.fields.iter().map(|field| field.item.key()))
                .chain(
                    edits
                        .fields
                        .iter()
                        .filter(|(kind, _)| *kind == source.kind)
                        .map(|(_, key)| *key),
                )
            {
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
            for key in keys {
                let before: Vec<_> = source
                    .original
                    .items()
                    .filter(|item| item.key() == key)
                    .collect();
                let after: Vec<_> = source
                    .fields
                    .iter()
                    .filter(|field| !field.remove && field.item.key() == key)
                    .map(|field| &field.item)
                    .collect();
                if before == after && !edits.fields.contains(&(source.kind, key)) {
                    continue;
                }
                ensure!(
                    after.is_empty() || super::field_keys(block.kind).contains(&key),
                    "Формат {} не поддерживает поле {:?}.",
                    super::type_name(block.kind),
                    key
                );
                block.fields.retain(|field| field.item.key() != key);
                block.fields.extend(after.into_iter().map(|item| Field {
                    item: item.clone(),
                    remove: false,
                }));
            }

            let mut names = Vec::new();
            for name in source
                .original_extras
                .iter()
                .map(|item| &item.0)
                .chain(source.extras.iter().map(|item| &item.name))
                .chain(
                    edits
                        .extras
                        .iter()
                        .filter(|(kind, _)| *kind == source.kind)
                        .map(|(_, name)| name),
                )
            {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
            for name in names {
                let before: Vec<_> = source
                    .original_extras
                    .iter()
                    .filter(|item| item.0 == name)
                    .cloned()
                    .collect();
                let after: Vec<_> = source
                    .extras
                    .iter()
                    .filter(|item| item.name == name && !item.remove)
                    .cloned()
                    .collect();
                if before == super::extra::signatures(&after)
                    && !edits.extras.contains(&(source.kind, name.clone()))
                {
                    continue;
                }
                block.extras.retain(|item| item.name != name);
                for item in after {
                    if source.kind == block.kind {
                        block.extras.push(item);
                    } else {
                        ensure!(
                            item.value_kind == 0,
                            "Поле {} нельзя перенести в другой формат тегов.",
                            name
                        );
                        block
                            .extras
                            .push(Extra::create(block.kind, &item.name, &item.value)?);
                    }
                }
            }

            let mut kinds = Vec::new();
            for kind in source
                .original
                .pictures()
                .iter()
                .map(|p| p.pic_type())
                .chain(source.covers.iter().map(|cover| cover.picture.pic_type()))
                .chain(
                    edits
                        .covers
                        .iter()
                        .filter(|(kind, _)| *kind == source.kind)
                        .map(|(_, kind)| *kind),
                )
            {
                if !kinds.contains(&kind) {
                    kinds.push(kind);
                }
            }
            for kind in kinds {
                let before: Vec<_> = source
                    .original
                    .pictures()
                    .iter()
                    .filter(|p| p.pic_type() == kind)
                    .collect();
                let after: Vec<_> = source
                    .covers
                    .iter()
                    .filter(|c| !c.remove && c.picture.pic_type() == kind)
                    .map(|c| &c.picture)
                    .collect();
                if before == after && !edits.covers.contains(&(source.kind, kind)) {
                    continue;
                }
                block.covers.retain(|c| c.picture.pic_type() != kind);
                block.covers.extend(after.into_iter().map(|picture| Cover {
                    picture: picture.clone(),
                    remove: false,
                }));
            }
        }
        Ok(())
    }
}
