//! Spine skeletons in a `.figcraft` archive (`docs/spine.md`): `spine/index.json`
//! (`{"skeletons": [{key, name, binary, pages: {file: imageHash}}]}`) plus
//! `spine/<key>/skeleton` and `spine/<key>/atlas` per skeleton. Atlas page images are ordinary
//! `images/` members. Readers that don't know about Spine ignore these entries.

use std::collections::BTreeMap;
use std::io::{Read, Seek, Write};
use std::sync::Arc;

use figcraft_doc::{Document, SpineAsset};
use serde::{Deserialize, Serialize};

use crate::{FormatError, add_image, io_err, json_options, put, read_member};

const INDEX: &str = "spine/index.json";
/// Largest skeleton or atlas member read.
const MAX_SPINE_MEMBER: u64 = 64 * 1024 * 1024;
/// Most skeletons read from one file.
const MAX_SKELETONS: usize = 4096;

#[derive(Serialize, Deserialize)]
struct Entry {
    key: String,
    name: String,
    #[serde(default)]
    binary: bool,
    #[serde(default)]
    pages: BTreeMap<String, String>,
}

#[derive(Serialize, Deserialize)]
struct Index {
    skeletons: Vec<Entry>,
}

/// The key a skeleton is kept under: the hash of its skeleton and atlas files.
pub fn spine_key(skeleton: &[u8], atlas: &str) -> String {
    let mut all = Vec::with_capacity(skeleton.len() + atlas.len() + 1);
    all.extend_from_slice(skeleton);
    all.push(0);
    all.extend_from_slice(atlas.as_bytes());
    crate::image_hash(&all)
}

fn valid_key(key: &str) -> bool {
    key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Register a Spine export with the document: the skeleton file (`binary`: `.skel`), its atlas
/// text and the atlas page images (file name as the atlas names it, encoded bytes). Returns the
/// skeleton's key.
pub fn add_spine(
    doc: &mut Document,
    name: &str,
    skeleton: Vec<u8>,
    binary: bool,
    atlas: String,
    pages: Vec<(String, Vec<u8>)>,
) -> Result<String, FormatError> {
    let key = spine_key(&skeleton, &atlas);
    let mut hashes = BTreeMap::new();
    for (file, bytes) in pages {
        let (hash, _, _) = add_image(doc, bytes).map_err(|e| FormatError::Corrupt(format!("atlas page {file}: {e}")))?;
        hashes.insert(file, hash);
    }
    doc.spine.insert(key.clone(), SpineAsset { name: name.to_string(), skeleton: Arc::new(skeleton), binary, atlas, pages: hashes });
    Ok(key)
}

pub(crate) fn write<W: Write + Seek>(z: &mut zip::ZipWriter<W>, doc: &Document) -> Result<(), FormatError> {
    if doc.spine.is_empty() {
        return Ok(());
    }
    let mut index = Index { skeletons: Vec::with_capacity(doc.spine.len()) };
    for (key, a) in &doc.spine {
        if !valid_key(key) {
            return Err(FormatError::Io(format!("invalid Spine skeleton key `{key}`")));
        }
        put(z, &format!("spine/{key}/skeleton"), json_options(), &a.skeleton)?;
        put(z, &format!("spine/{key}/atlas"), json_options(), a.atlas.as_bytes())?;
        index.skeletons.push(Entry { key: key.clone(), name: a.name.clone(), binary: a.binary, pages: a.pages.clone() });
    }
    put(z, INDEX, json_options(), &serde_json::to_vec(&index).map_err(io_err)?)
}

/// The archive's skeletons; damaged ones are skipped and reported in `issues`.
pub(crate) fn read<R: Read + Seek>(z: &mut zip::ZipArchive<R>, issues: &mut Vec<String>) -> BTreeMap<String, SpineAsset> {
    let mut out = BTreeMap::new();
    if z.by_name(INDEX).is_err() {
        return out;
    }
    let index = match read_member(z, INDEX, MAX_SPINE_MEMBER)
        .and_then(|b| serde_json::from_slice::<Index>(&b).map_err(|e| FormatError::Corrupt(e.to_string())))
    {
        Ok(i) => i,
        Err(e) => {
            issues.push(format!("the Spine skeletons were skipped: {e}"));
            return out;
        }
    };
    for e in index.skeletons.into_iter().take(MAX_SKELETONS) {
        if !valid_key(&e.key) {
            issues.push(format!("a Spine skeleton with an invalid key was skipped: {}", e.name));
            continue;
        }
        let skeleton = read_member(z, &format!("spine/{}/skeleton", e.key), MAX_SPINE_MEMBER);
        let atlas = read_member(z, &format!("spine/{}/atlas", e.key), MAX_SPINE_MEMBER)
            .and_then(|b| String::from_utf8(b).map_err(|e| FormatError::Corrupt(e.to_string())));
        match (skeleton, atlas) {
            (Ok(skeleton), Ok(atlas)) => {
                out.insert(e.key, SpineAsset { name: e.name, skeleton: Arc::new(skeleton), binary: e.binary, atlas, pages: e.pages });
            }
            (Err(err), _) | (_, Err(err)) => issues.push(format!("Spine skeleton {} was skipped: {err}", e.name)),
        }
    }
    out
}
