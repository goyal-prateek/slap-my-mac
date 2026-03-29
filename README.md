# Slap My Mac

**Slap My Mac** is a small macOS menu bar app for **Apple Silicon** laptops. It watches the machine’s built-in motion sensor; when it feels a sharp jolt—like a tap on the palm rest beside the trackpad—it plays a slap-style reaction sound and bumps a counter.

The idea is a light, physical “poke your Mac” moment without digging into Terminal or shortcuts.

## What it does

- **Detects taps / slaps** via the internal IMU (accelerometer), with adjustable **sensitivity** and **cooldown** so you can tune how hard a hit must be and how often sounds can fire.
- **Plays audio** on each registered hit: a built-in clip, or a **custom sound** you pick (common formats are supported; long files are trimmed to a configurable maximum length).
- **Temporarily sets macOS output volume** while a slap plays, then restores your previous level—handy if you want slaps loud without leaving the system cranked all day.
- **Counts slaps**, with a **Test slap** action so you can hear the sound and increment the counter even when the sensor is unavailable.
- **Runs from the menu bar** (hand-on-Mac icon): open **Settings**, **Test slap**, **Reset counter**, or **Quit**. Closing the settings window hides it; the app keeps running until you quit from the menu.
- Optional **Open at login** so it starts with your user session after sign-in.

## Requirements and limitations

- **macOS on Apple Silicon** is the target environment. The sensor path is built around the laptop’s internal hardware; **desktop Macs or unsupported machines** may show the sensor as disconnected—you can still use **Test slap** and adjust sounds and volume behavior.
- If the sensor stays disconnected, causes can include **permissions**, hardware differences, or drivers not being ready yet; the app tries to wake the relevant subsystem on startup—waiting a few seconds and retrying sometimes helps.

## Under the hood

Slap My Mac is a **[Tauri](https://tauri.app/)** desktop app: a **React + TypeScript** UI (Vite) with a **Rust** backend handling HID sensor reads, audio playback, tray integration, and settings persistence.

## Build and install locally

This project does not assume you already have Node.js, Rust, or related tools. Use **Quick path** to automate prerequisites and build, or **Manual setup** to do the same steps by hand.

### Quick path: automated setup

From the repository root after cloning:

```bash
./setup.sh
```

The script runs on **macOS** only. It **skips** any step that is already satisfied (Xcode CLT, Node, matching pnpm, Rust/cargo). It installs missing pieces when it can:

- **Node.js** — via [Homebrew](https://brew.sh/) if `brew` is available; otherwise it tells you to install Node manually.
- **pnpm** — via **Corepack** at the version pinned in `package.json` (`pnpm@9.4.0`).
- **Rust** — via [rustup](https://rustup.rs/) (`-y`, stable default) if `cargo` is missing.

Then it runs `pnpm install` and **`pnpm tauri build`**. To only prepare the toolchain and install JS dependencies (no release build yet):

```bash
./setup.sh --no-build
```

If Xcode Command Line Tools are missing, the script starts `xcode-select --install`; finish that installer, then run `./setup.sh` again.

With a full run (no `--no-build`), after `pnpm tauri build` succeeds the script **deletes** `src-tauri/target` (including the DMG under `target/release/bundle/dmg/`) and runs **`open -a "Slap My Mac"`**, which starts the app from **Applications** if it is already installed there. Use **`./setup.sh --no-build`** if you only want dependencies installed; then follow **Manual setup** to build, install from the DMG, and clean up `target/` yourself.

### Manual setup

Do this if you prefer not to use `./setup.sh`, or you used `./setup.sh --no-build`.

#### 1. Prerequisites (install in any order)

| What | Why | How |
|------|-----|-----|
| **Xcode Command Line Tools** | C compiler and linker for native code on macOS | Run `xcode-select --install` in Terminal and complete the installer. |
| **Node.js** (current **LTS** is fine) | Runs the frontend tooling and `pnpm` | Download from [nodejs.org](https://nodejs.org/), or use a version manager such as [nvm](https://github.com/nvm-sh/nvm) or [fnm](https://github.com/Schniz/fnm). |
| **pnpm** | This repo uses pnpm (see `package.json`) | With Node installed: `corepack enable` then `corepack prepare pnpm@9.4.0 --activate`, or `npm install -g pnpm`. |
| **Rust** | Compiles the Tauri / Rust backend | Install [rustup](https://rustup.rs/) (the official installer). Use the default stable toolchain for your Mac (**aarch64-apple-darwin** on Apple Silicon). |

You do **not** need Python for a normal `pnpm tauri build` on macOS.

#### 2. Clone and install dependencies

From the directory where you keep projects:

```bash
git clone https://github.com/goyal-prateek/slap-my-mac.git
cd slap-my-mac
pnpm install
```

#### 3. Build the app

From the **repository root** (the folder that contains `package.json` and `src-tauri/`):

```bash
pnpm tauri build
```

The first run downloads Rust crates and can take several minutes.

#### 4. Install from the DMG

When the build finishes, open the disk image under:

`src-tauri/target/release/bundle/dmg/`

The file name includes the version and architecture (for example `Slap My Mac_0.1.0_aarch64.dmg`). Double-click it, then drag **Slap My Mac** into **Applications** and launch it from there.

#### 5. Remove the build output (recommended)

Tauri also writes an app bundle under `src-tauri/target/release/bundle/macos/`. After you have installed from the DMG, keeping that tree around can mean two copies of the app on disk (the one in **Applications** and the one inside `target`), which is easy to confuse.

Delete the Cargo output directory when you are done:

```bash
rm -rf src-tauri/target
```

You can always run `pnpm tauri build` again later; the next build will recreate `target/`.