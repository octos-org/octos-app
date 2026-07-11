# Android reproduction guide (OnePlus 6 / DeepSeek / A2App)

How to rebuild and run the octos-app Android client end-to-end: on-device octos
server + native Makepad APK, DeepSeek chat, memory/compaction surfacing, and the
A2App/Splash live-UI feature. Written so another agent can repeat it from a
clean checkout.

> Secrets are **never** committed. The DeepSeek API key is supplied at runtime
> via the `DEEPSEEK_API_KEY` environment variable (see step 6).

---

## 0. Repo layout (must be sibling directories)

`octos-app`'s `Cargo.toml` uses **path dependencies** — Makepad crates from
`../aichat`, octos-core from `../octos` — so all four repos must sit side by
side under one parent:

```
<workspace>/
├── octos-app     github.com/octos-org/octos-app   branch: coding-green-m9-test-20260428
├── aichat        ymote/makepad (fork)             branch: octos-android-support   ← makepad framework fork the app compiles against
├── makepad       ymote/makepad (fork)             branch: octos-android-buildtool ← source cargo-makepad + NDK/SDK are installed from
└── octos         github.com/octos-org/octos       branch: main
```

`aichat` and `makepad` are two branches of the **same fork** (`ymote/makepad`);
clone it twice into the two directory names:

```sh
git clone -b coding-green-m9-test-20260428 https://github.com/octos-org/octos-app.git
git clone -b octos-android-support   https://github.com/ymote/makepad.git aichat
git clone -b octos-android-buildtool https://github.com/ymote/makepad.git makepad
git clone -b main https://github.com/octos-org/octos.git
```

## 1. Prerequisites

- Rust with the `aarch64-linux-android` target: `rustup target add aarch64-linux-android`
- Host build tools for the vendored native deps (aws-lc-sys/ring/zstd-sys): `cmake`, `perl`, a C compiler
- The Android SDK/NDK bundled with cargo-makepad. Install it and cargo-makepad
  **from the `makepad` repo** (its `CARGO_MANIFEST_DIR` is baked in at install
  time, and the build javac's `makepad/tools/cargo_makepad/.../MakepadActivity.java`):
  ```sh
  cargo install --path makepad/tools/cargo_makepad --locked
  cargo makepad android install-toolchain      # downloads the NDK under makepad/tools/cargo_makepad/android_33_*/
  ```
- `adb` to talk to the phone.

The NDK toolchain then lives at:
```
makepad/tools/cargo_makepad/android_33_linux_x64/ndk/28.2.13676358/toolchains/llvm/prebuilt/linux-x86_64/bin
```

## 2. NDK cross-compile env (used by both builds)

```sh
NDKBIN=<abs path>/makepad/tools/cargo_makepad/android_33_linux_x64/ndk/28.2.13676358/toolchains/llvm/prebuilt/linux-x86_64/bin
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$NDKBIN/aarch64-linux-android26-clang
export CC_aarch64_linux_android=$NDKBIN/aarch64-linux-android26-clang
export CXX_aarch64_linux_android=$NDKBIN/aarch64-linux-android26-clang++
export AR_aarch64_linux_android=$NDKBIN/llvm-ar
export RANLIB_aarch64_linux_android=$NDKBIN/llvm-ranlib
```

## 3. Build the octos server (Android binary)

`git`+`ast` add the `git` and `code_structure` agent tools; `browser` is already
default-on in octos-agent (it just needs headless Chrome, absent on Android).

```sh
cd octos
cargo build -p octos-cli --features "api,git,ast" --target aarch64-linux-android --release
# → octos/target/aarch64-linux-android/release/octos   (~90 MB)
```

## 4. Build the APK

Run from the `octos-app` directory (cargo-makepad resolves the crate dir from
CWD). Requires a `[lib]` target, `panic=unwind`, `lto=false` (prefer-dynamic).

```sh
cd octos-app
cargo makepad android build -p octos-app --release
# → octos-app/target/android/makepad-android-apk/octos_app/apk/octos_app.apk
# package: dev.makepad.octos_app / .MakepadApp
```

### 4b. Self-contained build (bundle octos, stdio transport)

On Android the app talks to octos over **stdio** — it spawns
`liboctos.so serve --stdio` (NDJSON JSON-RPC on stdin/stdout), no `octos serve`
daemon and no TCP port. `untrusted_app` may only exec from its
nativeLibraryDir, so the server binary must ship inside the APK as a `lib*.so`.

Bundle it with the `MAKEPAD_ANDROID_EXTRA_LIBS` env var (requires the
cargo-makepad patch on `ymote/makepad@dev`, commit `ad2fe48` — rebuild the tool
with `RUSTFLAGS="-Cprofile-use=$PWD/libs/box3d/box3d.profdata" cargo install
--path tools/cargo_makepad --force` from the makepad checkout):

```sh
cd octos-app
export MAKEPAD_ANDROID_EXTRA_LIBS="liboctos.so=$(cd ../octos && pwd)/target/aarch64-linux-android/release/octos"
cargo makepad android build -p octos-app --release   # look for "Bundled extra native lib: liboctos.so"
# APK grows to ~98 MB; verify: unzip -l …/octos_app.apk | grep liboctos.so
```

On install, Android extracts `lib/arm64-v8a/liboctos.so` to the app's
nativeLibraryDir as a real, exec-able file. If the binary is absent (plain
build), the app cleanly falls back to the WebSocket transport.

The stdio child needs a **per-app octos home** it can read/write under SELinux
(the app's own data dir — it cannot touch `/data/local/tmp`). The app spawns
octos with `HOME=/data/user/0/dev.makepad.octos_app/files/octos-home`; provision
that dir (owner = app uid, `restorecon`) with `.config/octos/config.json` whose
`env_vars.DEEPSEEK_API_KEY` holds the key inline — so the **app process never
handles the secret** (see step 6 for the config shape). A `.octos/profiles/yue`
profile must exist there. (Productionizing this via the QR/`apply_provision_string`
flow is a follow-up; today it is seeded by copying a working home.)

## 5. Deploy to the phone

Device: OnePlus 6, serial `cfb7c9e3`.

> **WSL note:** this workspace runs in WSL2 but the phone is attached to
> Windows. Use the Windows `adb.exe` (`/mnt/c/Users/dspfa/Dev/platform-tools/adb.exe`)
> for `install` (WSL's adb can't reach it, and `dl.google.com` is blocked in
> WSL). `adb push`/`shell` work from either.

```sh
# server binary
adb push octos/target/aarch64-linux-android/release/octos /data/local/tmp/octos
adb shell chmod 755 /data/local/tmp/octos

# APK — copy to a Windows path, then install with Windows adb
cp octos-app/target/android/makepad-android-apk/octos_app/apk/octos_app.apk /mnt/c/Users/dspfa/Dev/makepad-apks/
/mnt/c/Users/dspfa/Dev/platform-tools/adb.exe -s cfb7c9e3 install --no-incremental -r 'C:\Users\dspfa\Dev\makepad-apks\octos_app.apk'
```

## 6. Server config + run (DeepSeek + memory)

Everything lives under `HOME=/data/local/tmp/ohome`. Create the two config
files (via `adb shell "cat > … <<'EOF' … EOF"`):

`/data/local/tmp/ohome/.config/octos/config.json`
```json
{
  "provider": "deepseek",
  "model": "deepseek-v4-pro",
  "api_key_env": "DEEPSEEK_API_KEY",
  "memory": { "max_inject_tokens": 2500, "refresh": { "enabled": true, "min_idle_minutes": 5, "debounce_seconds": 30 } }
}
```

The per-profile store `…/.octos/profiles/yue.json` is created by the app's
silent solo-auth on first connect; the model + key resolve from the config
above (`config.env_vars.DEEPSEEK_API_KEY` / `config.llm.primary`).

Run the server (solo mode, port 50080) with the key in the env — **do not put
the key in any file**:

```sh
adb shell "export HOME=/data/local/tmp/ohome TMPDIR=/data/local/tmp DEEPSEEK_API_KEY=<your-deepseek-key>; /data/local/tmp/octos serve --solo"
```

Launch the app:
```sh
adb shell am start -S -n dev.makepad.octos_app/.MakepadApp
```

## 7. Verify

- App boots straight to chat (no login), top-left dot green, "Live".
- Chat with DeepSeek works; markdown renders; copy/share icons appear under
  answers; Share opens the Android share sheet.
- Top-bar context chip shows live `◔ <tokens> · <n> msgs` (requires the client
  to request the `context.lifecycle.v1` capability — already wired).
- **A2App**: toggle "A2App" in the composer, ask e.g. "a counter card titled
  Score with plus/minus buttons" or "beijing weather and air quality card" →
  the LLM returns a ```runsplash block rendered as a live, interactive card.
  Button taps drive `{{state.count}}` (single shared counter).

## Notes / gotchas

- The app connects to `ws://127.0.0.1:50080/api/ui-protocol/ws` on the phone's
  own loopback (server and app both on-device) — no `adb forward` needed.
- Shell runs **unsandboxed** on Android (no bwrap/seccomp/Docker) — the server
  logs a warning; shell tools still run.
- A2App: the Splash body **must start with a widget** (`RoundedView{`). A
  top-level `let X = View{…}` component definition fails to render (the prompt
  already forbids it).
- Episodic memory *recall* needs an embedding provider (DeepSeek has none);
  MEMORY.md injection + episode storage work without one.
- `makepad/.cargo/config.toml` may carry a machine-local PGO `profile-use` path
  — irrelevant to the app build (kept out of the committed history).
