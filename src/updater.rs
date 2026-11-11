use eframe::egui;
use futures_util::{pin_mut, StreamExt};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
    env, fs::{self, File, Metadata, OpenOptions}, io::{Read, Write}, os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf}, process::Command, sync::mpsc::{self, Receiver, TryRecvError},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tempfile::{NamedTempFile, TempDir};
use tough::{Repository, RepositoryLoader, TargetName};
use url::Url;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
const ROOT: &[u8] = include_bytes!("../packaging/update-root.json");
const DAILY: Duration = Duration::from_secs(86_400);
const MAX_MANIFEST: u64 = 64 * 1024;
const MAX_IMAGE: u64 = 512 * 1024 * 1024;

#[derive(Clone)]
struct Config { base: Url, state: PathBuf }

impl Config {
    fn load() -> Option<Self> {
        if cfg!(feature = "flatpak") { return None; }
        let base = Url::parse(option_env!("DARAH_UPDATE_BASE_URL")?).ok()?;
        if base.scheme() != "https" || base.host_str().is_none() || !base.username().is_empty()
            || base.password().is_some() || base.query().is_some() || base.fragment().is_some()
            || !base.path().ends_with('/')
            || serde_json::from_slice::<tough::schema::Signed<tough::schema::Root>>(ROOT).is_err()
        { return None; }
        let state = env::var_os("XDG_STATE_HOME").map(PathBuf::from).filter(|p| p.is_absolute())
            .or_else(|| env::var_os("HOME").map(PathBuf::from).filter(|p| p.is_absolute())
                .map(|home| home.join(".local/state")))?.join("darah-eda/updates");
        Some(Self { base, state })
    }

    fn lock(&self) -> Result<File> {
        fs::create_dir_all(&self.state)?;
        let lock = OpenOptions::new().read(true).write(true).create(true).truncate(false)
            .open(self.state.join("update.lock"))?;
        lock.try_lock().map_err(|e| format!("Could not lock update state. Another Darah window may be checking for updates. {e}"))?;
        Ok(lock)
    }

    async fn repository(&self, root: &[u8]) -> Result<Repository> {
        let cache = self.state.join("tuf");
        fs::create_dir_all(&cache)?;
        Ok(RepositoryLoader::new(&root, self.base.join("tuf/metadata/")?, self.base.join("tuf/targets/")?)
            .datastore(cache).load().await?)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Release { version: String, notes: String }

impl Release {
    fn version(&self) -> Result<Version> {
        let version = Version::parse(&self.version)?;
        if !version.pre.is_empty() || !version.build.is_empty() {
            return Err("The stable feed requires a plain release version.".into());
        }
        Ok(version)
    }

    fn image_name(&self) -> Result<String> {
        let arch = match env::consts::ARCH {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            _ => return Err("This architecture has no AppImage update.".into()),
        };
        Ok(format!("darah-eda-{}-linux-{arch}.AppImage", self.version()?))
    }
}

async fn release(repo: &Repository) -> Result<Release> {
    let name = TargetName::new("stable.json")?;
    let stream = repo.read_target(&name).await?.ok_or("The signed stable release is missing.")?;
    pin_mut!(stream);
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if (bytes.len() as u64).saturating_add(chunk.len() as u64) > MAX_MANIFEST {
            return Err("The release metadata exceeds its size limit.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let release: Release = serde_json::from_slice(&bytes)?;
    release.version()?;
    Ok(release)
}

fn current_version() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).expect("Cargo package version")
}

fn check(config: &Config, runtime: &tokio::runtime::Runtime) -> Result<Option<Release>> {
    let _lock = config.lock()?;
    runtime.block_on(async {
        let found = tokio::time::timeout(Duration::from_secs(20), async {
            let repo = config.repository(ROOT).await?;
            release(&repo).await
        }).await??;
        Ok((found.version()? > current_version()).then_some(found))
    })
}

#[derive(Clone, Copy)]
enum Package { AppImage, Deb, Other }

fn package() -> Package {
    match option_env!("DARAH_PACKAGE_KIND") {
        Some("appimage") => Package::AppImage,
        Some("deb") => Package::Deb,
        _ => Package::Other,
    }
}

fn appimage_path() -> Result<PathBuf> {
    let mount = PathBuf::from(env::var_os("APPDIR").ok_or("Launch Darah from its AppImage to update it.")?).canonicalize()?;
    if !env::current_exe()?.canonicalize()?.starts_with(mount) {
        return Err("Darah is running outside its AppImage mount.".into());
    }
    let path = PathBuf::from(env::var_os("APPIMAGE").ok_or("The AppImage path is missing.")?);
    if !path.is_absolute() { return Err("The AppImage path must be absolute.".into()); }
    let path = path.parent().ok_or("The AppImage has no parent folder.")?.canonicalize()?
        .join(path.file_name().ok_or("The AppImage has no filename.")?);
    let metadata = fs::symlink_metadata(&path)?;
    let user = fs::metadata("/proc/self")?.uid();
    if user == 0 || !metadata.is_file() || metadata.uid() != user {
        return Err("The AppImage must be a regular file owned by your non-root account.".into());
    }
    validate_image(&mut File::open(&path)?)?;
    Ok(path)
}

fn validate_image(file: &mut File) -> Result<()> {
    let mut header = [0; 20];
    file.read_exact(&mut header)?;
    if &header[..6] != b"\x7fELF\x02\x01" || &header[8..11] != b"AI\x02" {
        return Err("Expected a 64-bit type 2 Linux AppImage.".into());
    }
    if u16::from_le_bytes([header[18], header[19]]) != elf_machine()? {
        return Err("The AppImage is for a different processor architecture.".into());
    }
    Ok(())
}

fn elf_machine() -> Result<u16> {
    match env::consts::ARCH {
        "x86_64" => Ok(62),
        "aarch64" => Ok(183),
        _ => Err("This architecture has no AppImage update.".into()),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ImageIdentity { dev: u64, inode: u64, bytes: u64, modified: (i64, i64) }

impl From<&Metadata> for ImageIdentity {
    fn from(meta: &Metadata) -> Self {
        Self { dev: meta.dev(), inode: meta.ino(), bytes: meta.len(), modified: (meta.mtime(), meta.mtime_nsec()) }
    }
}

struct Staged {
    destination: PathBuf,
    original: ImageIdentity,
    image: NamedTempFile,
}

fn stage(config: &Config, runtime: &tokio::runtime::Runtime, selected: Release, progress: impl FnMut(Progress)) -> Result<Staged> {
    let destination = appimage_path()?;
    let original = ImageIdentity::from(&fs::symlink_metadata(&destination)?);
    let image = tempfile::Builder::new().prefix(".darah-update-")
        .tempfile_in(destination.parent().ok_or("Missing AppImage directory.")?)?;
    let _lock = config.lock()?;
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(900), async {
            let repo = config.repository(ROOT).await?;
            if release(&repo).await? != selected || selected.version()? <= current_version() {
                return Err("The stable release changed. Check for updates again.".into());
            }
            let image = download_image(&repo, &selected, image, progress).await?;
            Ok(Staged { destination, original, image })
        }).await?
    })
}

async fn download_image(repo: &Repository, selected: &Release, mut image: NamedTempFile, mut progress: impl FnMut(Progress)) -> Result<NamedTempFile> {
    let name = TargetName::new(selected.image_name()?)?;
    let (_, metadata) = repo.all_targets().find(|(key, _)| *key == &name)
        .ok_or("No signed AppImage is available for this architecture.")?;
    if metadata.length > MAX_IMAGE { return Err("The AppImage exceeds the update size limit.".into()); }
    progress(Progress::Downloading { bytes: 0, total: metadata.length });
    let stream = repo.read_target(&name).await?.ok_or("The signed AppImage is missing.")?;
    pin_mut!(stream);
    let mut bytes = 0u64;
    let mut last_update = Instant::now();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        bytes = bytes.saturating_add(chunk.len() as u64);
        if bytes > metadata.length { return Err("The AppImage exceeds its signed size.".into()); }
        image.write_all(&chunk)?;
        if bytes == metadata.length {
            progress(Progress::Verifying);
        } else if last_update.elapsed() >= Duration::from_millis(80) {
            progress(Progress::Downloading { bytes, total: metadata.length });
            last_update = Instant::now();
        }
    }
    if bytes != metadata.length { return Err("The AppImage does not match its signed size.".into()); }
    validate_image(&mut image.reopen()?)?;
    image.as_file().sync_all()?;
    Ok(image)
}

fn stable_image_name() -> String {
    format!("Darah-EDA-{}.AppImage", env::consts::ARCH)
}

fn rename_image(path: &Path, destination: &Path) -> std::io::Result<()> {
    // A hard link creates the new name without ever replacing an existing file.
    fs::hard_link(path, destination)?;
    if let Err(error) = fs::remove_file(path) {
        let _ = fs::remove_file(destination);
        return Err(error);
    }
    Ok(())
}

impl Staged {
    fn install(self, rename: bool) -> Result<(PathBuf, Option<String>)> {
        let directory = self.destination.parent().ok_or("Missing AppImage directory.")?;
        let current = File::open(&self.destination)?;
        current.try_lock().map_err(|e| format!("Another updater is using this AppImage. {e}"))?;
        let meta = fs::symlink_metadata(&self.destination)?;
        if !meta.is_file() || ImageIdentity::from(&meta) != self.original
            || ImageIdentity::from(&current.metadata()?) != self.original
        {
            return Err("The installed AppImage changed. Check for updates again.".into());
        }
        self.image.as_file().set_permissions(fs::Permissions::from_mode(meta.mode() & 0o777))?;
        self.image.as_file().sync_all()?;
        let mut name = self.destination.file_name().ok_or("Missing AppImage name.")?.to_os_string();
        name.push(".previous");
        let backup = directory.join(name);
        if let Ok(meta) = fs::symlink_metadata(&backup) {
            if !meta.is_file() || meta.uid() != meta_uid(&current)? {
                return Err("The previous-version backup path is already in use.".into());
            }
            validate_image(&mut File::open(&backup)?)?;
        }
        let staging = TempDir::with_prefix_in(".darah-backup-", directory)?;
        let previous = staging.path().join("previous");
        fs::hard_link(&self.destination, &previous)?;
        fs::rename(previous, backup)?;
        let dir = File::open(directory)?;
        dir.sync_all()?;
        self.image.as_file().try_lock()?;
        let _installed = self.image.persist(&self.destination)?;
        let stable = self.destination.with_file_name(stable_image_name());
        let result = if rename && self.destination != stable {
            match rename_image(&self.destination, &stable) {
                Ok(()) => (stable, None),
                Err(error) => (self.destination, Some(format!("The filename could not be changed: {error}"))),
            }
        } else {
            (self.destination, None)
        };
        if let Err(error) = dir.sync_all() { eprintln!("Darah installed the update but could not sync the directory: {error}"); }
        Ok(result)
    }
}

fn meta_uid(file: &File) -> std::io::Result<u32> { Ok(file.metadata()?.uid()) }

#[derive(Deserialize, Serialize)]
#[serde(default)]
struct Preferences { automatic: bool, remind_after: u64, skipped: Option<String> }

impl Default for Preferences {
    fn default() -> Self { Self { automatic: true, remind_after: 0, skipped: None } }
}

fn now() -> u64 { SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Progress {
    Downloading { bytes: u64, total: u64 },
    Verifying,
}

enum Event {
    Checked(Option<Release>),
    Progress(Progress),
    Staged(Staged),
    Installed(PathBuf, Option<String>),
    Error(String),
}

pub(crate) struct Updater {
    config: Option<Config>,
    prefs: Preferences,
    open: bool,
    last_check: Option<Instant>,
    available: Option<Release>,
    staged: Option<Staged>,
    work: Option<Receiver<Event>>,
    message: String,
    progress: Option<Progress>,
    rename: bool,
    installing: bool,
    installed: Option<PathBuf>,
    restart: Option<(PathBuf, Option<PathBuf>)>,
}

impl Updater {
    pub(crate) fn new() -> Self {
        let config = Config::load();
        let prefs = config.as_ref().and_then(|c| fs::read(c.state.join("preferences.json")).ok())
            .and_then(|data| serde_json::from_slice(&data).ok()).unwrap_or_default();
        Self { config, prefs, open: false, last_check: None, available: None, staged: None, work: None,
            message: String::new(), progress: None, rename: false, installing: false, installed: None, restart: None }
    }

    fn save_preferences(&mut self) {
        let Some(config) = &self.config else { return; };
        let result = (|| -> Result<()> {
            fs::create_dir_all(&config.state)?;
            let mut file = NamedTempFile::new_in(&config.state)?;
            serde_json::to_writer(&mut file, &self.prefs)?;
            file.as_file().sync_all()?;
            file.persist(config.state.join("preferences.json"))?;
            Ok(())
        })();
        if let Err(error) = result { self.message = format!("Could not save update settings: {error}"); }
    }

    fn start(&mut self, ctx: &egui::Context, download: Option<Release>) {
        let Some(config) = self.config.clone() else { return; };
        if self.work.is_some() || self.staged.is_some() || self.installed.is_some() { return; }
        self.last_check = Some(Instant::now());
        self.message = if download.is_some() { "Preparing download…" } else { "Checking for updates…" }.into();
        let (send, receive) = mpsc::channel();
        self.work = Some(receive);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = (|| -> Result<Event> {
                let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
                match download {
                    Some(selected) => Ok(Event::Staged(stage(&config, &runtime, selected, |progress| {
                        let _ = send.send(Event::Progress(progress));
                        ctx.request_repaint();
                    })?)),
                    None => Ok(Event::Checked(check(&config, &runtime)?)),
                }
            })();
            let _ = send.send(result.unwrap_or_else(|error| Event::Error(error.to_string())));
            ctx.request_repaint();
        });
    }

    pub(crate) fn tick(&mut self, ctx: &egui::Context) {
        while let Some(work) = &self.work {
            let event = match work.try_recv() {
                Ok(Event::Progress(progress)) => {
                    self.message.clear();
                    self.progress = Some(progress);
                    continue;
                }
                Ok(event) => event,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => Event::Error("The update worker stopped.".into()),
            };
            self.work = None;
            self.progress = None;
            self.installing = false;
            match event {
                Event::Checked(release) => {
                    self.message = if release.is_some() { "" } else { "Darah is up to date." }.into();
                    self.available = release;
                }
                Event::Staged(staged) => {
                    self.staged = Some(staged);
                    self.message = "AppImage verified and ready to install.".into();
                }
                Event::Installed(path, warning) => {
                    self.installed = Some(path);
                    self.message = warning.unwrap_or_default();
                }
                Event::Error(error) => self.message = format!("Update failed. {error}"),
                Event::Progress(_) => unreachable!(),
            }
        }
        if self.config.is_some() && self.prefs.automatic && self.staged.is_none() && self.work.is_none() && self.installed.is_none() {
            let elapsed = self.last_check.map_or(DAILY, |time| time.elapsed());
            if elapsed >= DAILY { self.start(ctx, None); }
            ctx.request_repaint_after(DAILY.saturating_sub(elapsed).max(Duration::from_secs(1)));
        }
    }

    pub(crate) fn open(&mut self, ctx: &egui::Context, check_now: bool) {
        self.open = true;
        if check_now || self.available.is_none() { self.start(ctx, None); }
    }

    pub(crate) fn badge(&self) -> Option<&str> {
        let available = self.available.as_ref()?;
        (self.installed.is_some() || self.staged.is_some() || (self.prefs.remind_after <= now()
            && self.prefs.skipped.as_deref() != Some(available.version.as_str()))).then_some(available.version.as_str())
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context) -> bool {
        if !self.open { return false; }
        let mut open = true;
        let mut restart = false;
        egui::Window::new("Updates").open(&mut open).default_width(450.0).show(ctx, |ui| {
            ui.label(format!("Running version {}", env!("CARGO_PKG_VERSION")));
            if let Some(path) = &self.installed {
                let version = self.available.as_ref().map(|release| release.version.as_str()).unwrap_or("the update");
                ui.label(format!("Installed v{version} at {}", path.display()));
                if !self.message.is_empty() { ui.label(&self.message); }
                if ui.button(format!("Restart to use v{version}")).clicked() { restart = true; }
                return;
            }
            if self.installing {
                ui.horizontal(|ui| { ui.spinner(); ui.label("Installing the verified AppImage…"); });
                return;
            }
            let Some(base) = self.config.as_ref().map(|config| config.base.clone()) else {
                ui.label("Updates are unconfigured in this build.");
                return;
            };
            if ui.checkbox(&mut self.prefs.automatic, "Check automatically").changed() { self.save_preferences(); }
            if ui.add_enabled(self.work.is_none() && self.staged.is_none(), egui::Button::new("Check now")).clicked() {
                self.start(ctx, None);
            }
            if !self.message.is_empty() { ui.label(&self.message); }
            match self.progress {
                Some(Progress::Downloading { bytes, total }) => {
                    ui.label(format!("Downloading AppImage · {bytes} / {total} bytes"));
                    ui.add(egui::ProgressBar::new(bytes as f32 / total.max(1) as f32).show_percentage());
                }
                Some(Progress::Verifying) => {
                    ui.horizontal(|ui| { ui.spinner(); ui.label("Verifying the AppImage…"); });
                }
                None => {}
            }
            if let Some(available) = self.available.clone() {
                ui.separator();
                ui.heading(format!("Darah {} is available", available.version));
                egui::ScrollArea::vertical().max_height(190.0).show(ui, |ui| { ui.label(&available.notes); });
                if let Some(staged) = &self.staged {
                    let name = stable_image_name();
                    if staged.destination.file_name().is_some_and(|current| current != name.as_str()) {
                        ui.checkbox(&mut self.rename, format!("Rename to {name}"))
                            .on_hover_text("Existing shortcuts may need to be updated if you rename the file.");
                    }
                    if ui.button("Install update").clicked() { self.install(ctx); }
                    if ui.button("Discard download").clicked() {
                        self.staged = None;
                        self.rename = false;
                        self.message.clear();
                    }
                } else {
                    match package() {
                        Package::AppImage => {
                            if ui.add_enabled(self.work.is_none(), egui::Button::new("Download and verify")).clicked() {
                                self.start(ctx, Some(available.clone()));
                            }
                        }
                        Package::Deb => {
                            ui.label("Use your system package manager to upgrade Darah.");
                            if let Ok(apt) = base.join("apt/") { ui.hyperlink_to("APT setup and upgrade instructions", apt.as_str()); }
                        }
                        Package::Other => { ui.label("Use the installation method for this build."); }
                    }
                    ui.hyperlink_to("Download page", base.as_str());
                    ui.horizontal(|ui| {
                        if ui.button("Remind me tomorrow").clicked() {
                            self.prefs.remind_after = now().saturating_add(DAILY.as_secs());
                            self.save_preferences();
                        }
                        if ui.button("Skip this version").clicked() {
                            self.prefs.skipped = Some(available.version);
                            self.save_preferences();
                        }
                    });
                }
            }
        });
        self.open = open;
        restart
    }

    fn install(&mut self, ctx: &egui::Context) {
        let Some(staged) = self.staged.take() else { return; };
        let rename = std::mem::take(&mut self.rename);
        self.installing = true;
        let (send, receive) = mpsc::channel();
        self.work = Some(receive);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let event = match staged.install(rename) {
                Ok((path, warning)) => Event::Installed(path, warning),
                Err(error) => Event::Error(error.to_string()),
            };
            let _ = send.send(event);
            ctx.request_repaint();
        });
    }

    pub(crate) fn prepare_restart(&mut self, project: Option<PathBuf>) -> bool {
        let Some(path) = &self.installed else { return false; };
        self.restart = Some((path.clone(), project));
        true
    }

    pub(crate) fn installing(&self) -> bool { self.installing }

    pub(crate) fn restart_on_exit(&mut self) {
        let Some((destination, project)) = self.restart.take() else { return; };
        let mut command = Command::new(destination);
        command.args(project).env_remove("APPIMAGE").env_remove("APPDIR").env_remove("ARGV0")
            .env_remove("LD_LIBRARY_PATH");
        if let Some(home) = env::var_os("HOME") { command.current_dir(home); }
        if let Err(error) = command.spawn() { eprintln!("Darah was updated but could not restart: {error}"); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(label: &[u8]) -> Vec<u8> {
        let mut header = [0; 20];
        header[..6].copy_from_slice(b"\x7fELF\x02\x01");
        header[8..11].copy_from_slice(b"AI\x02");
        header[18..20].copy_from_slice(&elf_machine().unwrap().to_le_bytes());
        [header.as_slice(), label].concat()
    }

    fn staged(destination: PathBuf, original: ImageIdentity, bytes: &[u8]) -> Staged {
        let mut image = NamedTempFile::new_in(destination.parent().unwrap()).unwrap();
        image.write_all(bytes).unwrap();
        image.as_file().sync_all().unwrap();
        Staged { destination, original, image }
    }

    #[test]
    fn replacement_keeps_original_and_refuses_a_stale_concurrent_install() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let path = folder.path().join("Darah.AppImage");
        let old = image(b"old");
        let new = image(b"new");
        fs::write(&path, &old)?;
        let stage = |bytes| staged(path.clone(), ImageIdentity::from(&fs::metadata(&path).unwrap()), bytes);
        let first = stage(&new);
        let stale = stage(&old);
        assert_eq!(first.install(false)?.0, path);
        assert!(stale.install(false).is_err());
        assert_eq!(fs::read(&path)?, new);
        assert_eq!(fs::read(folder.path().join("Darah.AppImage.previous"))?, old);
        Ok(())
    }

    #[test]
    fn optional_rename_preserves_backup_and_never_overwrites_another_appimage() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let path = folder.path().join("Darah-EDA-0.7.0.AppImage");
        let stable = folder.path().join(stable_image_name());
        let old = image(b"old");
        let new = image(b"new");
        fs::write(&path, &old)?;
        fs::write(&stable, b"occupied")?;
        let stage = |bytes| staged(path.clone(), ImageIdentity::from(&fs::metadata(&path).unwrap()), bytes);
        let (actual, warning) = stage(&new).install(true)?;
        assert_eq!(actual, path);
        assert!(warning.is_some());
        assert_eq!(fs::read(&stable)?, b"occupied");
        assert_eq!(fs::read(&path)?, new);
        fs::remove_file(&stable)?;
        let (actual, warning) = stage(&old).install(true)?;
        assert_eq!(actual, stable);
        assert!(warning.is_none());
        assert!(!path.exists());
        assert_eq!(fs::read(stable)?, old);
        assert_eq!(fs::read(folder.path().join("Darah-EDA-0.7.0.AppImage.previous"))?, new);
        Ok(())
    }

    #[cfg(feature = "flatpak")]
    #[test]
    fn flatpak_uses_only_its_package_manager_for_updates() {
        assert!(Config::load().is_none());
    }

    #[test]
    #[ignore = "requires a published local test feed with amd64 or arm64 fixtures"]
    fn verified_feed_accepts_image_and_rejects_tampering() -> Result<()> {
        let input = PathBuf::from(env::var_os("DARAH_TEST_UPDATE_SITE").ok_or("Set DARAH_TEST_UPDATE_SITE")?);
        let root = fs::read(input.join("tuf/metadata/1.root.json"))?;
        let folder = tempfile::tempdir()?;
        let origin = Url::from_directory_path(&input).map_err(|_| "Use an absolute site path")?;
        let config = Config { base: origin, state: folder.path().join("state") };
        let _lock = config.lock()?;
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        runtime.block_on(async {
            let repo = config.repository(&root).await?;
            let selected = release(&repo).await?;
            let mut progress = Vec::new();
            let good = download_image(&repo, &selected, NamedTempFile::new_in(folder.path())?, |event| {
                progress.push(event);
            }).await?;
            validate_image(&mut good.reopen()?)?;
            let total = good.as_file().metadata()?.len();
            assert_eq!(progress.first(), Some(&Progress::Downloading { bytes: 0, total }));
            assert_eq!(progress.last(), Some(&Progress::Verifying));
            let filename = selected.image_name()?;
            let copy = folder.path().join("tampered/tuf");
            fs::create_dir_all(copy.join("targets"))?;
            let data = fs::read(good.path())?;
            let mut corrupt = data.clone();
            *corrupt.last_mut().unwrap() ^= 1;
            use sha2::{Digest, Sha256};
            let encoded = format!("{:x}", Sha256::digest(&data));
            fs::write(copy.join("targets").join(format!("{encoded}.{filename}")), corrupt)?;
            // Use the signed metadata unchanged and only change the target bytes.
            let metadata_base = config.base.join("tuf/metadata/")?;
            let target_base = Url::from_directory_path(copy.join("targets")).unwrap();
            let repo = RepositoryLoader::new(&root, metadata_base, target_base).load().await?;
            assert!(download_image(&repo, &selected, NamedTempFile::new_in(folder.path())?, |_| {}).await.is_err());
            if let Some(previous) = env::var_os("DARAH_TEST_PREVIOUS_SITE") {
                let base = Url::from_directory_path(PathBuf::from(previous)).map_err(|_| "Use an absolute previous site path")?;
                let previous = Config { base, state: config.state };
                assert!(previous.repository(&root).await.is_err(), "accepted older signed metadata");
            }
            Ok(())
        })
    }
}
