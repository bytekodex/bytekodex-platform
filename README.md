# 🦀 Bytekodex Platform

<p align="center">
  <a href="https://github.com/bytekodex/bytekodex-platform/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/bytekodex/bytekodex-platform?label=release"></a>
  <a href="https://github.com/bytekodex/bytekodex-platform/actions/workflows/ci.yml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/bytekodex/bytekodex-platform/ci.yml?branch=master&label=ci"></a>
  <img alt="Rust 1.100" src="https://img.shields.io/badge/Rust-1.100-CE422B?logo=rust&logoColor=white">
  <img alt="C ABI" src="https://img.shields.io/badge/C%20ABI-v1-555555">
  <img alt="Linux | macOS | Windows" src="https://img.shields.io/badge/Linux%20%7C%20macOS%20%7C%20Windows-x64%20%7C%20arm64-2F6FEB">
  <a href="LICENSE"><img alt="License" src="https://img.shields.io/github/license/bytekodex/bytekodex-platform"></a>
</p>

🦀 **Bytekodex Platform** reads compiled bytecode and draws it. It parses JVM class files on its own, turns them into a platform-neutral token stream, and renders that stream to an indexed PNG, all in Rust behind a small C ABI. [@bytekodex_bot](https://github.com/bytekodex/bytekodex-telegram) is built on it.

[Русская версия](README.RU.md)

<p align="center"><img src="https://raw.githubusercontent.com/bytekodex/bytekodex-telegram/master/docs/java-switch.png" width="760" alt="The bytecode of a sealed interface switch, drawn by the platform: methods, offsets, and opcodes in color"></p>

## Get started

Take the archive for your platform from the [latest release](https://github.com/bytekodex/bytekodex-platform/releases/latest): Linux (x64 and arm64), macOS on Apple silicon, or Windows (x64). Each one holds the shared and the static library, [`bytekodex.h`](include/bytekodex.h), and `bk`, a command line tool to try it out:

```shell
bk dump --pool --locals Foo.class
bk render Foo.class --font /path/to/JetBrainsMono-Regular.ttf --size 26 --rows 34 --out page
```

`--font` must be a single TrueType face. A `.ttc` collection is not one and is refused.

## What changed from the painter

This replaces [bytekodex-painter](https://github.com/bytekodex/bytekodex-painter) and [bytekodex-antlr](https://github.com/bytekodex/bytekodex-antlr), both archived.

**No `javap`.** The class file is parsed directly. Shelling out to `javap` meant forking a JVM per call, tens of milliseconds and tens of megabytes, to get human-readable text that then had to be lexed back into tokens. Reading the binary skips both halves: it takes microseconds, needs no JDK at runtime, and the tokens come out already knowing what they are.

**No ANTLR.** With the binary reader in place there is nothing to recognize on the fast path, so the grammar has no job. A lexer still exists, on `logos`, for the one case that needs one: a user pasting `javap` output into the bot.

**No 2D graphics library.** Bytecode is monospace text on a flat background. Position is `column * advance`, glyphs are rasterized once into a cache, and the only shape drawn is four rounded corners. Skia was carrying a full text and vector pipeline for work we never asked it to do.

**Indexed output.** A dump uses about twenty colors, so the framebuffer stores one byte per pixel: which color, how covered. The PNG palette does the blending. A quarter of the memory, and a much smaller file.

## Layout

| Crate | Depends on | Role |
| --- | --- | --- |
| `bk-core` | nothing | Token IR, document, stats, errors. The contract every other crate agrees on |
| `bk-theme` | core | Token kind to color |
| `bk-jvm` | core, logos | Class file reader, opcode table, javap-text lexer |
| `bk-render` | core, theme, fontdue, png | Monospace grid renderer, indexed PNG encoder |
| `bk-ffi` | core, jvm, render, theme | The C ABI: `cdylib` and `staticlib` |
| `bk-cli` | core, jvm, render, theme, rayon | `bk`, for driving it locally |

Adding a bytecode format means adding a crate that implements `bk_core::Frontend`. Nothing in `bk-render` knows what an opcode is.

## Build

Rust 1.100, pinned in [`rust-toolchain.toml`](rust-toolchain.toml): rustup fetches it on the first `cargo` call.

```shell
cargo build --release          # target/release/libbytekodex.{so,dylib,dll} and libbytekodex.a
cargo test --workspace
cargo run --bin bk -- dump --pool --locals Foo.class
```

## C ABI

[`include/bytekodex.h`](include/bytekodex.h) is the specification, written by hand so that changing it is deliberate. In short:

```c
bk_renderer *r = bk_renderer_new(font, 40.0f, &status);

bk_response response;
int32_t status = bk_render(r, &request, buffer, capacity, &response);
if (status == BK_ERR_BUFFER_TOO_SMALL) {
    /* response.required is exact: grow and retry */
}
```

The output buffer belongs to the caller. Nothing is returned that has to be freed except the renderer handle, so there is no allocator crossing the boundary and no leak to forget. Counts land in `response` even when rendering fails, so a caller that hit a size limit can still report how many opcodes it saw.

## Notes on performance

Class files are one to fifty kilobytes. At that size `read` into a reused buffer beats `mmap`, whose page faults and teardown cost more than the copy. So `memmap2` is not a dependency here, and belongs behind a flag for jars.

Opcode counting uses a plain local counter. An atomic increment per instruction would dominate the parse on a large method for no benefit, since each worker owns its own class, and `rayon` folds the per-class results at the end. The one atomic in the codebase counts opcodes per process, is touched once per request, and is read only by metrics.

## Verified against

997 class files from an ANTLR distribution (versions 49.0, 50.0, and 51.0) parse with zero malformed instructions or unknown opcodes.

## Releases

[CHANGELOG.md](CHANGELOG.md) lists what each version brings. A release is a manual run of the “Release” workflow with the new version, and every Monday an autopilot updates the dependencies and releases what changed.

## Related projects

- **[@bytekodex_bot](https://github.com/bytekodex/bytekodex-telegram)** – the Telegram bot built on this library: send Java, Kotlin, or Groovy, get a picture of its bytecode.
- **[Bytekodex workspace](https://github.com/bytekodex/.github)** – every Bytekodex repository in one clone, as submodules.
- **[bytekodex-painter](https://github.com/bytekodex/bytekodex-painter)** – the original ANTLR and Skia painter this library replaces, archived.
- **[bytekodex-antlr](https://github.com/bytekodex/bytekodex-antlr)** – the painter’s javap grammars, archived.
- **[Capsa](https://github.com/MairwunNx/Capsa)** – a Kotlin Multiplatform engine for screens built from small capabilities.
- **[WorldAPI](https://github.com/MairwunNx/minecraft-worldapi)** – a Minecraft server on HTTP, for sites, bots, and AI agents.

> 🫡 Made by Pavel Erokhin (Павел Ерохин), aka mairwunnx.
