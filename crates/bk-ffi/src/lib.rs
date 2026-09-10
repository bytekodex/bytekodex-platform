//! The C ABI.
//!
//! Three properties the previous FFI did not have:
//!
//! Nothing panics across the boundary. Every entry point is wrapped in `catch_unwind`, so a bug
//! becomes [`BK_ERR_INTERNAL`] instead of aborting the host process. The old `paint` called
//! `unwrap()` on the input's UTF-8 validity, which took the whole bot down on malformed input.
//!
//! Nothing crosses the boundary owning memory. The caller passes a buffer and the PNG is
//! written straight into it, so there is no allocation to hand back and no `free` to forget.
//! When the buffer is too small the exact required size comes back and the caller retries —
//! which pairs with a pooled buffer on the Go side for an allocation-free steady state.
//!
//! Nothing is a C string. Inputs are pointer-plus-length, because class files contain NUL bytes
//! and Go strings are not NUL-terminated, so `CString` meant both a copy and a hazard.

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};

use bk_core::{DocumentBuilder, Error, Frontend, InputKind, Platform, ViewFlags, ViewOptions};
use bk_jvm::JvmFrontend;
use bk_render::{RenderOptions, Renderer};

/// Bumped whenever `bk_request` or `bk_response` changes shape. The caller sends the version it
/// was built against and gets [`BK_ERR_ABI_MISMATCH`] if the two disagree, which turns a
/// silent struct-layout corruption into a clear error on the first call.
pub const BK_ABI_VERSION: u32 = 1;

pub const BK_OK: i32 = 0;
pub const BK_ERR_ABI_MISMATCH: i32 = -1;
pub const BK_ERR_INVALID_ARGUMENT: i32 = -2;
pub const BK_ERR_BUFFER_TOO_SMALL: i32 = -7;
pub const BK_ERR_INTERNAL: i32 = -99;

pub const BK_PLATFORM_JVM: u32 = 1;
pub const BK_PLATFORM_CIL: u32 = 2;

pub const BK_INPUT_BINARY: u32 = 1;
pub const BK_INPUT_DISASSEMBLY_TEXT: u32 = 2;

/// A borrowed byte run. Never owned by the callee.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BkSlice {
    pub ptr: *const u8,
    pub len: usize,
}

impl BkSlice {
    /// # Safety
    /// `ptr` must be valid for `len` bytes for the duration of the call.
    unsafe fn as_slice(&self) -> Option<&[u8]> {
        if self.ptr.is_null() || self.len == 0 {
            return None;
        }
        Some(unsafe { std::slice::from_raw_parts(self.ptr, self.len) })
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct BkRequest {
    pub abi_version: u32,
    pub platform: u32,
    pub input_kind: u32,
    pub view_flags: u32,
    pub page: u32,
    pub page_rows: u32,
    pub font_size: f32,
    pub margin: u32,
    pub corner_radius: u32,
    /// Zero means the renderer's default.
    pub max_dimension_sum: u32,
    pub input: BkSlice,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct BkResponse {
    /// Bytes of PNG written into the output buffer.
    pub written: usize,
    /// Bytes the output buffer needs. Meaningful when the status is
    /// [`BK_ERR_BUFFER_TOO_SMALL`], and equal to `written` on success.
    pub required: usize,
    pub pages_total: u32,
    pub width: u32,
    pub height: u32,
    pub opcodes_total: u64,
    pub methods: u32,
    pub fields: u32,
    pub status: i32,
}

/// A renderer with its glyph atlas already built.
///
/// Opaque and long-lived on purpose: rasterizing the ASCII range costs real time, and doing it
/// per request would throw away the cache that makes rendering cheap. Not thread-safe — the
/// atlas fills in lazily — so the caller keeps one per worker or guards it.
pub struct BkRenderer {
    inner: Renderer,
}

thread_local! {
    static LAST_ERROR: RefCell<String> = const { RefCell::new(String::new()) };
}

fn remember(error: &Error) -> i32 {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = error.to_string());
    error.code()
}

fn remember_message(message: &str, code: i32) -> i32 {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = message.to_string());
    code
}

/// Copies the last error of the calling thread into `buf` as NUL-terminated text and returns
/// the number of bytes it needs, including the terminator.
///
/// # Safety
/// `buf` must be writable for `cap` bytes, or null to query the required size.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bk_last_error(buf: *mut u8, cap: usize) -> usize {
    LAST_ERROR.with(|slot| {
        let message = slot.borrow();
        let needed = message.len() + 1;
        if !buf.is_null() && cap > 0 {
            let copy = message.len().min(cap - 1);
            unsafe {
                std::ptr::copy_nonoverlapping(message.as_ptr(), buf, copy);
                buf.add(copy).write(0);
            }
        }
        needed
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn bk_abi_version() -> u32 {
    BK_ABI_VERSION
}

/// Total opcodes this library has decoded since it was loaded, for metrics.
#[unsafe(no_mangle)]
pub extern "C" fn bk_opcodes_decoded_total() -> u64 {
    bk_core::stats::total_opcodes()
}

/// Builds a renderer from font bytes the caller owns; the bytes are not retained.
///
/// # Safety
/// `font` must describe a readable byte run, and `status` must be writable or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bk_renderer_new(
    font: BkSlice,
    font_size: f32,
    status: *mut i32,
) -> *mut BkRenderer {
    let set = |code: i32| {
        if !status.is_null() {
            unsafe { status.write(code) };
        }
    };

    let result = catch_unwind(AssertUnwindSafe(|| {
        let Some(bytes) = (unsafe { font.as_slice() }) else {
            return Err(Error::InvalidArgument("font slice is empty"));
        };
        Renderer::new(bytes, font_size, &bk_theme::DARK)
    }));

    match result {
        Ok(Ok(renderer)) => {
            set(BK_OK);
            Box::into_raw(Box::new(BkRenderer { inner: renderer }))
        }
        Ok(Err(error)) => {
            set(remember(&error));
            std::ptr::null_mut()
        }
        Err(_) => {
            set(remember_message(
                "panic while building the renderer",
                BK_ERR_INTERNAL,
            ));
            std::ptr::null_mut()
        }
    }
}

/// # Safety
/// `renderer` must come from [`bk_renderer_new`] and must not be used afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bk_renderer_free(renderer: *mut BkRenderer) {
    if renderer.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(renderer) });
}

/// Renders one page of `request.input` into `out`.
///
/// On [`BK_ERR_BUFFER_TOO_SMALL`] nothing usable was written, but `response.required` holds the
/// exact size, so the caller grows its buffer and calls again. Every other field of `response`
/// is filled in either way, so counts are available even when the image did not fit.
///
/// # Safety
/// All pointers must be valid for the duration of the call: `request` and `response` readable
/// and writable respectively, and `out` writable for `out_cap` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bk_render(
    renderer: *mut BkRenderer,
    request: *const BkRequest,
    out: *mut u8,
    out_cap: usize,
    response: *mut BkResponse,
) -> i32 {
    if renderer.is_null() || request.is_null() || response.is_null() {
        return remember_message(
            "null renderer, request or response",
            BK_ERR_INVALID_ARGUMENT,
        );
    }

    let result = catch_unwind(AssertUnwindSafe(|| {
        let renderer = unsafe { &mut *renderer };
        let request = unsafe { &*request };
        let buffer = if out.is_null() || out_cap == 0 {
            &mut [][..]
        } else {
            unsafe { std::slice::from_raw_parts_mut(out, out_cap) }
        };
        render(renderer, request, buffer)
    }));

    let (outcome, status) = match result {
        Ok(Ok(response)) => (response, BK_OK),
        Ok(Err((partial, error))) => {
            let code = remember(&error);
            (partial, code)
        }
        Err(_) => (
            BkResponse::default(),
            remember_message("panic while rendering", BK_ERR_INTERNAL),
        ),
    };

    let mut outcome = outcome;
    outcome.status = status;
    unsafe { response.write(outcome) };
    status
}

type RenderOutcome = Result<BkResponse, (BkResponse, Error)>;

fn render(renderer: &mut BkRenderer, request: &BkRequest, out: &mut [u8]) -> RenderOutcome {
    let fail = |error: Error| Err((BkResponse::default(), error));

    if request.abi_version != BK_ABI_VERSION {
        return fail(Error::AbiMismatch {
            expected: BK_ABI_VERSION,
            got: request.abi_version,
        });
    }

    let platform = match Platform::from_raw(request.platform) {
        Ok(platform) => platform,
        Err(error) => return fail(error),
    };
    if platform != Platform::Jvm {
        return fail(Error::UnsupportedPlatform(request.platform));
    }
    let input_kind = match InputKind::from_raw(request.input_kind) {
        Ok(kind) => kind,
        Err(error) => return fail(error),
    };
    let Some(input) = (unsafe { request.input.as_slice() }) else {
        return fail(Error::InvalidArgument("input slice is empty"));
    };

    let view = ViewOptions {
        flags: if request.view_flags == 0 {
            ViewFlags::default()
        } else {
            ViewFlags(request.view_flags)
        },
        page: request.page,
        page_rows: if request.page_rows == 0 {
            ViewOptions::DEFAULT_PAGE_ROWS
        } else {
            request.page_rows
        },
    };

    let mut builder = DocumentBuilder::with_capacity(input.len() * 8, input.len());
    let stats = match JvmFrontend.emit(input, input_kind, &view, &mut builder) {
        Ok(stats) => stats,
        Err(error) => return Err((BkResponse::default(), error)),
    };
    let document = builder.finish();

    // Counts survive even a failed render, so a caller that hit a size limit can still report
    // "3 methods, 418 opcodes" instead of nothing.
    let partial = BkResponse {
        pages_total: Renderer::pages(&document, view.page_rows),
        opcodes_total: stats.opcodes_total,
        methods: stats.methods,
        fields: stats.fields,
        ..BkResponse::default()
    };

    let defaults = RenderOptions::default();
    let options = RenderOptions {
        font_size: defaults.font_size,
        margin: if request.margin == 0 {
            defaults.margin
        } else {
            request.margin
        },
        corner_radius: request.corner_radius,
        page: view.page,
        page_rows: view.page_rows,
        max_dimension_sum: if request.max_dimension_sum == 0 {
            defaults.max_dimension_sum
        } else {
            request.max_dimension_sum
        },
    };

    // The PNG encoder appends to a `Vec`, and a `Vec` cannot be made to live in caller memory,
    // so the encode runs into a staging buffer and the result is copied out once. That single
    // copy is the price of never exposing an allocator across the boundary — a memcpy of a few
    // hundred kilobytes, traded against a whole class of leaks that used to be structural.
    let mut staging = Vec::with_capacity(out.len().max(64 * 1024));
    let rendered = match renderer
        .inner
        .render_page(&document, &options, &mut staging)
    {
        Ok(rendered) => rendered,
        Err(error) => return Err((partial, error)),
    };

    let written = staging.len();
    if written > out.len() {
        return Err((
            BkResponse {
                required: written,
                ..partial
            },
            Error::BufferTooSmall { required: written },
        ));
    }
    out[..written].copy_from_slice(&staging);

    Ok(BkResponse {
        written,
        required: written,
        pages_total: rendered.pages_total,
        width: rendered.width,
        height: rendered.height,
        ..partial
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Option<Vec<u8>> {
        [
            "/System/Library/Fonts/SFNSMono.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
            "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
            "C:/Windows/Fonts/consola.ttf",
        ]
        .iter()
        .find_map(|path| std::fs::read(path).ok())
    }

    fn request(input: &[u8]) -> BkRequest {
        BkRequest {
            abi_version: BK_ABI_VERSION,
            platform: BK_PLATFORM_JVM,
            input_kind: BK_INPUT_DISASSEMBLY_TEXT,
            view_flags: 0,
            page: 0,
            page_rows: 0,
            font_size: 24.0,
            margin: 0,
            corner_radius: 12,
            max_dimension_sum: 0,
            input: BkSlice {
                ptr: input.as_ptr(),
                len: input.len(),
            },
        }
    }

    #[test]
    fn abi_mismatch_is_reported_not_ignored() {
        let Some(font_bytes) = font() else { return };
        let mut status = 0;
        let renderer = unsafe {
            bk_renderer_new(
                BkSlice {
                    ptr: font_bytes.as_ptr(),
                    len: font_bytes.len(),
                },
                24.0,
                &mut status,
            )
        };
        assert_eq!(status, BK_OK);

        let input = b"0: return";
        let mut req = request(input);
        req.abi_version = 999;
        let mut response = BkResponse::default();
        let code = unsafe { bk_render(renderer, &req, std::ptr::null_mut(), 0, &mut response) };

        assert_eq!(code, BK_ERR_ABI_MISMATCH);
        assert_eq!(response.status, BK_ERR_ABI_MISMATCH);
        unsafe { bk_renderer_free(renderer) };
    }

    #[test]
    fn too_small_a_buffer_reports_the_exact_size_needed() {
        let Some(font_bytes) = font() else { return };
        let mut status = 0;
        let renderer = unsafe {
            bk_renderer_new(
                BkSlice {
                    ptr: font_bytes.as_ptr(),
                    len: font_bytes.len(),
                },
                24.0,
                &mut status,
            )
        };

        let input = b"   0: iconst_0\n   1: ireturn\n";
        let req = request(input);

        let mut response = BkResponse::default();
        let mut tiny = [0u8; 4];
        let code =
            unsafe { bk_render(renderer, &req, tiny.as_mut_ptr(), tiny.len(), &mut response) };
        assert_eq!(code, BK_ERR_BUFFER_TOO_SMALL);
        assert!(response.required > tiny.len());
        assert_eq!(response.opcodes_total, 2);

        let mut buffer = vec![0u8; response.required];
        let mut second = BkResponse::default();
        let code = unsafe {
            bk_render(
                renderer,
                &req,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut second,
            )
        };
        assert_eq!(code, BK_OK);
        assert_eq!(second.written, response.required);
        assert_eq!(&buffer[..8], b"\x89PNG\r\n\x1a\n");

        unsafe { bk_renderer_free(renderer) };
    }

    #[test]
    fn last_error_is_readable_after_a_failure() {
        let mut status = 0;
        let bogus = unsafe {
            bk_renderer_new(
                BkSlice {
                    ptr: b"not a font".as_ptr(),
                    len: 10,
                },
                24.0,
                &mut status,
            )
        };
        assert!(bogus.is_null());

        let needed = unsafe { bk_last_error(std::ptr::null_mut(), 0) };
        let mut message = vec![0u8; needed];
        unsafe { bk_last_error(message.as_mut_ptr(), message.len()) };
        let text = String::from_utf8_lossy(&message[..needed - 1]);
        assert!(text.contains("font"), "unexpected message: {text}");
    }
}
