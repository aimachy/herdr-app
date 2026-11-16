//! Finding herdr-core (`hc`) on this machine. The app doesn't carry one: it
//! uses the core people installed, or installs the core's own release.
//!
//! Where it looks, in order: `core` in settings.json; this process's PATH;
//! the person's login shell's PATH (an app opened from the Dock or a
//! launcher doesn't get it); then the usual install places. A candidate
//! counts only if `--version` says `hc …`: some window managers ship an
//! unrelated `hc`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

#[cfg(any(target_os = "macos", all(test, unix)))]
mod cli_link;

static BIN: Mutex<Option<PathBuf>> = Mutex::new(None);

#[cfg(target_os = "macos")]
pub fn install_cli_link() {
    if let Err(e) = std::thread::Builder::new().name("cli-link".into()).spawn(|| {
        let result = std::env::current_exe().and_then(|exe| cli_link::ensure(&crate::mods::home(), &exe));
        if let Err(e) = result {
            tracing::warn!("cannot install herdr-app CLI link: {e}");
        }
    }) {
        tracing::warn!("cannot start herdr-app CLI setup: {e}");
    }
}

/// The core the app runs (`hc` until one is found).
pub fn path() -> PathBuf {
    BIN.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone().unwrap_or_else(|| PathBuf::from("hc"))
}

/// Uses this core from now on, and puts its folder first on PATH so mods'
/// `hc()` and anything the app runs find the same one.
pub fn use_core(bin: PathBuf) {
    if let Some(dir) = bin.parent() {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut dirs: Vec<PathBuf> = vec![dir.to_path_buf()];
        dirs.extend(std::env::split_paths(&path).filter(|d| d != dir));
        if let Ok(joined) = std::env::join_paths(dirs) {
            // SAFETY: set once at start, before other threads read the environment.
            unsafe { std::env::set_var("PATH", joined) };
        }
    }
    *BIN.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(bin);
}

/// Finder and Dock launches don't inherit the person's shell PATH.
#[cfg(target_os = "macos")]
pub fn fill_path() {
    if std::os::unix::process::parent_id() != 1 {
        return;
    }
    let home = crate::mods::home();
    let current = std::env::var_os("PATH").unwrap_or_default();
    if let Some(path) = merged_path(login_shell_path().as_deref(), &usual_dirs(&home)[..3], &current) {
        // SAFETY: called at startup, before other threads read the environment.
        unsafe { std::env::set_var("PATH", path) };
    }
}

#[cfg(any(target_os = "macos", test))]
fn merged_path(login: Option<&std::ffi::OsStr>, usual: &[PathBuf], current: &std::ffi::OsStr) -> Option<std::ffi::OsString> {
    let mut dirs: Vec<PathBuf> = login.map(|path| dirs_on(path).collect()).unwrap_or_default();
    dirs.extend_from_slice(usual);
    dirs.extend(dirs_on(current));
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|dir| seen.insert(dir.clone()));
    std::env::join_paths(dirs).ok()
}

/// `hc X.Y.Z` if this binary is herdr-core and runs.
pub fn version(bin: &Path) -> Option<String> {
    let out = crate::spawn::command(bin).arg("--version").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    (out.status.success() && text.starts_with("hc ")).then_some(text)
}

/// The first herdr-core found, or None.
pub fn find(configured: Option<&str>) -> Option<PathBuf> {
    let home = crate::mods::home();
    let pick = |candidates: Vec<PathBuf>| candidates.into_iter().filter(|c| c.is_file()).find(|c| version(c).is_some());
    if let Some(found) = pick(configured.map(|c| vec![expand(c, &home)]).unwrap_or_default()) {
        return Some(found);
    }
    // Tests: HERDR_CORE_DIRS replaces every place Herdr would look.
    if let Some(dirs) = std::env::var_os("HERDR_CORE_DIRS") {
        return pick(std::env::split_paths(&dirs).flat_map(|d| names(&d)).collect());
    }
    // The quick places first; the login shell (slow) only if they have none.
    let path = std::env::var_os("PATH").unwrap_or_default();
    let quick: Vec<PathBuf> = dirs_on(&path).chain(usual_dirs(&home)).flat_map(|d| names(&d)).collect();
    pick(quick).or_else(|| pick(dirs_on(&login_shell_path()?).flat_map(|d| names(&d)).collect()))
}

fn expand(p: &str, home: &Path) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(p),
    }
}

fn names(dir: &Path) -> [PathBuf; 2] {
    let exe = std::env::consts::EXE_SUFFIX;
    [dir.join(format!("herdr-core{exe}")), dir.join(format!("hc{exe}"))]
}

fn dirs_on(path: &std::ffi::OsStr) -> impl Iterator<Item = PathBuf> + '_ {
    std::env::split_paths(path)
}

/// Where herdr-core gets installed: its own installer and Herdr 1.x's
/// (~/.local/bin), Homebrew, mise, Nix, cargo.
fn usual_dirs(home: &Path) -> Vec<PathBuf> {
    if cfg!(windows) {
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| home.join("AppData/Local"));
        return vec![local.join("Programs/herdr-core"), home.join(".cargo/bin")];
    }
    let mut dirs = vec![
        home.join(".local/bin"),
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        home.join(".local/share/mise/shims"),
        home.join(".nix-profile/bin"),
        PathBuf::from("/run/current-system/sw/bin"),
        home.join(".cargo/bin"),
    ];
    if cfg!(target_os = "macos") {
        dirs.push(home.join(".local/share/herdr-core/current/Contents/MacOS"));
        // Older installers placed the bundle directly in herdr-core.
        dirs.push(home.join(".local/share/herdr-core/Herdr Core.app/Contents/MacOS"));
    }
    dirs
}

/// The PATH the person's login shell sets up, or None if it doesn't answer quickly.
fn login_shell_path() -> Option<std::ffi::OsString> {
    if cfg!(windows) {
        return None;
    }
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let mut child = Command::new(shell)
        .args(["-lc", "printf %s \"$PATH\""])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // A slow or interactive shell profile must not hold up the app.
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        if child.try_wait().ok()?.is_some() {
            break;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut out = String::new();
    std::io::Read::read_to_string(&mut child.stdout.take()?, &mut out).ok()?;
    Some(out.into())
}

/// Installs the core's latest release with its own installer (into
/// ~/.local/bin, or HC_INSTALL_DIR), then finds it. Err: what went wrong.
pub fn install() -> Result<PathBuf, String> {
    let releases = std::env::var("HERDR_RELEASES_URL").unwrap_or_else(|_| "https://github.com/aimachy/herdr-core/releases".into());
    let mut cmd = if cfg!(windows) {
        let mut c = crate::spawn::command("powershell");
        c.args(["-NoProfile", "-Command", &format!("irm {releases}/latest/download/install.ps1 | iex")]);
        c
    } else {
        let mut c = crate::spawn::command("sh");
        c.args(["-c", &format!("curl -fsSL {releases}/latest/download/install.sh | sh")]);
        c
    };
    let out = cmd.stdin(Stdio::null()).output().map_err(|e| format!("cannot run the installer: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(if err.is_empty() { "the installer failed".into() } else { err });
    }
    let home = crate::mods::home();
    let dir = std::env::var_os("HC_INSTALL_DIR").map(PathBuf::from).unwrap_or_else(|| {
        if cfg!(windows) {
            usual_dirs(&home).remove(0)
        } else {
            home.join(".local/bin")
        }
    });
    let bin = dir.join(format!("herdr-core{}", std::env::consts::EXE_SUFFIX));
    version(&bin).map(|_| bin).ok_or_else(|| "installed, but herdr-core doesn't run".into())
}

/// Updates the core to its latest release (`hc update`: same generation only,
/// terminals kept). Ok: the version it runs now. Err: what it said, e.g.
/// "belongs to Homebrew: brew upgrade herdr-core".
pub fn update() -> Result<String, String> {
    update_to(None)
}

/// `hc update --version X`: at least X (a newer core stays). Cores before 0.1.5 lack it.
pub fn update_to(at_least: Option<&str>) -> Result<String, String> {
    let mut args = vec!["update".to_string(), "--json".to_string()];
    if let Some(v) = at_least {
        args.extend(["--version".to_string(), v.to_string()]);
    }
    update_with(&args)
}

/// Hands the installed hc a core binary to swap in (`hc update --from <bin>`).
fn update_from(bin: &Path) -> Result<String, String> {
    update_with(&update_from_args(bin))
}

/// The `--from` argument list, kept separate so tests can pin it.
fn update_from_args(bin: &Path) -> Vec<String> {
    vec!["update".to_string(), "--from".to_string(), bin.to_string_lossy().into_owned(), "--json".to_string()]
}

/// Runs one `hc update` form against the installed core and reads its reply:
/// Ok is the version it runs now (`now`), Err is what it said, e.g.
/// "belongs to Homebrew: brew upgrade herdr-core".
fn update_with(args: &[String]) -> Result<String, String> {
    let out = crate::spawn::command(path()).args(args).stdin(Stdio::null()).output().map_err(|e| format!("cannot run hc update: {e}"))?;
    let reply: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    if out.status.success() {
        let now = reply.get("now").and_then(|v| v.as_str()).unwrap_or("the latest").to_owned();
        #[cfg(target_os = "macos")]
        ensure_core_bundle(&now)?;
        return Ok(now);
    }
    let err: serde_json::Value = serde_json::from_slice(&out.stderr).unwrap_or_default();
    let message = err.pointer("/error/message").and_then(|v| v.as_str()).map(str::to_owned);
    let hint = err.pointer("/error/hint").and_then(|v| v.as_str()).map(|h| format!(" ({h})")).unwrap_or_default();
    Err(message.map(|m| m + &hint).unwrap_or_else(|| String::from_utf8_lossy(&out.stderr).trim().to_owned()))
}

// Updating from the fork's own releases. The update check reads the fork's
// GitHub Releases, so the swap has to come from there too: an hc installed
// from upstream updates itself from upstream's list, never sees the fork's
// builds, and still reports success. So the app downloads the fork's build
// itself, checks it, and hands it to the installed hc's `update --from`:
// the swap, the package-manager refusal and the terminal-keeping restart
// all stay hc's own work.

/// One core release list (`core-latest.json`, or a version's `manifest.json`).
#[derive(serde::Deserialize)]
struct CoreManifest {
    version: String,
    generation: Option<u64>,
    targets: std::collections::BTreeMap<String, CoreTarget>,
}

#[derive(serde::Deserialize)]
struct CoreTarget {
    url: String,
    sha256: String,
}

/// This machine's name in a core release list (`linux-x86_64`).
fn core_target() -> String {
    let os = if cfg!(target_os = "macos") { "macos" } else if cfg!(windows) { "windows" } else { "linux" };
    format!("{os}-{}", std::env::consts::ARCH)
}

/// A release list's address: the latest one, or one version's.
fn manifest_url(base: &str, version: Option<&str>) -> String {
    let base = base.trim_end_matches('/');
    match version {
        Some(v) => format!("{base}/download/v{v}/manifest.json"),
        None => format!("{base}/latest/download/core-latest.json"),
    }
}

fn parse_manifest(text: &str) -> Result<CoreManifest, String> {
    serde_json::from_str(text).map_err(|e| format!("the core release list is damaged: {e}"))
}

/// The download a list offers this machine, once it clears `at_least`.
fn download_for<'a>(manifest: &'a CoreManifest, target: &str, at_least: Option<&str>) -> Result<(&'a str, &'a str), String> {
    if let Some(min) = at_least
        && crate::update::newer(min, &manifest.version)
    {
        return Err(format!("the latest core ({}) is older than the {min} this app needs", manifest.version));
    }
    let entry = manifest.targets.get(target).ok_or_else(|| format!("no core build for {target} in {}", manifest.version))?;
    Ok((entry.url.as_str(), entry.sha256.as_str()))
}

/// Downloads `url` to `file` and checks it against `sha` before it is used.
fn fetch_checked(url: &str, sha: &str, file: &Path) -> Result<(), String> {
    let out = crate::spawn::command(crate::update::system("curl"))
        .args(["-fsSL", "--max-time", "600", "-o"])
        .arg(file)
        .arg(url)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        return Err(format!("{url}: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
    if crate::update::sha256(&bytes) != sha.to_ascii_lowercase() {
        return Err("the core download is damaged (checksum mismatch)".into());
    }
    Ok(())
}

/// The herdr-core inside an unpacked release archive: at its root, or in
/// the macOS bundle the archive also carries.
fn unpacked_core(dir: &Path) -> Option<PathBuf> {
    let exe = dir.join(format!("herdr-core{}", std::env::consts::EXE_SUFFIX));
    if exe.is_file() {
        return Some(exe);
    }
    let bundled = dir.join("Herdr Core.app/Contents/MacOS/herdr-core");
    bundled.is_file().then_some(bundled)
}

fn unpack_core(archive: &Path, dir: &Path) -> Result<PathBuf, String> {
    // System32's tar on Windows: it opens the zip the Windows build ships.
    let out = crate::spawn::command(crate::update::system("tar")).arg("-xf").arg(archive).arg("-C").arg(dir).stdin(Stdio::null()).output().map_err(|e| format!("tar: {e}"))?;
    if !out.status.success() {
        return Err(format!("tar: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    unpacked_core(dir).ok_or_else(|| "the core download holds no herdr-core".into())
}

/// Updates the core from the fork's releases: downloads the latest build
/// for this machine, checks its sha256, unpacks it, and lets the installed
/// hc swap it in (`update --from`). `at_least`: swap only when the latest
/// is at least this version. A list from another generation keeps the
/// delegated update, which pins versions exactly. Ok: the version it runs now.
pub fn update_from_releases(at_least: Option<&str>) -> Result<String, String> {
    let base = crate::update::core_releases();
    let manifest = parse_manifest(&crate::update::curl_text(&manifest_url(&base, None))?)?;
    if manifest.generation.is_some_and(|g| g != crate::hc::GENERATION) {
        return update_to(at_least);
    }
    let (url, sha) = download_for(&manifest, &core_target(), at_least)?;
    let dir = std::env::temp_dir().join(format!("herdr-core-update-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let staged = (|| {
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let archive = dir.join(url.rsplit('/').next().unwrap_or("core.tar.gz"));
        fetch_checked(url, sha, &archive)?;
        unpack_core(&archive, &dir)
    })();
    let result = staged.and_then(|bin| update_from(&bin));
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[cfg(any(target_os = "macos", all(test, unix)))]
const BUNDLE_INSTALL_SCRIPT: &str = "installer=$(curl -fsSL \"$1/latest/download/install.sh\") && case \"$installer\" in *HC_BUNDLE_ONLY*) printf '%s\\n' \"$installer\" | sh ;; *) exit 0 ;; esac";

#[cfg(any(target_os = "macos", all(test, unix)))]
fn bundle_completion_path(bin: &Path, home: &Path) -> bool {
    bin.starts_with(home)
        && !bin.ends_with("Contents/MacOS/herdr-core")
        && !["/Cellar/", "/nix/store/", "/mise/installs/"]
            .iter().any(|marker| bin.to_string_lossy().contains(marker))
}

#[cfg(any(target_os = "macos", all(test, unix)))]
fn bundle_completion_writable(bin: &Path) -> bool {
    bin.parent().is_some_and(|dir| {
        Command::new("/bin/sh").args(["-c", "test -w \"$1\"", "sh"]).arg(dir).status().is_ok_and(|status| status.success())
    })
}

/// Old core updaters copied only the bare executable. Finish that upgrade
/// with the current installer, which retains the complete signed app bundle.
#[cfg(target_os = "macos")]
fn ensure_core_bundle(version: &str) -> Result<(), String> {
    let bin = path();
    let resolved = bin.canonicalize().unwrap_or_else(|_| bin.clone());
    if !bundle_completion_path(&resolved, &crate::mods::home()) || !bundle_completion_writable(&resolved) {
        return Ok(());
    }
    // Never replace a contributor's build, package-manager core, or a host app.
    let official = crate::spawn::command("/usr/bin/codesign")
        .args(["--verify", "--strict", "-R", "=identifier \"dev.herdr.core\" and anchor apple generic and certificate leaf[subject.OU] = \"2W9652Q7QR\""])
        .arg(&bin).output().is_ok_and(|out| out.status.success());
    if !official {
        return Ok(());
    }
    let releases = std::env::var("HERDR_RELEASES_URL").unwrap_or_else(|_| "https://github.com/aimachy/herdr-core/releases".into());
    let mut cmd = crate::spawn::command("sh");
    cmd.args([
        "-c",
        BUNDLE_INSTALL_SCRIPT,
        "sh",
        &releases,
    ]);
    cmd.env("HC_BUNDLE_ONLY", "1");
    if let Some(v) = version.strip_prefix("hc ") {
        cmd.env("HC_VERSION", v);
    }
    if resolved.is_absolute()
        && let Some(dir) = resolved.parent()
    {
        cmd.env("HC_INSTALL_DIR", dir);
    }
    let out = cmd
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot finish core bundle installation: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn the_independent_core_bundle_is_a_discovery_fallback() {
        let home = Path::new("/Users/test");
        let dir = home.join(".local/share/herdr-core/current/Contents/MacOS");
        let legacy = home.join(".local/share/herdr-core/Herdr Core.app/Contents/MacOS");
        let dirs = usual_dirs(home);
        assert_eq!(&dirs[dirs.len() - 2..], &[dir.clone(), legacy]);
        assert_eq!(names(&dir)[0], dir.join("herdr-core"));
    }

    #[cfg(unix)]
    #[test]
    fn setup_can_replace_an_unusable_core_before_reconnecting() {
        const CHILD: &str = "HERDR_TEST_RESELECT_CORE";
        if std::env::var_os(CHILD).is_none() {
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "core_bin::tests::setup_can_replace_an_unusable_core_before_reconnecting", "--nocapture"])
                .env(CHILD, "1").output().unwrap();
            assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            return;
        }
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("herdr-core-reselect-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let bad = root.join("bad-hc");
        std::fs::write(&bad, "#!/bin/sh\nif [ \"$1\" = --version ]; then echo hc 0.1.18; exit; fi\nread hello\necho unusable >&2\nexit 1\n").unwrap();
        std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o755)).unwrap();
        for replacement in ["chosen", "installed"] {
            use_core(find(Some(bad.to_str().unwrap())).unwrap());
            assert!(crate::hc::Core::connect().is_err());
            let good = root.join(replacement);
            let hello = serde_json::json!({"result": {"selected": replacement}});
            std::fs::write(&good, format!("#!/bin/sh\nread hello\necho '{hello}'\n")).unwrap();
            std::fs::set_permissions(&good, std::fs::Permissions::from_mode(0o755)).unwrap();
            use_core(good.clone());
            let core = crate::hc::Core::connect().unwrap_or_else(|e| panic!("{replacement}: {e:#}"));
            assert_eq!(path(), good);
            assert_eq!(core.hello["selected"], replacement);
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_path_keeps_shell_order_and_falls_back_to_usual_dirs() {
        let usual = [PathBuf::from("/home/test/.local/bin"), PathBuf::from("/opt/homebrew/bin")];
        let current = std::env::join_paths(["/usr/bin", "/bin"]).unwrap();
        let login = std::env::join_paths(["/custom/bin", "/opt/homebrew/bin", "/usr/bin"]).unwrap();
        let merged = merged_path(Some(&login), &usual, &current).unwrap();
        assert_eq!(std::env::split_paths(&merged).collect::<Vec<_>>(), ["/custom/bin", "/opt/homebrew/bin", "/usr/bin", "/home/test/.local/bin", "/bin"].map(PathBuf::from));
        let fallback = merged_path(None, &usual, &current).unwrap();
        assert_eq!(std::env::split_paths(&fallback).collect::<Vec<_>>(), ["/home/test/.local/bin", "/opt/homebrew/bin", "/usr/bin", "/bin"].map(PathBuf::from));
    }

    #[cfg(unix)]
    #[test]
    fn bundle_completion_skips_managed_and_system_paths() {
        let home = Path::new("/Users/test");
        assert!(bundle_completion_path(&home.join(".local/bin/herdr-core"), home));
        for bin in [
            "/usr/local/bin/herdr-core",
            "/opt/homebrew/Cellar/herdr-core/1/bin/herdr-core",
            "/nix/store/abc/bin/herdr-core",
            "/Users/test/.local/share/mise/installs/herdr-core/1/herdr-core",
            "/Users/test/.local/share/herdr-core/current/Contents/MacOS/herdr-core",
        ] {
            assert!(!bundle_completion_path(Path::new(bin), home), "{bin}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn bundle_completion_skips_unwritable_directories() {
        assert!(bundle_completion_writable(&std::env::temp_dir().join("herdr-core")));
        assert!(!bundle_completion_writable(Path::new("/nonexistent/herdr-core")));
        let root = Command::new("id").arg("-u").output().unwrap().stdout == b"0\n";
        if !root {
            assert!(!bundle_completion_writable(Path::new("/usr/bin/herdr-core")));
        }
    }

    #[cfg(unix)]
    #[test]
    fn bundle_completion_does_not_run_a_legacy_installer() {
        for (installer, expected) in [("echo restarted", ""), ("# HC_BUNDLE_ONLY\necho bundled", "bundled\n")] {
            let out = Command::new("sh")
                .args(["-c", &format!("curl() {{ printf '%s\\n' \"$INSTALLER\"; }}; {BUNDLE_INSTALL_SCRIPT}")])
                .env("INSTALLER", installer)
                .output().unwrap();
            assert!(out.status.success());
            assert_eq!(String::from_utf8(out.stdout).unwrap(), expected);
        }
    }

    // Fakes the binary with a shell script.
    #[cfg(unix)]
    #[test]
    fn only_a_binary_that_says_hc_counts() {
        // Runtime writes can leave a writer fd in a parallel fork, causing ETXTBSY.
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/core-bin");
        assert_eq!(version(&dir.join("hc")), None);
        let real = dir.join("herdr-core");
        assert_eq!(version(&real).as_deref(), Some("hc 0.1.1"));
        assert_eq!(find(Some(real.to_str().unwrap())), Some(real));
    }

    #[test]
    fn core_manifest_urls_follow_the_fork_layout() {
        let base = "https://github.com/aimachy/herdr-core/releases";
        assert_eq!(manifest_url(base, None), "https://github.com/aimachy/herdr-core/releases/latest/download/core-latest.json");
        assert_eq!(manifest_url(base, Some("0.1.22")), "https://github.com/aimachy/herdr-core/releases/download/v0.1.22/manifest.json");
        assert_eq!(manifest_url("https://example.test/releases/", None), "https://example.test/releases/latest/download/core-latest.json");
    }

    #[test]
    fn core_manifest_offers_downloads_and_refuses_shortfalls() {
        let text = r#"{"version":"0.1.22","generation":1,"notes":"x","targets":{"linux-x86_64":{"url":"https://example.test/herdr-core-linux-x86_64.tar.gz","sha256":"ab12"}}}"#;
        let manifest = parse_manifest(text).unwrap();
        assert_eq!(manifest.version, "0.1.22");
        let offered = ("https://example.test/herdr-core-linux-x86_64.tar.gz", "ab12");
        assert_eq!(download_for(&manifest, "linux-x86_64", None), Ok(offered));
        assert_eq!(download_for(&manifest, "linux-x86_64", Some("0.1.20")), Ok(offered));
        // Below the floor the app asked for: refused before any download.
        let err = download_for(&manifest, "linux-x86_64", Some("0.2.0")).unwrap_err();
        assert!(err.contains("0.2.0"), "{err}");
        // No build for this machine.
        assert!(download_for(&manifest, "windows-x86_64", None).unwrap_err().contains("windows-x86_64"));
        // A damaged list is an error, never an empty manifest.
        assert!(parse_manifest("not json").is_err());
        assert!(parse_manifest("{}").is_err());
    }

    #[test]
    fn update_from_args_hand_the_binary_to_hc() {
        assert_eq!(update_from_args(Path::new("/tmp/new/herdr-core")), ["update", "--from", "/tmp/new/herdr-core", "--json"]);
    }

    #[test]
    fn a_download_that_fails_its_checksum_is_an_error() {
        let dir = std::env::temp_dir().join(format!("herdr-core-fetch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("core.bin");
        std::fs::write(&source, b"core bytes").unwrap();
        let url = format!("file://{}", source.display());
        let out = dir.join("out.bin");
        assert!(fetch_checked(&url, &crate::update::sha256(b"core bytes"), &out).is_ok());
        let err = fetch_checked(&url, &"0".repeat(64), &out).unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn an_unpacked_release_yields_its_core() {
        let dir = std::env::temp_dir().join(format!("herdr-core-unpacked-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(unpacked_core(&dir), None);
        let exe = dir.join(format!("herdr-core{}", std::env::consts::EXE_SUFFIX));
        std::fs::write(&exe, "bin").unwrap();
        assert_eq!(unpacked_core(&dir), Some(exe));
        // Without a root binary, the macOS bundle's binary is the core.
        let other = std::env::temp_dir().join(format!("herdr-core-bundled-{}", std::process::id()));
        let bundled = other.join("Herdr Core.app/Contents/MacOS/herdr-core");
        std::fs::create_dir_all(bundled.parent().unwrap()).unwrap();
        std::fs::write(&bundled, "bin").unwrap();
        assert_eq!(unpacked_core(&other), Some(bundled));
        std::fs::remove_dir_all(dir).unwrap();
        std::fs::remove_dir_all(other).unwrap();
    }

    // End to end against a file:// fixture in the fork's layout and a fake
    // installed hc that only records how it was called.
    #[cfg(unix)]
    #[test]
    fn update_from_releases_downloads_checks_and_hands_over() {
        const CHILD: &str = "HERDR_TEST_CORE_FROM_RELEASES";
        if std::env::var_os(CHILD).is_none() {
            use std::os::unix::fs::PermissionsExt;
            let root = std::env::temp_dir().join(format!("herdr-core-from-releases-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            let target = core_target();
            let stage = root.join("stage");
            std::fs::create_dir_all(&stage).unwrap();
            let payload = stage.join("herdr-core");
            std::fs::write(&payload, "#!/bin/sh\necho hc 0.1.23\n").unwrap();
            std::fs::set_permissions(&payload, std::fs::Permissions::from_mode(0o755)).unwrap();
            let archive = root.join(format!("herdr-core-{target}.tar.gz"));
            let tar = Command::new("tar").arg("-czf").arg(&archive).arg("-C").arg(&stage).arg("herdr-core").output().unwrap();
            assert!(tar.status.success(), "{}", String::from_utf8_lossy(&tar.stderr));
            let mut targets = serde_json::Map::new();
            targets.insert(target, serde_json::json!({ "url": format!("file://{}", archive.display()), "sha256": crate::update::sha256(&std::fs::read(&archive).unwrap()) }));
            let manifest = serde_json::json!({ "version": "0.1.23", "generation": crate::hc::GENERATION, "targets": targets });
            let latest = root.join("latest/download");
            std::fs::create_dir_all(&latest).unwrap();
            std::fs::write(latest.join("core-latest.json"), manifest.to_string()).unwrap();
            let installed = root.join("installed/herdr-core");
            std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
            std::fs::write(&installed, "#!/bin/sh\nif [ \"$1\" = --version ]; then echo hc 0.1.21; exit 0; fi\nprintf '%s\\n' \"$*\" >> \"$HERDR_FAKE_RECORD\"\nprintf '%s\\n' '{\"now\":\"hc 0.1.23\"}'\n").unwrap();
            std::fs::set_permissions(&installed, std::fs::Permissions::from_mode(0o755)).unwrap();
            let output = Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "core_bin::tests::update_from_releases_downloads_checks_and_hands_over", "--nocapture"])
                .env(CHILD, "1")
                .env("HERDR_FIXTURE_ROOT", &root)
                .env("HERDR_CORE_RELEASES_URL", format!("file://{}", root.display()))
                .env("HERDR_FAKE_RECORD", root.join("record"))
                .output()
                .unwrap();
            std::fs::remove_dir_all(&root).unwrap();
            assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
            return;
        }
        let root = PathBuf::from(std::env::var("HERDR_FIXTURE_ROOT").unwrap());
        let record = root.join("record");
        use_core(root.join("installed/herdr-core"));
        // The happy path: the app downloads, checks, and hands hc the binary.
        assert_eq!(update_from_releases(None).as_deref(), Ok("hc 0.1.23"));
        let calls = std::fs::read_to_string(&record).unwrap();
        let args: Vec<&str> = calls.trim().split(' ').collect();
        assert_eq!(args[..2], ["update", "--from"]);
        assert!(args[2].ends_with("herdr-core"), "{}", args[2]);
        assert_eq!(args[3], "--json");
        // A damaged download is refused before hc is asked again.
        std::fs::remove_file(&record).unwrap();
        let latest = root.join("latest/download/core-latest.json");
        let mut manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&latest).unwrap()).unwrap();
        manifest["targets"][core_target()]["sha256"] = serde_json::json!("0".repeat(64));
        std::fs::write(&latest, manifest.to_string()).unwrap();
        let err = update_from_releases(None).unwrap_err();
        assert!(err.contains("checksum mismatch"), "{err}");
        assert!(!record.exists(), "hc must not be asked to swap in a damaged download");
        // Below the app's floor: refused, and hc is still not asked.
        let err = update_from_releases(Some("0.2.0")).unwrap_err();
        assert!(err.contains("0.2.0"), "{err}");
        assert!(!record.exists());
        // With nothing waiting, the plain delegation is untouched.
        assert_eq!(update().as_deref(), Ok("hc 0.1.23"));
        assert_eq!(std::fs::read_to_string(&record).unwrap().trim(), "update --json");
    }
}
