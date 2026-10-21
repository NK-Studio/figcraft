//! The Spine host (`tools/spine-host`, `docs/spine.md`): builds `figcraft-spine-host-4.2` and
//! `-4.3` with CMake against the official spine-cpp of each version and copies them beside the
//! debug and release builds, where the app finds them. spine-cpp (Spine Runtimes License) never
//! enters the repository: it is read from `SPINE_RUNTIMES_DIR` (default `../spine-runtimes`), one
//! checkout per version (`<dir>/4.2`, `<dir>/4.3`); `--fetch` makes sparse clones there.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{root, run};

const REPO: &str = "https://github.com/EsotericSoftware/spine-runtimes.git";

/// spine-cpp's folder (holding `include/` and `src/`) in a checkout: 4.2 nests it one deeper.
fn spine_cpp(checkout: &Path) -> Option<PathBuf> {
    [checkout.join("spine-cpp/spine-cpp"), checkout.join("spine-cpp")].into_iter().find(|d| d.join("include/spine/spine.h").is_file())
}

fn fetch(dir: &Path, version: &str) -> bool {
    let checkout = dir.join(version);
    if checkout.exists() {
        return true;
    }
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let co = checkout.to_string_lossy().into_owned();
    run(Command::new("git").args(["clone", "--depth", "1", "--filter=blob:none", "--sparse", "-b", version, REPO, &co]))
        && run(Command::new("git").args(["-C", &co, "sparse-checkout", "set", "spine-cpp"]))
}

pub fn build(args: &[String]) -> bool {
    let root = root();
    let dir = std::env::var_os("SPINE_RUNTIMES_DIR").map(PathBuf::from).unwrap_or_else(|| root.join("../spine-runtimes"));
    let fetching = args.iter().any(|a| a == "--fetch");
    let target = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from).unwrap_or_else(|| root.join("target"));
    let mut built = 0;
    for version in ["4.2", "4.3"] {
        if fetching && !fetch(&dir, version) {
            eprintln!("spine-host: couldn't fetch spine-runtimes {version}");
            return false;
        }
        let Some(cpp) = spine_cpp(&dir.join(version)) else {
            eprintln!("spine-host: no spine-cpp {version} in {} (pass --fetch, or set SPINE_RUNTIMES_DIR)", dir.join(version).display());
            continue;
        };
        let build = target.join(format!("spine-host/build-{version}"));
        let (b, c) = (build.to_string_lossy().into_owned(), cpp.to_string_lossy().into_owned());
        let src = root.join("tools/spine-host").to_string_lossy().into_owned();
        let ok = run(Command::new("cmake").args([
            "-S",
            &src,
            "-B",
            &b,
            &format!("-DSPINE_CPP_DIR={c}"),
            &format!("-DSPINE_HOST_VERSION={}", version.replace('.', "")),
            &format!("-DSPINE_HOST_OUTPUT_SUFFIX={version}"),
            "-DCMAKE_BUILD_TYPE=Release",
        ])) && run(Command::new("cmake").args(["--build", &b, "--config", "Release", "--parallel"]));
        if !ok {
            eprintln!("spine-host: building {version} failed (CMake and a C++ compiler are needed)");
            return false;
        }
        let name = format!("figcraft-spine-host-{version}{}", std::env::consts::EXE_SUFFIX);
        // Single-config generators put it in the build folder, Visual Studio in Release/.
        let Some(exe) = [build.join(&name), build.join("Release").join(&name)].into_iter().find(|p| p.is_file()) else {
            eprintln!("spine-host: {name} wasn't built");
            return false;
        };
        for profile in ["debug", "release"] {
            let out = target.join(profile);
            if std::fs::create_dir_all(&out).and_then(|_| std::fs::copy(&exe, out.join(&name))).is_err() {
                eprintln!("spine-host: couldn't copy {name} into {}", out.display());
                return false;
            }
        }
        eprintln!("spine-host: {name} → {}/{{debug,release}}", target.display());
        built += 1;
    }
    if built == 0 {
        eprintln!("spine-host: nothing built");
    }
    built > 0
}
