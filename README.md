# bytekodex-platform

Reads compiled bytecode and draws it. In Rust, exposed over a C ABI.

This replaces [bytekodex-painter](https://github.com/bytekodex/bytekodex-painter) and
[bytekodex-antlr](https://github.com/bytekodex/bytekodex-antlr), both archived.

## What changed from the painter

**No `javap`.** The class file is parsed directly. Shelling out to `javap` meant forking a JVM
per call — tens of milliseconds and tens of megabytes — to get human-readable text that then had
to be lexed back into tokens. Reading the binary skips both halves: it takes microseconds, needs
no JDK at runtime, and the tokens come out already knowing what they are.

**No ANTLR.** With the binary reader in place there is nothing to recognize on the fast path, so
the grammar has no job. A lexer still exists, on `logos`, for the one case that genuinely needs
one: a user pasting `javap` output into the bot.

**No 2D graphics library.** Bytecode is monospace text on a flat background. Position is
`column * advance`, glyphs are rasterized once into a cache, and the only shape drawn is four
rounded corners. Skia was carrying a full text and vector pipeline for work we never asked it to
do.

**Indexed output.** A dump uses about twenty colors, so the framebuffer stores one byte per
pixel — which color, how covered — and the PNG palette does the blending. A quarter of the
memory, and a much smaller file.

## Layout

| Crate | Depends on | Role |
| --- | --- | --- |
| `bk-core` | nothing | Token IR, document, stats, errors. The contract every other crate agrees on. |
| `bk-theme` | core | Token kind to color |
| `bk-jvm` | core, logos | Class file reader, opcode table, javap-text lexer |
| `bk-render` | core, theme, fontdue, png | Monospace grid renderer, indexed PNG encoder |
| `bk-ffi` | core, jvm, render, theme | The C ABI, `cdylib` |
| `bk-cli` | core, jvm, render, theme, rayon | `bk`, for driving it locally |

Adding a bytecode format means adding a crate that implements `bk_core::Frontend`. Nothing in
`bk-render` knows what an opcode is.

## Build

```shell
cargo build --release          # target/release/libbytekodex.{so,dylib,dll}
cargo test --workspace
```

## Try it

```shell
cargo run --bin bk -- dump --pool --locals Foo.class

cargo run --bin bk -- render Foo.class \
  --font /path/to/JetBrainsMono-Regular.ttf --size 26 --rows 34 --out page
```

`--font` must be a single TrueType face. A `.ttc` collection is not one and will be rejected.

## C ABI

[`include/bytekodex.h`](include/bytekodex.h) is the specification, written by hand so that
changing it is deliberate. In short:

```c
bk_renderer *r = bk_renderer_new(font, 40.0f, &status);

bk_response response;
int32_t status = bk_render(r, &request, buffer, capacity, &response);
if (status == BK_ERR_BUFFER_TOO_SMALL) {
    /* response.required is exact — grow and retry */
}
```

The output buffer belongs to the caller. Nothing is returned that has to be freed except the
renderer handle, so there is no allocator crossing the boundary and no leak to forget. Counts
land in `response` even when rendering fails, so a caller that hit a size limit can still report
how many opcodes it saw.

## Notes on performance

Class files are one to fifty kilobytes. At that size `read` into a reused buffer beats `mmap`,
whose page faults and teardown cost more than the copy — so `memmap2` is not a dependency here,
and belongs behind a flag for jars.

Opcode counting uses a plain local counter. An atomic increment per instruction would dominate
the parse on a large method for no benefit, since each worker owns its own class; `rayon` folds
the per-class results at the end. The one atomic in the codebase counts opcodes per process, is
touched once per request, and is read only by metrics.

## Verified against

997 class files from an ANTLR distribution (versions 49.0, 50.0 and 51.0) parse with zero
malformed instructions or unknown opcodes.

