//! Tags of a Free Music download: title, artist, album, genre, the licence and
//! the attribution, and the cover.
//!
//! The part of Vivi's Auto-tagger (`plugins/auto_tagger`, MIT, Copyright (c)
//! 2026 Ruslan Popelyshyn) that Free Music uses: its `Tags` fields and
//! `write_tags`, without lyrics.

use lofty::config::WriteOptions;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::*;
use lofty::tag::{ItemKey, Tag};
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Tags {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub date: String,
    pub genre: String,
    pub copyright: String,
    pub license: String,
    pub comment: String,
}

impl Tags {
    fn fields(&self) -> [(ItemKey, &str); 9] {
        [
            (ItemKey::TrackTitle, &self.title),
            (ItemKey::TrackArtist, &self.artist),
            (ItemKey::AlbumTitle, &self.album),
            (ItemKey::AlbumArtist, &self.album_artist),
            (ItemKey::RecordingDate, &self.date),
            (ItemKey::Genre, &self.genre),
            (ItemKey::CopyrightMessage, &self.copyright),
            // Not every format has a licence field (ID3v2 has none): the licence
            // URL is also in the copyright message and the comment.
            (ItemKey::License, &self.license),
            (ItemKey::Comment, &self.comment),
        ]
    }
}

fn mime_of(b: &[u8]) -> Option<MimeType> {
    match b {
        [0x89, b'P', b'N', b'G', ..] => Some(MimeType::Png),
        [0xFF, 0xD8, ..] => Some(MimeType::Jpeg),
        _ => None,
    }
}

/// Write `t` (non-empty fields) and the front cover into the file at `path`.
pub fn write_tags(path: &Path, t: &Tags, cover: Option<&[u8]>) -> Result<(), String> {
    let mut tf = lofty::read_from_path(path).map_err(|e| e.to_string())?;
    let tt = tf.primary_tag_type();
    if tf.tag(tt).is_none() {
        tf.insert_tag(Tag::new(tt));
    }
    let tag = tf.tag_mut(tt).ok_or("cannot create a tag")?;
    for (key, v) in t.fields() {
        if !v.is_empty() {
            tag.insert_text(key, v.to_string());
        }
    }
    if let Some(c) = cover.filter(|c| mime_of(c).is_some()) {
        tag.remove_picture_type(PictureType::CoverFront);
        tag.push_picture(
            Picture::unchecked(c.to_vec())
                .pic_type(PictureType::CoverFront)
                .mime_type(mime_of(c).unwrap_or(MimeType::Jpeg))
                .build(),
        );
    }
    tag.save_to_path(path, WriteOptions::default())
        .map_err(|e| e.to_string())
}
