//! Spine skeletons survive saving and loading.

use std::io::Write;

use figcraft_doc::{Document, NodeKind, SpineData};

use super::*;

fn png() -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(4, 2, image::Rgba([0, 90, 0, 255]));
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png).unwrap();
    out
}

fn doc_with_spine() -> (Document, String) {
    let mut d = Document::new("File");
    let key =
        add_spine(&mut d, "hero", b"{\"skeleton\":{}}".to_vec(), false, "page.png\nsize: 4,2\n".into(), vec![("page.png".into(), png())]).unwrap();
    let p = d.first_page().unwrap();
    let data = SpineData {
        asset: key.clone(),
        animation: Some("run".into()),
        skin: None,
        time: 0.5,
        looping: true,
        playing: true,
        speed: 2.0,
        frame: [-8.0, -16.0, 16.0, 16.0],
    };
    d.create(p, NodeKind::Spine(data), "hero", 0.0, 0.0, 16.0, 16.0).unwrap();
    (d, key)
}

#[test]
fn skeletons_round_trip() {
    let (d, key) = doc_with_spine();
    let bytes = save_bytes(&d, None).unwrap();
    let (back, issues) = load_bytes(&bytes).unwrap();
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(back.spine, d.spine);
    let a = &back.spine[&key];
    assert_eq!(a.name, "hero");
    assert!(back.images.contains_key(&a.pages["page.png"]));
    let node = back.nodes().find(|n| matches!(n.kind, NodeKind::Spine(_))).unwrap();
    let s = if let NodeKind::Spine(s) = &node.kind { Some(s) } else { None }.unwrap();
    assert_eq!(s.asset, key);
    assert_eq!(s.animation.as_deref(), Some("run"));
    assert_eq!(s.speed, 2.0);
}

#[test]
fn keys_follow_content() {
    assert_eq!(spine_key(b"a", "b"), spine_key(b"a", "b"));
    assert_ne!(spine_key(b"a", "b"), spine_key(b"ab", ""));
}

#[test]
fn a_damaged_index_is_reported_not_fatal() {
    let (d, _) = doc_with_spine();
    let bytes = save_bytes(&d, None).unwrap();
    let mut src = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..src.len() {
        let mut f = src.by_index(i).unwrap();
        let name = f.name().to_string();
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut data).unwrap();
        out.start_file(name.as_str(), zip::write::SimpleFileOptions::default()).unwrap();
        out.write_all(if name == "spine/index.json" { b"{\"skeletons\": [{\"key\": \"../x\", \"name\": \"bad\"}]}" } else { &data }).unwrap();
    }
    let bytes = out.finish().unwrap().into_inner();
    let (back, issues) = load_bytes(&bytes).unwrap();
    assert!(back.spine.is_empty());
    assert!(issues.iter().any(|i| i.contains("invalid key")), "{issues:?}");
}
