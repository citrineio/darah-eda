# Installing Darah EDA

[← README](../README.md) · [User guide](usage.md)

Install a Linux release package or compile the application from source. Download official packages from the [GitHub releases page](https://github.com/citrineio/darah-eda/releases/latest).

## Choose an installation method

| Method | Suitable for | What it needs |
| --- | --- | --- |
| [AppImage](#appimage) | A portable executable with a small installation footprint | A compatible Linux desktop and graphics drivers. The package includes its application libraries and PDF/PNG renderer. |
| [Flatpak](#flatpak) | A sandboxed desktop installation using a shared runtime | Flatpak and the runtime requested during installation. The first installation can download several hundred megabytes of platform and graphics components. |
| [Compile from source](#compile-from-source) | Development, inspection, or a local native build | Current stable Rust, a C build toolchain, and the development packages below. |

Use an asset matching your machine's architecture. Run `uname -m` if unsure: `x86_64` is AMD64, and `aarch64` is ARM64. Only use an architecture actually listed in the release assets.

Commands below use `VERSION` in filenames. Replace it with the version you downloaded; do not type `VERSION` literally. The package examples use `x86_64`.

## AppImage

### Install and launch

1. Download the `.AppImage` asset into your `Downloads` folder.
2. Put it in a permanent, writable location and mark it executable:

   ```bash
   mkdir -p "$HOME/Applications"
   install -m755 "$HOME/Downloads/darah-eda-VERSION-linux-x86_64.AppImage" \
     "$HOME/Applications/Darah-EDA-x86_64.AppImage"
   ```

3. Launch Darah:

   ```bash
   "$HOME/Applications/Darah-EDA-x86_64.AppImage"
   ```

The stable local filename is optional. You can also make the original downloaded file executable with `chmod +x` and run it in place. Keep the file in a folder your user can write to if you want to apply in-app updates.

To open a project directly:

```bash
"$HOME/Applications/Darah-EDA-x86_64.AppImage" /path/to/project.darah.toml
```

### Updates

Use **Help → Check for Updates** in a configured AppImage build. Darah verifies update metadata and the downloaded package before offering installation. The updater retains a previous copy for recovery. Follow its instructions to restart into the new build.

You can also download a newer AppImage manually. Close the running application before replacing your usual copy.

### FUSE or execution problems

If the AppImage reports a FUSE problem, try the extract-and-run option:

```bash
"$HOME/Applications/Darah-EDA-x86_64.AppImage" --appimage-extract-and-run
```

See the [official AppImage FUSE guidance](https://docs.appimage.org/user-guide/troubleshooting/fuse.html) for distribution-specific setup. Darah still uses the host's graphics drivers and graphical session.

## Flatpak

### Set up Flatpak

If Flatpak is already installed, continue with the Flathub command. On Ubuntu or Debian, install it with:

```bash
sudo apt update
sudo apt install flatpak
```

Add the user Flathub remote so Flatpak can obtain the required runtime:

```bash
flatpak remote-add --user --if-not-exists flathub \
  https://dl.flathub.org/repo/flathub.flatpakrepo
```

Darah's `.flatpak` bundle is distributed through GitHub releases. Adding Flathub supplies its runtime dependencies; it does not install Darah from a Flathub application listing.

### Install and launch

Download the `.flatpak` asset into `Downloads`, then install it as your normal user:

```bash
flatpak install --user "$HOME/Downloads/darah-eda-VERSION-linux-x86_64.flatpak"
```

Accept the required runtime downloads. The application bundle does not include the platform runtime or graphics extensions. A fresh VM can therefore need a much larger download than the Darah bundle itself. Runtime components can be reused by other applications that use the same versions. Entries marked **partial** are selectively installed; their displayed size is not necessarily the final transfer size.

Launch Darah with:

```bash
flatpak run io.github.citrineio.DarahEDA
```

Use the application's **Open** and **Save** dialogs to access your project files through the desktop portal. Rust and Flatpak Builder are not required to run a release bundle.

### Updates and removal

For these GitHub release bundles, download a newer bundle and install it with the same `flatpak install --user` command, using the new filename. Flatpak will offer to update the installed application.

`flatpak update --user` updates installed runtimes and applications supplied by their configured remotes. The runtime remote alone does not provide new Darah application releases. Darah's AppImage updater is disabled in Flatpak builds.

To remove Darah:

```bash
flatpak uninstall --user io.github.citrineio.DarahEDA
```

This does not request deletion of application data.

## Compile from source

The following instructions target a Linux desktop on Ubuntu or Debian. Other distributions need equivalent packages. Run Darah from a graphical desktop session as your normal user.

### 1. Install build and runtime dependencies

```bash
sudo apt update
sudo apt install git curl build-essential pkg-config \
  libgl1-mesa-dev libx11-dev libxcb1-dev \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
  librsvg2-bin xdg-desktop-portal xdg-desktop-portal-gtk
```

`librsvg2-bin` provides `rsvg-convert` for PDF and PNG exports in native source builds. A working file chooser portal is needed for Open and Save dialogs. If your desktop already provides a suitable portal backend, use it.

### 2. Install current stable Rust

Follow the [official Rust installation instructions](https://www.rust-lang.org/tools/install). On Linux, the standard rustup installation command is:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
```

If rustup is already installed, update the stable toolchain:

```bash
rustup update stable
```

### 3. Obtain, test, and build Darah

```bash
git clone https://github.com/citrineio/darah-eda.git
cd darah-eda
cargo +stable test --locked
cargo +stable build --release --locked
```

The repository includes `Cargo.lock`. Keep it and use `--locked` to build the recorded dependency set. The first build needs internet access to download dependencies.

### 4. Launch the native build

```bash
./target/release/darah-eda
```

To open an existing design:

```bash
./target/release/darah-eda /path/to/project.darah.toml
```

After pulling source updates, rebuild with the same locked build command. Maintainers preparing AppImage and Flatpak releases should follow [Building and publishing releases](publishing-updates.md).

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| Open or Save dialog does not appear | Run inside a desktop session with a working file chooser portal. On Ubuntu/Debian, `xdg-desktop-portal` with a suitable GTK or desktop-specific backend supplies it. |
| PDF or PNG export fails in a source build | Check `rsvg-convert --version`; install `librsvg2-bin` if missing. Release packages provide their renderer. |
| A downloaded executable will not start | Check the architecture, executable permission, graphical session, and terminal output. For AppImage-specific FUSE errors, use the guidance above. |
| Flatpak requests a large runtime download | This is separate from the Darah bundle. Allow the required runtime installation, or choose the AppImage installation method. |

If installation still fails, [open an issue](https://github.com/citrineio/darah-eda/issues/new/choose) with your OS version, architecture, installation method, exact command, and complete error output.

## Request another package format or platform

Need a different package format, distribution integration, or operating-system build? [Submit an issue or feature request](https://github.com/citrineio/darah-eda/issues/new/choose). Include the platform and architecture, requested format, and preferred installation/update workflow so the request can be evaluated and tested.
