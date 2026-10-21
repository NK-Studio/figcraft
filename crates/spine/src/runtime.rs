//! The `spine2d` runtime behind [`crate::Asset`]. Every call into it goes through [`guarded`].

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use spine2d::{AnimationState, AnimationStateData, Atlas, Physics, Skeleton, SkeletonData};

use crate::{AnimationInfo, Batch, Blend, Frame, Info, Pose, SpineError};

pub(crate) struct Loaded {
    data: Arc<SkeletonData>,
    atlas: Atlas,
}

/// Runs `f`, turning a panic inside the runtime into [`SpineError::Internal`].
fn guarded<T>(f: impl FnOnce() -> Result<T, SpineError>) -> Result<T, SpineError> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(p) => {
            let msg =
                p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "panic".into());
            Err(SpineError::Internal(msg))
        }
    }
}

fn invalid(e: impl std::fmt::Display) -> SpineError {
    SpineError::Invalid(e.to_string())
}

impl Loaded {
    pub(crate) fn load(skeleton: &[u8], binary: bool, atlas: &str) -> Result<Loaded, SpineError> {
        guarded(|| {
            let data = if binary {
                SkeletonData::from_skel_bytes(skeleton).map_err(invalid)?
            } else {
                let text = std::str::from_utf8(skeleton).map_err(invalid)?;
                SkeletonData::from_json_str(text).map_err(invalid)?
            };
            let atlas: Atlas = atlas.parse().map_err(invalid)?;
            Ok(Loaded { data, atlas })
        })
    }

    pub(crate) fn info(&self) -> Result<Info, SpineError> {
        let mut info = Info {
            version: self.data.get_version().map(str::to_string),
            animations: self
                .data
                .get_animations()
                .iter()
                .map(|a| AnimationInfo {
                    name: a.get_name().to_string(),
                    duration: if a.get_duration().is_finite() { a.get_duration().max(0.0) } else { 0.0 },
                })
                .collect(),
            skins: self.data.get_skins().iter().map(|s| s.get_name().to_string()).collect(),
            pages: self.atlas.get_pages().iter().map(|p| p.texture_path.clone()).collect(),
            setup_bounds: None,
        };
        info.setup_bounds = self.pose(Pose::default())?.bounds();
        Ok(info)
    }

    pub(crate) fn pose(&self, pose: Pose<'_>) -> Result<Frame, SpineError> {
        guarded(|| {
            let mut sk = Skeleton::new(self.data.clone());
            if let Some(skin) = pose.skin {
                sk.set_skin(Some(skin));
            }
            sk.setup_pose();
            if let Some(name) = pose.animation {
                let mut state = AnimationState::new(AnimationStateData::new(self.data.clone()));
                state.set_animation(0, name, pose.looping);
                state.update(pose.time);
                state.apply(&mut sk);
            }
            sk.update(pose.time);
            // Physics isn't simulated: a pose is a pure function of its time.
            sk.update_world_transform_with_physics(Physics::None);
            let list = spine2d::build_draw_list_with_atlas(&sk, &self.atlas);
            Ok(self.batches(&list))
        })
    }

    /// Splits the runtime's draw list into self-contained batches (y flipped down), dropping draws
    /// whose texture isn't an atlas page (an attachment with no atlas region).
    fn batches(&self, list: &spine2d::DrawList) -> Frame {
        let pages = self.atlas.get_pages();
        let mut out = Frame::default();
        for d in &list.draws {
            let Some(page) = pages.iter().find(|p| p.texture_path == d.texture_path) else { continue };
            let end = d.first_index.saturating_add(d.index_count);
            let Some(idx) = list.indices.get(d.first_index..end) else { continue };
            let (Some(&lo), Some(&hi)) = (idx.iter().min(), idx.iter().max()) else { continue };
            let Some(verts) = list.vertices.get(lo as usize..=hi as usize) else { continue };
            out.batches.push(Batch {
                page: page.texture_path.clone(),
                blend: match d.blend {
                    spine2d::BlendMode::Normal => Blend::Normal,
                    spine2d::BlendMode::Additive => Blend::Additive,
                    spine2d::BlendMode::Multiply => Blend::Multiply,
                    spine2d::BlendMode::Screen => Blend::Screen,
                },
                premultiplied: d.premultiplied_alpha,
                positions: verts.iter().map(|v| [v.position[0], -v.position[1]]).collect(),
                uvs: verts.iter().map(|v| v.uv).collect(),
                colors: verts.iter().map(|v| v.color).collect(),
                indices: idx.iter().map(|i| i - lo).collect(),
            });
        }
        out
    }
}
