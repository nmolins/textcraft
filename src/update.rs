//! Updates from the project's GitHub releases.
//!
//! * Asks GitHub for the latest release (at start-up, and from Aide > Rechercher des mises à jour).
//! * If it is newer, offers it; nothing is installed without a click.
//! * Downloads the file for this system, checks its SHA-256 against the one GitHub
//!   publishes, swaps it in for the running program and restarts it. Tabs come back
//!   through the usual session restore.
//!
//! Downloads use `curl`, which ships with Windows 10/11 and Linux, so no extra
//! network or TLS library is built into the app.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use eframe::egui::{self, RichText};
use sha2::{Digest, Sha256};

use crate::app::App;

pub const REPO: &str = "nmolins/textcraft";

pub fn current_version() -> String {
    // Test aid: TEXTCRAFT_TEST_PRETEND_VERSION makes the app believe it is that version.
    std::env::var("TEXTCRAFT_TEST_PRETEND_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

/// "v1.2.3" or "1.2.3-beta" → (1, 2, 3)
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let core = s.trim().trim_start_matches(['v', 'V']).split(['-', '+']).next()?;
    let mut it = core.split('.').map(|p| p.parse::<u64>().ok());
    Some((it.next()??, it.next().unwrap_or(Some(0))?, it.next().unwrap_or(Some(0))?))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(a), Some(b)) => a > b,
        _ => false,
    }
}

/// The download meant for this system.
fn asset_name() -> Option<&'static str> {
    if cfg!(windows) {
        Some("TextCraft.exe")
    } else if cfg!(target_os = "linux") {
        Some("TextCraft-linux.tar.gz")
    } else {
        None
    }
}

#[derive(Clone, Debug)]
pub struct Release {
    pub version: String,
    pub page: String,
    pub notes: String,
    pub url: String,
    pub sha256: String,
}

#[derive(Clone, Debug)]
pub enum State {
    Idle,
    Checking { manual: bool },
    UpToDate { manual: bool },
    Available(Release),
    Downloading(Release),
    Installed(PathBuf),
    Failed(String),
}

#[derive(Clone)]
pub struct Updater {
    state: Arc<Mutex<State>>,
}

impl Default for Updater {
    fn default() -> Self {
        Updater { state: Arc::new(Mutex::new(State::Idle)) }
    }
}

fn curl() -> Command {
    // On Windows use the system's own curl, by full path.
    let exe = std::env::var_os("SystemRoot")
        .map(|r| PathBuf::from(r).join("System32").join("curl.exe"))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("curl"));
    let mut c = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // no console window
    }
    c.args(["-fsSL", "--proto", "=https", "--tlsv1.2", "-H", "User-Agent: TextCraft-updater"]);
    c
}

fn get_text(url: &str) -> Result<String, String> {
    let out = curl()
        .args(["--max-time", "20", "-H", "Accept: application/vnd.github+json", url])
        .output()
        .map_err(|_| "curl est introuvable sur cet ordinateur".to_string())?;
    if !out.status.success() {
        return Err("pas de réponse de GitHub (connexion internet ?)".into());
    }
    String::from_utf8(out.stdout).map_err(|_| "réponse illisible".into())
}

fn latest_release() -> Result<Release, String> {
    let name = asset_name().ok_or("pas de version prête pour ce système")?;
    let json = get_text(&format!("https://api.github.com/repos/{REPO}/releases/latest"))?;
    let v: serde_json::Value = serde_json::from_str(&json).map_err(|_| "réponse de GitHub illisible")?;
    let tag = v["tag_name"].as_str().ok_or("aucune version publiée")?.to_string();
    let assets = v["assets"].as_array().cloned().unwrap_or_default();
    let asset = assets.iter().find(|a| a["name"].as_str() == Some(name)).ok_or_else(|| format!("la version {tag} n'a pas de fichier {name}"))?;
    let url = asset["browser_download_url"].as_str().ok_or("lien de téléchargement manquant")?.to_string();
    // The checksum GitHub computes for each file, or else the SHA256SUMS.txt file of the release.
    let mut sha256 = asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).unwrap_or("").to_lowercase();
    if sha256.len() != 64 {
        if let Some(sums) = assets.iter().find(|a| a["name"].as_str() == Some("SHA256SUMS.txt")) {
            if let Some(u) = sums["browser_download_url"].as_str() {
                let text = get_text(u)?;
                sha256 = text
                    .lines()
                    .find_map(|l| {
                        let mut parts = l.split_whitespace();
                        let h = parts.next()?;
                        let n = parts.next()?.trim_start_matches('*');
                        (n == name).then(|| h.to_lowercase())
                    })
                    .unwrap_or_default();
            }
        }
    }
    if sha256.len() != 64 {
        return Err("la version publiée n'a pas d'empreinte de contrôle : installation refusée".into());
    }
    Ok(Release {
        version: tag.trim_start_matches(['v', 'V']).to_string(),
        page: v["html_url"].as_str().unwrap_or("").to_string(),
        notes: v["body"].as_str().unwrap_or("").trim().to_string(),
        url,
        sha256,
    })
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h)?;
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Downloads, checks and swaps in the new program. Returns the path to start.
fn install(rel: &Release) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("programme introuvable : {e}"))?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    let dir = exe.parent().ok_or("dossier du programme introuvable")?.to_path_buf();
    let part = dir.join(".textcraft-update.part");
    let _ = std::fs::remove_file(&part);
    std::fs::write(&part, b"").map_err(|e| format!("impossible d'écrire dans le dossier de TextCraft ({}) : {e}", dir.display()))?;

    let ok = curl()
        .args(["--max-time", "900", "-o"])
        .arg(&part)
        .arg(&rel.url)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        let _ = std::fs::remove_file(&part);
        return Err("le téléchargement n'a pas abouti".into());
    }
    let got = sha256_file(&part).map_err(|e| format!("fichier téléchargé illisible : {e}"))?;
    if got != rel.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err("le fichier téléchargé ne correspond pas à l'empreinte publiée : rien n'a été installé".into());
    }

    if cfg!(windows) {
        // A running .exe can't be overwritten, but it can be renamed out of the way.
        let old = old_exe_path(&exe);
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&exe, &old).map_err(|e| format!("impossible de remplacer TextCraft : {e}"))?;
        if let Err(e) = std::fs::rename(&part, &exe) {
            let _ = std::fs::rename(&old, &exe);
            return Err(format!("impossible de remplacer TextCraft : {e}"));
        }
    } else {
        let unpack = dir.join(".textcraft-update");
        let _ = std::fs::remove_dir_all(&unpack);
        std::fs::create_dir_all(&unpack).map_err(|e| e.to_string())?;
        let ok = Command::new("tar").arg("xzf").arg(&part).arg("-C").arg(&unpack).status().map(|s| s.success()).unwrap_or(false);
        let _ = std::fs::remove_file(&part);
        let new_bin = unpack.join("TextCraft-linux").join("textcraft");
        if !ok || !new_bin.exists() {
            let _ = std::fs::remove_dir_all(&unpack);
            return Err("archive de mise à jour illisible".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&new_bin, std::fs::Permissions::from_mode(0o755));
        }
        // Renaming over the running program is safe on Linux.
        let r = std::fs::rename(&new_bin, &exe);
        let _ = std::fs::remove_dir_all(&unpack);
        r.map_err(|e| format!("impossible de remplacer TextCraft : {e}"))?;
    }
    Ok(exe)
}

fn old_exe_path(exe: &Path) -> PathBuf {
    let stem = exe.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "TextCraft".into());
    exe.with_file_name(format!("{stem}.old.exe"))
}

/// Removes what a previous update left behind (the old .exe on Windows).
pub fn clean_leftovers() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::fs::remove_file(old_exe_path(&exe));
        if let Some(dir) = exe.parent() {
            let _ = std::fs::remove_file(dir.join(".textcraft-update.part"));
        }
    }
}

impl Updater {
    pub fn state(&self) -> State {
        self.state.lock().map(|s| s.clone()).unwrap_or(State::Idle)
    }

    fn set(&self, s: State) {
        if let Ok(mut g) = self.state.lock() {
            *g = s;
        }
    }

    pub fn reset(&self) {
        self.set(State::Idle);
    }

    pub fn check(&self, ctx: &egui::Context, manual: bool) {
        if matches!(self.state(), State::Checking { .. } | State::Downloading(_)) {
            return;
        }
        self.set(State::Checking { manual });
        let me = self.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let next = match latest_release() {
                Ok(r) if is_newer(&r.version, &current_version()) => State::Available(r),
                Ok(_) => State::UpToDate { manual },
                Err(e) if manual => State::Failed(format!("Impossible de vérifier les mises à jour : {e}")),
                Err(_) => State::Idle, // automatic check: stay quiet
            };
            me.set(next);
            ctx.request_repaint();
        });
    }

    pub fn install(&self, ctx: &egui::Context, rel: Release) {
        self.set(State::Downloading(rel.clone()));
        let me = self.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let next = match install(&rel) {
                Ok(path) => State::Installed(path),
                Err(e) => State::Failed(format!("Mise à jour impossible : {e}")),
            };
            me.set(next);
            ctx.request_repaint();
        });
    }
}

impl App {
    /// Thin strip under the menus when there is something to say about updates.
    pub fn update_banner(&mut self, ui: &mut egui::Ui) {
        let p = *self.pal();
        let state = self.updater.state();
        let ctx = ui.ctx().clone();
        match &state {
            State::UpToDate { manual: true } => {
                self.flash(&ctx, format!("TextCraft est à jour (version {})", current_version()));
                self.updater.reset();
                return;
            }
            State::UpToDate { manual: false } => {
                self.updater.reset();
                return;
            }
            State::Available(r) if self.update_dismissed.as_deref() == Some(r.version.as_str()) => return,
            State::Idle | State::Checking { .. } => return,
            _ => {}
        }
        let mut action: Option<u8> = None; // 1 install, 2 later, 3 notes, 4 close
        egui::Frame::NONE.fill(p.selection).inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| match &state {
                State::Available(r) => {
                    ui.label(RichText::new(format!("TextCraft {} est disponible", r.version)).strong().color(p.ink));
                    ui.label(RichText::new(format!("(vous avez la {})", current_version())).color(p.faint));
                    ui.add_space(8.0);
                    if ui.button("Mettre à jour").clicked() {
                        action = Some(1);
                    }
                    if !r.page.is_empty() && ui.button("Voir les nouveautés").on_hover_text(&r.notes).clicked() {
                        action = Some(3);
                    }
                    if ui.button("Plus tard").clicked() {
                        action = Some(2);
                    }
                }
                State::Downloading(r) => {
                    ui.spinner();
                    ui.label(RichText::new(format!("Téléchargement de TextCraft {}…", r.version)).color(p.ink));
                }
                State::Installed(_) => {
                    ui.spinner();
                    ui.label(RichText::new("Mise à jour installée, redémarrage…").color(p.ink));
                }
                State::Failed(msg) => {
                    ui.label(RichText::new(msg).color(p.accent));
                    if ui.small_button("×").clicked() {
                        action = Some(4);
                    }
                }
                _ => {}
            });
        });
        match (action, state) {
            (Some(1), State::Available(r)) => {
                if cfg!(debug_assertions) {
                    self.updater.set(State::Failed(
                        "Version de développement : téléchargez la nouvelle version depuis la page GitHub".into(),
                    ));
                } else {
                    self.save_session();
                    self.updater.install(&ctx, r);
                }
            }
            (Some(2), State::Available(r)) => self.update_dismissed = Some(r.version),
            (Some(3), State::Available(r)) => ctx.open_url(egui::OpenUrl::new_tab(r.page)),
            (Some(4), _) => self.updater.reset(),
            _ => {}
        }
    }

    /// Called every frame: starts the automatic check, restarts after an install.
    pub fn update_tick(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if !self.update_started && self.update_auto && self.shot_path.is_none() && !cfg!(debug_assertions) && now > 3.0 {
            self.update_started = true;
            self.updater.check(ctx, false);
        } else if !self.update_started && now > 3.0 {
            self.update_started = true;
        } else if !self.update_started {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        if let State::Installed(path) = self.updater.state() {
            if self.update_restarting {
                return; // already started; this window is closing
            }
            // Everything is saved; start the new version (once) and close this one.
            self.save_session();
            if Command::new(&path).spawn().is_ok() {
                self.update_restarting = true;
                self.allow_quit = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else {
                self.updater.set(State::Failed("La nouvelle version est installée : relancez TextCraft.".into()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_like_numbers() {
        assert!(is_newer("v1.0.10", "1.0.9"));
        assert!(is_newer("1.1.0", "1.0.99"));
        assert!(is_newer("v2.0.0", "1.9.9"));
        assert!(!is_newer("v1.0.1", "1.0.1"));
        assert!(!is_newer("v1.0.0", "1.0.1"));
        assert!(!is_newer("pas-une-version", "1.0.0"));
        assert_eq!(parse_version("v1.2"), Some((1, 2, 0)));
        assert_eq!(parse_version("1.2.3-beta"), Some((1, 2, 3)));
    }

    #[test]
    fn file_checksum_is_sha256() {
        let p = std::env::temp_dir().join("textcraft-sha-test.txt");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(sha256_file(&p).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = std::fs::remove_file(&p);
    }
}
