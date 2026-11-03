//! Legacy (non-UTF-8) character encodings. The safe `vte4` bindings don't
//! wrap these — vte deprecated them in favor of UTF-8 everywhere — but the
//! C functions are still there (`vte4::ffi`) and still work when vte was
//! built with ICU, which is how Tilix's per-profile encoding works too.

use std::ffi::{CStr, CString};

use gtk4::glib;
use gtk4::glib::translate::ToGlibPtr;

/// vte's own name for UTF-8, and what an empty/unset encoding means.
pub const UTF8: &str = "UTF-8";

/// Every encoding this vte build can convert, UTF-8 first then sorted —
/// just UTF-8 when vte was built without ICU.
pub fn supported_encodings() -> Vec<String> {
    let mut encodings = Vec::new();
    // SAFETY: `vte_get_encodings` returns a newly allocated,
    // NULL-terminated array of NUL-terminated strings, which we own and
    // release with `g_strfreev` once copied.
    unsafe {
        let list = vte4::ffi::vte_get_encodings(glib::ffi::GFALSE);
        if !list.is_null() {
            let mut entry = list;
            while !(*entry).is_null() {
                encodings.push(CStr::from_ptr(*entry).to_string_lossy().into_owned());
                entry = entry.add(1);
            }
            glib::ffi::g_strfreev(list);
        }
    }
    encodings.retain(|e| e != UTF8);
    encodings.sort_by_key(|e| e.to_lowercase());
    encodings.insert(0, UTF8.to_string());
    encodings
}

/// Switches how `terminal` decodes its child's output (and encodes what's
/// typed). UTF-8 goes through `NULL`, vte's "reset to the default".
pub fn set_encoding(terminal: &vte4::Terminal, encoding: &str) -> Result<(), glib::Error> {
    let codeset = (encoding != UTF8 && !encoding.is_empty())
        .then(|| CString::new(encoding))
        .transpose()
        .map_err(|_| {
            glib::Error::new(glib::FileError::Inval, "encoding name contains a NUL byte")
        })?;
    let mut error = std::ptr::null_mut();
    // SAFETY: `terminal` is a live `VteTerminal` for the duration of the
    // call, `codeset` is either NULL or a valid NUL-terminated string that
    // outlives it, and on failure vte sets `error` to a GError we take
    // ownership of below.
    let ok = unsafe {
        vte4::ffi::vte_terminal_set_encoding(
            terminal.to_glib_none().0,
            codeset.as_ref().map_or(std::ptr::null(), |c| c.as_ptr()),
            &mut error,
        )
    };
    if ok == glib::ffi::GFALSE && !error.is_null() {
        // SAFETY: vte reported a failure and handed us this GError.
        return Err(unsafe { glib::translate::from_glib_full(error) });
    }
    Ok(())
}
