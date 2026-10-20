//! A device frame's body on the canvas (View ▸ Device bezels): the phone or tablet drawn round
//! the frame in code, as the presenter draws its devices — a rounded slab with a coloured rim and
//! black glass, side buttons on phones, the screen's rounded corners and its cutouts in black. Only
//! ever outside the screen (and in its corners and cutouts), so the design stays visible.

use egui::{Color32, Mesh, Painter, Pos2, Rect, Shape, Stroke, pos2, vec2};
use figcraft_doc::{Document, NodeId};
use figcraft_geom::{Point, rotation_degrees};

use crate::FigApp;
use crate::present::{GLASS, corner_masks, rim, shade};

/// The body's colour (graphite).
const BODY: [u8; 3] = [0x2C, 0x2C, 0x30];
/// Points per quarter circle of a rounded outline.
const ARC: usize = 10;

/// The frame's screen rectangle and the bezel geometry, in screen pixels: (screen, bezel, rim,
/// screen corner radius, tablet, landscape). `None` for frames without a device or turned ones.
fn geometry(app: &FigApp, doc: &Document, id: NodeId) -> Option<(Rect, f32, f32, f32, bool, bool)> {
    let n = doc.node(id)?;
    let d = n.device.as_ref()?;
    let (w, h) = (n.width, n.height);
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) || rotation_degrees(doc.world_transform(id)).abs() > 1e-6 {
        return None;
    }
    let m = doc.world_transform(id);
    let a = m * Point::new(0.0, 0.0);
    let b = m * Point::new(w, h);
    let s = Rect::from_two_pos(app.to_screen(a.x, a.y), app.to_screen(b.x, b.y));
    let z = s.width() / w as f32;
    if !(z.is_finite() && z > 0.0) {
        return None;
    }
    let lower = d.name.to_ascii_lowercase();
    let tablet = lower.contains("ipad") || lower.contains("tab") || w.min(h) >= 600.0;
    let upright = w.min(h) as f32;
    let bezel_pt = (upright * if tablet { 0.05 } else { 0.06 }).max(6.0);
    let radius = d.corner_radius_for(w, h) as f32 * z;
    Some((s, bezel_pt * z, rim(upright, bezel_pt) * z, radius, tablet, w > h))
}

/// How far above the frame the body reaches (screen pixels), so the frame's label clears it.
pub fn lift(app: &FigApp, doc: &Document, id: NodeId) -> f32 {
    geometry(app, doc, id).map_or(0.0, |(_, b, ..)| b)
}

pub fn draw(app: &FigApp, painter: &Painter, doc: &Document, id: NodeId) {
    let Some((s, b, band, r, tablet, landscape)) = geometry(app, doc, id) else { return };
    let (body, edge) = (shade(BODY, 0.0), shade(BODY, -0.35));
    let slab = s.expand(b);
    let outer_r = r + b;
    // Phones have buttons on their sides (above and below when turned).
    if !tablet {
        for (left, f0, len) in [(true, 0.18, 0.06), (true, 0.27, 0.1), (false, 0.24, 0.13)] {
            let t = (b * 0.12).max(1.5);
            let btn = if landscape {
                let y = if left { slab.max.y - t * 0.2 } else { slab.min.y - t };
                Rect::from_min_size(pos2(slab.min.x + slab.width() * f0, y), vec2(slab.width() * len, t * 1.2))
            } else {
                let x = if left { slab.min.x - t } else { slab.max.x - t * 0.2 };
                Rect::from_min_size(pos2(x, slab.min.y + slab.height() * f0), vec2(t * 1.2, slab.height() * len))
            };
            painter.rect_filled(btn, egui::CornerRadius::same(1), edge);
        }
    }
    let inner = slab.shrink(band);
    let inner_r = (outer_r - band).max(0.0);
    painter.add(ring(slab, outer_r, inner, inner_r, body));
    painter.add(ring(inner, inner_r, s, 0.0, GLASS));
    painter.add(Shape::closed_line(rounded(slab, outer_r), Stroke::new(1.0, edge)));
    // The screen's own rounded corners, and its cutouts (notch, island, hole punch) in black.
    if r > 0.0 {
        painter.add(corner_masks(s, r.min(s.width() / 2.0).min(s.height() / 2.0), GLASS));
    }
    let Some(n) = doc.node(id) else { return };
    let Some(d) = &n.device else { return };
    let z = s.width() / n.width as f32;
    for c in d.cutouts_for(n.width, n.height) {
        let cr = Rect::from_min_size(pos2(s.min.x + c.x as f32 * z, s.min.y + c.y as f32 * z), vec2(c.width as f32 * z, c.height as f32 * z));
        let round = (cr.width().min(cr.height()) / 2.0).min(255.0) as u8;
        painter.rect_filled(cr, egui::CornerRadius::same(round), Color32::BLACK);
    }
}

/// The outline of `r` with corners of radius `rad`, clockwise from the top-left arc, `ARC + 1`
/// points per corner (a radius of 0 repeats the corner).
fn rounded(r: Rect, rad: f32) -> Vec<Pos2> {
    use std::f32::consts::{FRAC_PI_2, PI};
    let rad = rad.clamp(0.0, r.width().min(r.height()) / 2.0);
    let centres = [
        (pos2(r.min.x + rad, r.min.y + rad), PI),
        (pos2(r.max.x - rad, r.min.y + rad), PI + FRAC_PI_2),
        (pos2(r.max.x - rad, r.max.y - rad), 0.0),
        (pos2(r.min.x + rad, r.max.y - rad), FRAC_PI_2),
    ];
    let mut pts = Vec::with_capacity(4 * (ARC + 1));
    for (c, start) in centres {
        for k in 0..=ARC {
            let a = start + FRAC_PI_2 * k as f32 / ARC as f32;
            pts.push(c + rad * vec2(a.cos(), a.sin()));
        }
    }
    pts
}

/// The band between two rounded rectangles (`inner` inside `outer`), filled with `color`.
fn ring(outer: Rect, ro: f32, inner: Rect, ri: f32, color: Color32) -> Shape {
    let (o, i) = (rounded(outer, ro), rounded(inner, ri));
    let mut mesh = Mesh::default();
    for (a, b) in o.iter().zip(&i) {
        mesh.colored_vertex(*a, color);
        mesh.colored_vertex(*b, color);
    }
    let n = o.len().min(i.len()) as u32;
    for k in 0..n {
        let (a0, b0, a1, b1) = (2 * k, 2 * k + 1, 2 * ((k + 1) % n), 2 * ((k + 1) % n) + 1);
        mesh.add_triangle(a0, b0, a1);
        mesh.add_triangle(a1, b0, b1);
    }
    Shape::mesh(mesh)
}

#[cfg(test)]
mod tests {
    use egui::{pos2, vec2};

    use super::{ARC, ring, rounded};

    #[test]
    fn outlines_stay_in_their_box_and_rings_are_closed() {
        let r = egui::Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 200.0));
        for rad in [0.0, 12.0, 1e6] {
            let pts = rounded(r, rad);
            assert_eq!(pts.len(), 4 * (ARC + 1));
            assert!(pts.iter().all(|p| r.expand(1e-3).contains(*p)), "{rad}");
        }
        let egui::Shape::Mesh(m) = ring(r.expand(10.0), 30.0, r, 0.0, egui::Color32::BLACK) else { panic!("a mesh") };
        assert_eq!(m.vertices.len(), 2 * 4 * (ARC + 1));
        assert_eq!(m.indices.len(), 6 * 4 * (ARC + 1));
        assert!(m.indices.iter().all(|i| (*i as usize) < m.vertices.len()));
    }
}
