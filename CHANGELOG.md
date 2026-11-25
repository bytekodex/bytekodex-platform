# Changelog

The “Release” workflow takes the notes for a version from its section here, unless you type them into its `changelog` field.

## 1.0.0

- 🎉 **The first release.** The platform reads JVM class files on its own, from Java 1.0 up to the frames JDK 28 adds to the StackMapTable, turns them into a token stream, and draws that stream as an indexed PNG. No `javap`, no JDK, no graphics library.
- 📦 **Ready-made libraries.** Each release carries an archive for Linux (x64 and arm64), macOS on Apple silicon, and Windows (x64): the shared and the static library, `bytekodex.h`, and the `bk` command line tool.
- 🦀 Rust is pinned to 1.100.0 in `rust-toolchain.toml`, so CI, releases, and the bot’s image build with the same compiler.
- ⬆️ Updated fontdue to 0.9.5 and dropped memmap2 and tiny-skia, which nothing used.
