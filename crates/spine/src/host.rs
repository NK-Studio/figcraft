//! The Spine host (`tools/spine-host`): the official spine-cpp runtime in a process of its own,
//! one per loaded skeleton. A damaged file can crash or hang only that process: a crash or a
//! request past its deadline ends it and becomes [`SpineError::Internal`]; it is started again a
//! few times, then the skeleton stays unavailable.
//!
//! Hosts are found as `figcraft-spine-host-<major.minor>` (`.exe` on Windows) in
//! `FIGCRAFT_SPINE_HOST_DIR`, beside the running program, or one folder up from it (test
//! binaries); `cargo xtask spine-host` builds them.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use crate::{AnimationInfo, Batch, Blend, Frame, Info, Pose, SpineError};

/// How long loading and posing may take before the host is ended.
const LOAD_DEADLINE: Duration = Duration::from_secs(10);
const POSE_DEADLINE: Duration = Duration::from_secs(3);
/// How many times a host is started for one skeleton.
const MAX_STARTS: u32 = 3;
/// Most batches, vertices and indices one pose may give.
const MAX_BATCHES: usize = 4096;
const MAX_VERTICES: usize = 1 << 20;

/// The host program for Spine version `version` (`"4.2"`), when one is installed.
pub(crate) fn host_path(version: &str) -> Option<PathBuf> {
    let name = format!("figcraft-spine-host-{version}{}", std::env::consts::EXE_SUFFIX);
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf));
    let dirs = [
        std::env::var_os("FIGCRAFT_SPINE_HOST_DIR").map(PathBuf::from),
        exe_dir.clone(),
        exe_dir.as_deref().and_then(Path::parent).map(Path::to_path_buf),
    ];
    dirs.into_iter().flatten().map(|d| d.join(&name)).find(|p| p.is_file())
}

struct Proc {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
}

impl Drop for Proc {
    fn drop(&mut self) {
        let _ = self.stdin.write_all(b"QUIT\n");
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Proc {
    fn start(exe: &Path) -> Result<Proc, SpineError> {
        let mut cmd = Command::new(exe);
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW: no console flashes up.
            cmd.creation_flags(0x0800_0000);
        }
        let mut child = cmd.spawn().map_err(|e| SpineError::Internal(format!("the Spine host didn't start: {e}")))?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(SpineError::Internal("the Spine host has no pipes".into()));
        };
        let (tx, lines) = channel();
        std::thread::Builder::new()
            .name("spine-host".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| SpineError::Internal(e.to_string()))?;
        Ok(Proc { child, stdin, lines })
    }

    fn send(&mut self, head: &str, body: &[&[u8]]) -> Result<(), SpineError> {
        let lost = |e: std::io::Error| SpineError::Internal(format!("the Spine host stopped: {e}"));
        self.stdin.write_all(head.as_bytes()).map_err(lost)?;
        for b in body {
            self.stdin.write_all(b).map_err(lost)?;
        }
        self.stdin.flush().map_err(lost)
    }

    fn line(&self, deadline: Instant) -> Result<String, SpineError> {
        let left = deadline.saturating_duration_since(Instant::now());
        match self.lines.recv_timeout(left) {
            Ok(l) => Ok(l),
            Err(RecvTimeoutError::Timeout) => Err(SpineError::Internal("the Spine host took too long".into())),
            Err(RecvTimeoutError::Disconnected) => Err(SpineError::Internal("the Spine host stopped (the file may be damaged)".into())),
        }
    }
}

/// A skeleton loaded in a host process.
pub(crate) struct Hosted {
    exe: PathBuf,
    skeleton: Vec<u8>,
    binary: bool,
    atlas: String,
    state: Mutex<(Option<Proc>, u32)>,
}

impl Hosted {
    pub(crate) fn load(exe: PathBuf, skeleton: &[u8], binary: bool, atlas: &str) -> Result<(Hosted, Info), SpineError> {
        let h = Hosted { exe, skeleton: skeleton.to_vec(), binary, atlas: atlas.to_string(), state: Mutex::new((None, 0)) };
        let mut info = {
            let mut st = h.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            let (proc_, info) = h.start()?;
            st.0 = Some(proc_);
            st.1 = 1;
            info
        };
        info.setup_bounds = h.pose(Pose::default())?.bounds();
        Ok((h, info))
    }

    /// A new host with the skeleton loaded, and what the skeleton holds.
    fn start(&self) -> Result<(Proc, Info), SpineError> {
        let mut p = Proc::start(&self.exe)?;
        let head = format!("LOAD {} {} {}\n", u8::from(self.binary), self.skeleton.len(), self.atlas.len());
        p.send(&head, &[&self.skeleton, self.atlas.as_bytes()])?;
        let line = p.line(Instant::now() + LOAD_DEADLINE)?;
        if let Some(msg) = line.strip_prefix("ERR ") {
            return Err(SpineError::Invalid(msg.to_string()));
        }
        let json = line.strip_prefix("OK ").ok_or_else(|| SpineError::Internal("the Spine host answered nonsense".into()))?;
        Ok((p, parse_info(json)?))
    }

    pub(crate) fn pose(&self, pose: Pose<'_>) -> Result<Frame, SpineError> {
        let mut st = self.state.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if st.0.is_none() {
            if st.1 >= MAX_STARTS {
                return Err(SpineError::Internal("the Spine host stopped too often on this skeleton".into()));
            }
            st.1 += 1;
            st.0 = Some(self.start()?.0);
        }
        let Some(p) = st.0.as_mut() else { return Err(SpineError::Internal("no Spine host".into())) };
        let result = request_pose(p, pose);
        if matches!(result, Err(SpineError::Internal(_))) {
            // Crashed, hung or garbled: end it; the next pose starts another.
            st.0 = None;
        }
        result
    }
}

fn request_pose(p: &mut Proc, pose: Pose<'_>) -> Result<Frame, SpineError> {
    let anim = pose.animation.unwrap_or("").as_bytes();
    let skin = pose.skin.unwrap_or("").as_bytes();
    let head = format!("POSE {} {} {} {}\n", pose.time, u8::from(pose.looping), anim.len(), skin.len());
    p.send(&head, &[anim, skin])?;
    let deadline = Instant::now() + POSE_DEADLINE;
    let first = p.line(deadline)?;
    if let Some(msg) = first.strip_prefix("ERR ") {
        return Err(SpineError::Invalid(msg.to_string()));
    }
    let bad = || SpineError::Internal("the Spine host answered nonsense".into());
    let n: usize = first.strip_prefix("FRAME ").and_then(|n| n.trim().parse().ok()).ok_or_else(bad)?;
    if n > MAX_BATCHES {
        return Err(bad());
    }
    let mut frame = Frame::default();
    let mut total = 0usize;
    for _ in 0..n {
        let head = p.line(deadline)?;
        let mut parts = head.splitn(6, ' ');
        let (Some("B"), Some(blend), Some(pma), Some(nv), Some(ni)) = (parts.next(), parts.next(), parts.next(), parts.next(), parts.next()) else {
            return Err(bad());
        };
        let page = parts.next().unwrap_or("").to_string();
        let nv: usize = nv.parse().map_err(|_| bad())?;
        let ni: usize = ni.parse().map_err(|_| bad())?;
        total = total.saturating_add(nv);
        if total > MAX_VERTICES || ni > MAX_VERTICES * 3 {
            return Err(bad());
        }
        let floats = |line: String, tag: &str, want: usize| -> Result<Vec<f32>, SpineError> {
            let rest = line.strip_prefix(tag).ok_or_else(bad)?;
            let v: Vec<f32> = rest.split_ascii_whitespace().map(|t| t.parse::<f32>().map_err(|_| bad())).collect::<Result<_, _>>()?;
            if v.len() != want || v.iter().any(|f| !f.is_finite()) {
                return Err(bad());
            }
            Ok(v)
        };
        let ints = |line: String, tag: &str, want: usize| -> Result<Vec<u32>, SpineError> {
            let rest = line.strip_prefix(tag).ok_or_else(bad)?;
            let v: Vec<u32> = rest.split_ascii_whitespace().map(|t| t.parse::<u32>().map_err(|_| bad())).collect::<Result<_, _>>()?;
            if v.len() != want {
                return Err(bad());
            }
            Ok(v)
        };
        let pos = floats(p.line(deadline)?, "P", nv * 2)?;
        let uv = floats(p.line(deadline)?, "U", nv * 2)?;
        let col = ints(p.line(deadline)?, "C", nv)?;
        let idx = ints(p.line(deadline)?, "I", ni)?;
        if idx.iter().any(|&i| i as usize >= nv) {
            return Err(bad());
        }
        let argb =
            |c: u32| [((c >> 16) & 0xff) as f32 / 255.0, ((c >> 8) & 0xff) as f32 / 255.0, (c & 0xff) as f32 / 255.0, (c >> 24) as f32 / 255.0];
        frame.batches.push(Batch {
            page,
            blend: match blend {
                "1" => Blend::Additive,
                "2" => Blend::Multiply,
                "3" => Blend::Screen,
                _ => Blend::Normal,
            },
            premultiplied: pma == "1",
            positions: pos.chunks_exact(2).map(|c| [c[0], c[1]]).collect(),
            uvs: uv.chunks_exact(2).map(|c| [c[0], c[1]]).collect(),
            colors: col.into_iter().map(argb).collect(),
            indices: idx,
        });
    }
    Ok(frame)
}

fn parse_info(json: &str) -> Result<Info, SpineError> {
    #[derive(serde::Deserialize)]
    struct Raw {
        version: Option<String>,
        #[serde(default)]
        animations: Vec<RawAnim>,
        #[serde(default)]
        skins: Vec<String>,
        #[serde(default)]
        pages: Vec<String>,
    }
    #[derive(serde::Deserialize)]
    struct RawAnim {
        name: String,
        duration: f32,
    }
    let r: Raw = serde_json::from_str(json).map_err(|e| SpineError::Internal(format!("the Spine host answered nonsense: {e}")))?;
    Ok(Info {
        version: r.version,
        animations: r
            .animations
            .into_iter()
            .map(|a| AnimationInfo { name: a.name, duration: if a.duration.is_finite() { a.duration.max(0.0) } else { 0.0 } })
            .collect(),
        skins: r.skins,
        pages: r.pages,
        setup_bounds: None,
    })
}
