use crate::fixture::{ATLAS, SKELETON_JSON};
use crate::*;

fn asset() -> Asset {
    Asset::load(SKELETON_JSON.as_bytes(), false, ATLAS).unwrap()
}

#[test]
fn loads_the_fixture() {
    let a = asset();
    let info = a.info();
    assert_eq!(info.version.as_deref(), Some("4.3.00"));
    assert_eq!(info.animations, vec![AnimationInfo { name: "wave".into(), duration: 1.0 }]);
    assert_eq!(info.skins, vec!["default".to_string(), "bare".to_string()]);
    assert_eq!(info.pages, vec!["figure.png".to_string()]);
    let b = info.setup_bounds.unwrap();
    // Body 32×32 centred at (0, 16) in y-up → x -16..16, y -32..0 once flipped down; the arm
    // reaches from x 16 to 48.
    assert!((b[0] + 16.0).abs() < 1e-3, "{b:?}");
    assert!((b[2] - 48.0).abs() < 1e-3, "{b:?}");
    assert!((b[1] + 32.0).abs() < 1e-3, "{b:?}");
}

#[test]
fn setup_pose_has_a_region_and_a_mesh() {
    let f = asset().pose(Pose::default()).unwrap();
    let verts: usize = f.batches.iter().map(|b| b.positions.len()).sum();
    let tris: usize = f.batches.iter().map(|b| b.indices.len()).sum();
    assert_eq!(verts, 8);
    assert_eq!(tris, 12);
    for b in &f.batches {
        assert_eq!(b.page, "figure.png");
        assert!(b.indices.iter().all(|&i| (i as usize) < b.positions.len()));
        assert!(b.uvs.iter().flatten().all(|u| (0.0..=1.0).contains(u)));
    }
}

#[test]
fn wave_rotates_the_arm_up() {
    let a = asset();
    let at = |t: f32| a.pose(Pose { animation: Some("wave"), time: t, ..Pose::default() }).unwrap().bounds().unwrap();
    let start = at(0.0);
    let end = at(1.0);
    // Rotated 90° counter-clockwise (y up) the arm points up: higher (smaller y) and narrower.
    assert!(end[1] < start[1] - 10.0, "{start:?} {end:?}");
    assert!(end[2] < start[2] - 10.0, "{start:?} {end:?}");
    // Holding past the end without looping; wrapping with it.
    assert_eq!(at(1.0), at(5.0));
    let looped = a.pose(Pose { animation: Some("wave"), time: 2.0, looping: true, ..Pose::default() }).unwrap();
    assert_eq!(looped.bounds(), Some(start));
}

#[test]
fn skins_change_attachments() {
    let a = asset();
    let bare = a.pose(Pose { skin: Some("bare"), ..Pose::default() }).unwrap();
    // `bare` has no attachments of its own; the default skin's still show.
    assert!(!bare.batches.is_empty());
}

#[test]
fn unknown_names_are_errors() {
    let a = asset();
    assert!(a.pose(Pose { animation: Some("nope"), ..Pose::default() }).is_err());
    assert!(a.pose(Pose { skin: Some("nope"), ..Pose::default() }).is_err());
}

#[test]
fn junk_times_are_clamped() {
    let a = asset();
    for t in [f32::NAN, f32::INFINITY, -1.0, 1.0e30] {
        assert!(a.pose(Pose { animation: Some("wave"), time: t, looping: true, ..Pose::default() }).is_ok());
    }
}

#[test]
fn junk_input_never_panics() {
    let skel = SKELETON_JSON.as_bytes();
    let mut seed = 0x2545_f491_u32;
    for n in 0..300 {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let cut = (seed as usize) % skel.len();
        let mut bytes = skel[..cut].to_vec();
        if n % 3 == 0 {
            bytes.extend_from_slice(&skel[cut..]);
            if let Some(b) = bytes.get_mut(cut) {
                *b = (seed >> 8) as u8;
            }
        }
        let _ = Asset::load(&bytes, false, ATLAS);
        let _ = Asset::load(&bytes, true, ATLAS);
        let _ = Asset::load(skel, false, ATLAS.get(..cut % ATLAS.len()).unwrap_or(""));
    }
    assert!(Asset::load(b"", false, "").is_err());
    assert!(Asset::load(&[0xff; 64], true, ATLAS).is_err());
}

#[test]
fn sniffs_skeleton_json() {
    assert!(is_skeleton_json(SKELETON_JSON.as_bytes()));
    assert!(!is_skeleton_json(br#"{"name":"doc","document":{}}"#));
    assert!(!is_skeleton_json(br#"{"skeleton":{}}"#));
    assert!(!is_skeleton_json(b"\xff\xfe"));
}
