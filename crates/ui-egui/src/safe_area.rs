//! A device frame's safe area on the canvas (View ▸ Safe areas): the bands outside the safe area
//! tinted, the safe area's edge dashed, the display cutouts filled and the screen's rounded
//! corners outlined, all in the frame's own coordinates (they turn with it).

use egui::{Painter, Pos2, Shape, Stroke};
use figcraft_doc::{Document, NodeId};
use figcraft_geom::Point;

use crate::FigApp;
use crate::theme::Tokens;

/// Points per quarter circle of a rounded corner.
const ARC_STEPS: usize = 8;

pub fn draw(app: &FigApp, painter: &Painter, t: &Tokens, doc: &Document, id: NodeId) {
    let Some(n) = doc.node(id) else { return };
    let Some(d) = &n.device else { return };
    let (w, h) = (n.width, n.height);
    if !(w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return;
    }
    let m = doc.world_transform(id);
    let to_screen = |x: f64, y: f64| {
        let p = m * Point::new(x, y);
        app.to_screen(p.x, p.y)
    };
    let quad = |x0: f64, y0: f64, x1: f64, y1: f64| vec![to_screen(x0, y0), to_screen(x1, y0), to_screen(x1, y1), to_screen(x0, y1)];
    let i = d.insets_for(w, h);
    let band = t.danger.gamma_multiply(0.16);
    // Top and bottom bands span the frame; the side bands fill between them.
    let bands = [(0.0, 0.0, w, i.top), (0.0, h - i.bottom, w, h), (0.0, i.top, i.left, h - i.bottom), (w - i.right, i.top, w, h - i.bottom)];
    for (x0, y0, x1, y1) in bands {
        if x1 > x0 && y1 > y0 {
            painter.add(Shape::convex_polygon(quad(x0, y0, x1, y1), band, Stroke::NONE));
        }
    }
    let edge = Stroke::new(1.0, t.danger.gamma_multiply(0.8));
    if i.top + i.bottom + i.left + i.right > 0.0 {
        let mut q = quad(i.left, i.top, w - i.right, h - i.bottom);
        if let Some(first) = q.first().copied() {
            q.push(first);
        }
        painter.extend(Shape::dashed_line(&q, edge, 4.0, 3.0));
    }
    for c in d.cutouts_for(w, h) {
        painter.add(Shape::convex_polygon(quad(c.x, c.y, c.x + c.width, c.y + c.height), t.danger.gamma_multiply(0.45), edge));
    }
    let r = d.corner_radius_for(w, h);
    if r > 0.0 {
        painter.add(Shape::closed_line(
            rounded_outline(w, h, r).into_iter().map(|(x, y)| to_screen(x, y)).collect::<Vec<Pos2>>(),
            Stroke::new(1.0, t.text_secondary),
        ));
    }
}

/// The outline of a `w × h` box with corners of radius `r`, clockwise from the top-left arc.
fn rounded_outline(w: f64, h: f64, r: f64) -> Vec<(f64, f64)> {
    use std::f64::consts::{FRAC_PI_2, PI};
    let centres = [(r, r, PI), (w - r, r, PI + FRAC_PI_2), (w - r, h - r, 0.0), (r, h - r, FRAC_PI_2)];
    let mut pts = Vec::with_capacity(4 * (ARC_STEPS + 1));
    for (cx, cy, start) in centres {
        for k in 0..=ARC_STEPS {
            let a = start + FRAC_PI_2 * k as f64 / ARC_STEPS as f64;
            pts.push((cx + r * a.cos(), cy + r * a.sin()));
        }
    }
    pts
}

#[cfg(test)]
mod tests {
    use super::rounded_outline;

    #[test]
    fn rounded_outline_stays_in_the_box_and_touches_each_side() {
        let pts = rounded_outline(100.0, 200.0, 20.0);
        assert!(pts.iter().all(|(x, y)| (-1e-9..=100.0 + 1e-9).contains(x) && (-1e-9..=200.0 + 1e-9).contains(y)));
        for touches in [
            pts.iter().any(|p| p.1.abs() < 1e-9),
            pts.iter().any(|p| (p.1 - 200.0).abs() < 1e-9),
            pts.iter().any(|p| p.0.abs() < 1e-9),
            pts.iter().any(|p| (p.0 - 100.0).abs() < 1e-9),
        ] {
            assert!(touches);
        }
    }
}
