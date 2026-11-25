# Bytekodex Platform: notes for AI agents

The engine of Bytekodex, in Rust. It reads compiled bytecode (JVM class files today, .NET IL some day), turns it into a platform-neutral token stream, and draws that stream as an indexed PNG. Hosts reach it over a C ABI: the Telegram bot (bytekodex/bytekodex-telegram) links it statically through cgo. README.md is the technical overview, README.RU.md its Russian copy, CHANGELOG.md what each release brings.

## Talking to the owner

- Reply in Russian, address the owner as «вы». Quotes «ёлочки», nested „лапки“, an em dash with spaces « — ».
- Code, comments, commit messages, CHANGELOG.md, and README.md are in English: curly quotes “…”, an en dash with spaces (“text – text”), never an em dash glued to the words. No semicolon between clauses: a full stop, a comma, or a colon instead. README.RU.md follows Russian typography. Change both READMEs together.
- Commits are short one-line imperatives, one logical change each: “Render the stack map of a JDK 29 class”.

## How it fits together

| Crate | Depends on | Role |
|---|---|---|
| `bk-core` | nothing | Token IR, document, stats, errors: the contract every other crate agrees on. It stays dependency-free |
| `bk-theme` | core | Token kind to color |
| `bk-jvm` | core, logos | Class file reader (`class.rs`, `pool.rs`, `stackmap.rs`), opcode table, the javap-text lexer |
| `bk-render` | core, theme, fontdue, png | Monospace grid renderer and indexed PNG encoder. No graphics library: layout is arithmetic |
| `bk-ffi` | core, jvm, render, theme | The C ABI: `cdylib`, `staticlib`, and `rlib` named `bytekodex` |
| `bk-cli` | core, jvm, render, theme, rayon | `bk`, for dumping and rendering class files by hand |

A new bytecode format is a new crate that implements `bk_core::Frontend`. Nothing in `bk-render` knows what an opcode is.

## Rules that keep it working

- **`include/bytekodex.h` is the ABI's specification,** written by hand. Change it only on purpose, raise `BK_ABI_VERSION` with any change a host would notice, and remember the bot carries a copy (`internal/render/include/bytekodex.h`) that its CI compares against the release it builds.
- **Caller-owned buffers.** Nothing crosses the boundary owning memory except the renderer handle. A too-small buffer returns `BK_ERR_BUFFER_TOO_SMALL` with the exact size in `response.required`.
- **No panic crosses the boundary.** `bk-ffi` wraps every entry in `catch_unwind`, which is why `panic = "abort"` stays out of `[profile.release]`.
- **No `javap`, no JDK.** The class file is parsed directly. A new class file feature (a new attribute, a new stack map frame) goes into `bk-jvm` with a test over real bytes.

## Versions

- Rust: `rust-toolchain.toml`. The bot's `Dockerfile.bot` uses the same version (`rust:<version>-slim-trixie`), so a change here goes there in the bot's next update. `rust-version` in `Cargo.toml` is the oldest Rust the code compiles with.
- Crates: `[workspace.dependencies]` in `Cargo.toml`, resolved in `Cargo.lock`.
- Actions: `uses:` in `.github/workflows/`.
- `/update-deps` (`.claude/skills/update-deps/`) moves all of them.

## Checking a change

```shell
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p bk-ffi
```

Renderer tests skip themselves without a monospace face. On Linux install `fonts-dejavu-core` first, or the renderer goes untested. After a change to what the bot sees, build the bot against it too: in the superproject, `telegram/` sits next to this repository, and its CLAUDE.md says how.

## Releasing

- After a change that reaches the library, `bk`, or the header, add it to CHANGELOG.md in the same push: under the topmost “## X.Y.Z” section while that version isn't released (`gh release view vX.Y.Z` fails), or under a new section with the next version. A new feature is a minor version, a fix or an update a patch, an ABI change a major.
- Release: `gh workflow run release.yml -R bytekodex/bytekodex-platform -f version=X.Y.Z`. It writes the version into its build of `Cargo.toml`, runs the tests on Linux (x64, arm64), macOS, and Windows, and publishes the `vX.Y.Z` release with one archive per platform and `bytekodex.h`. Check that the release has all four archives.
- Commit the new version into `[workspace.package] version` in `Cargo.toml` after the release, with the changelog's next change at the latest.
- The bot moves to the new release in its own repository: `PLATFORM_VERSION` there names the tag it builds from.

## Autopilot

`.github/workflows/autopilot.yml` runs Claude every Monday at 03:00 UTC with `CLAUDE_CODE_OAUTH_TOKEN`: `/update-deps`, then a release when something that ships changed. The bot's autopilot runs at 05:00 and picks the release up. The secret comes from MairwunNx/infrastructure (`github.toml`, `scripts/secrets.py github`).
