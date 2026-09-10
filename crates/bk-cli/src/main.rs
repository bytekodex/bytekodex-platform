//! Local driver: `bk dump` prints the disassembly, `bk render` writes PNG pages.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bk_core::{Document, DocumentBuilder, Frontend, Stats, ViewFlags, ViewOptions};
use bk_jvm::JvmFrontend;
use bk_render::{RenderOptions, Renderer};
use rayon::prelude::*;

const USAGE: &str = "\
bk — bytekodex platform driver

usage:
  bk dump   <file.class|file.txt>... [options]
  bk render <file.class|file.txt>... --font <font.ttf> [options]
  bk diagnostic <compiler-output.txt>... --font <font.ttf> [options]

options:
  --font <path>       monospace TrueType font, required by render
  --size <px>         font size, default 40
  --out <prefix>      output prefix for render, default 'page'
  --rows <n>          lines per page, default 90
  --pool              include the constant pool
  --locals            include the local variable table
  --all               include everything
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().cloned() else {
        eprint!("{USAGE}");
        return ExitCode::FAILURE;
    };

    match run(&command, &args[1..]) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("bk: {message}");
            ExitCode::FAILURE
        }
    }
}

struct Options {
    inputs: Vec<PathBuf>,
    font: Option<PathBuf>,
    size: f32,
    out_prefix: String,
    view: ViewOptions,
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        inputs: Vec::new(),
        font: None,
        size: 40.0,
        out_prefix: "page".into(),
        view: ViewOptions::default(),
    };

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let mut value = || {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{arg} needs a value"))
        };
        match arg.as_str() {
            "--font" => options.font = Some(PathBuf::from(value()?)),
            "--size" => options.size = value()?.parse().map_err(|_| "--size must be a number")?,
            "--out" => options.out_prefix = value()?,
            "--rows" => {
                options.view.page_rows = value()?.parse().map_err(|_| "--rows must be a number")?
            }
            "--pool" => options.view.flags = options.view.flags.union(ViewFlags::CONSTANT_POOL),
            "--locals" => options.view.flags = options.view.flags.union(ViewFlags::LOCALS),
            "--all" => options.view.flags = ViewFlags::ALL,
            other if other.starts_with("--") => return Err(format!("unknown option {other}")),
            path => options.inputs.push(PathBuf::from(path)),
        }
    }

    if options.inputs.is_empty() {
        return Err("no input files".into());
    }
    Ok(options)
}

fn run(command: &str, args: &[String]) -> Result<(), String> {
    let options = parse(args)?;
    let (document, stats) = if command == "diagnostic" {
        build_diagnostic(&options)?
    } else {
        build(&options)?
    };

    match command {
        "dump" => {
            print!("{}", document.text());
            report(&stats, &document);
            Ok(())
        }
        "render" | "diagnostic" => {
            let font_path = options
                .font
                .as_deref()
                .ok_or("render needs --font <font.ttf>")?;
            render(&document, &stats, font_path, &options)
        }
        other => Err(format!("unknown command {other}\n\n{USAGE}")),
    }
}

/// Reads compiler output and lays it out as a document. Its own command because a diagnostic is
/// not bytecode: there is nothing to disassemble and no platform to pick.
fn build_diagnostic(options: &Options) -> Result<(Document, Stats), String> {
    let mut builder = DocumentBuilder::new();
    for path in &options.inputs {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        bk_core::diagnostic::emit("", &text, &mut builder);
    }
    Ok((builder.finish(), Stats::default()))
}

/// Reads and parses every input, then concatenates the results in argument order.
///
/// Parsing runs in parallel because compiling one Kotlin file routinely produces a dozen class
/// files, and they are independent. Below a handful of files the thread pool costs more than it
/// saves, so the sequential path is kept for the common case of one or two.
fn build(options: &Options) -> Result<(Document, Stats), String> {
    const PARALLEL_THRESHOLD: usize = 2;

    let read = |path: &Path| std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()));

    let mut builder = DocumentBuilder::new();
    let mut total = Stats::default();

    let bytes: Vec<Vec<u8>> = if options.inputs.len() > PARALLEL_THRESHOLD {
        options
            .inputs
            .par_iter()
            .map(|p| read(p))
            .collect::<Result<_, _>>()?
    } else {
        options
            .inputs
            .iter()
            .map(|p| read(p))
            .collect::<Result<_, _>>()?
    };

    for (path, input) in options.inputs.iter().zip(&bytes) {
        let kind = bk_jvm::detect_input_kind(input);
        let stats = JvmFrontend
            .emit(input, kind, &options.view, &mut builder)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        total = total.merge(stats);
        builder.newline();
    }

    Ok((builder.finish(), total))
}

fn render(
    document: &Document,
    stats: &Stats,
    font_path: &Path,
    options: &Options,
) -> Result<(), String> {
    let font = std::fs::read(font_path).map_err(|e| format!("{}: {e}", font_path.display()))?;
    let mut renderer =
        Renderer::new(&font, options.size, &bk_theme::DARK).map_err(|e| e.to_string())?;
    if !renderer.is_monospace() {
        eprintln!(
            "bk: warning — {} is not monospace, columns will drift",
            font_path.display()
        );
    }

    let pages = Renderer::pages(document, options.view.page_rows);
    let mut png = Vec::with_capacity(512 * 1024);

    for page in 0..pages {
        png.clear();
        let render_options = RenderOptions {
            font_size: options.size,
            page,
            page_rows: options.view.page_rows,
            ..RenderOptions::default()
        };
        let rendered = renderer
            .render_page(document, &render_options, &mut png)
            .map_err(|e| format!("page {page}: {e}"))?;

        let path = format!("{}-{:02}.png", options.out_prefix, page + 1);
        std::fs::write(&path, &png).map_err(|e| format!("{path}: {e}"))?;
        println!(
            "{path}  {}x{}  {} KiB",
            rendered.width,
            rendered.height,
            png.len() / 1024
        );
    }

    report(stats, document);
    Ok(())
}

fn report(stats: &Stats, document: &Document) {
    eprintln!(
        "\n{} class(es), {} field(s), {} method(s), {} opcode(s), {} line(s)",
        stats.classes,
        stats.fields,
        stats.methods,
        stats.opcodes_total,
        document.rows()
    );

    let mut heaviest: Vec<_> = stats.per_method.iter().collect();
    heaviest.sort_unstable_by_key(|method| std::cmp::Reverse(method.opcodes));
    for method in heaviest.iter().take(5) {
        eprintln!(
            "  {:>5} opcodes  {}{}",
            method.opcodes, method.name, method.descriptor
        );
    }
}
