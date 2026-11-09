//! The Windows Recycle Bin, through the shell's `IFileOperation`.
//!
//! - Only recycles. The shell deletes for good what the Recycle Bin cannot
//!   take (too big, a drive without one); the progress sink refuses those
//!   items before that happens, so the file stays where it is and the
//!   caller gets an error. Nothing is ever deleted instead.
//! - Reports where each item went (`<drive>\$Recycle.Bin\<SID>\$R…`), so
//!   undoing is a rename back, as on macOS; [`forget`] then removes the
//!   matching `$I…` file, which holds the original path and date.
//!
//! Each operation runs on its own thread with COM initialised only there,
//! so the async runtime's threads are never touched.
//!
//! `unsafe` is allowed only in this platform module: COM calls are `unsafe`
//! in the `windows` crate. Each block says why it is sound.
#![allow(unsafe_code)]

use std::cell::RefCell;
use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use windows::Win32::Foundation::E_ABORT;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize,
};
use windows::Win32::UI::Shell::{
    FOF_ALLOWUNDO, FOF_NO_UI, FOFX_EARLYFAILURE, FOFX_RECYCLEONDELETE, FileOperation,
    IFileOperation, IFileOperationProgressSink, IFileOperationProgressSink_Impl, IShellItem,
    SHCreateItemFromParsingName, SIGDN_FILESYSPATH, TSF_DELETE_RECYCLE_IF_POSSIBLE,
};
use windows::core::{HRESULT, HSTRING, PCWSTR, Ref};
use windows_core::implement;

/// Moves `path` (a file or a folder) to the Recycle Bin and returns where it
/// is now. On error the file has not moved.
pub fn trash(path: &Path) -> io::Result<PathBuf> {
    let path = shell_path(path);
    std::thread::spawn(move || {
        // SAFETY: initialises COM on this new thread only; balanced by the
        // CoUninitialize below once every COM object of `recycle` is gone.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }
            .ok()
            .map_err(to_io)?;
        let result = recycle(&path);
        // SAFETY: matches the successful CoInitializeEx above; `recycle` has
        // returned, so no COM object of this thread is alive.
        unsafe { CoUninitialize() };
        result
    })
    .join()
    .map_err(|_| io::Error::other("the Recycle Bin thread panicked"))?
}

/// After undoing: the item is back in place, so its `$I…` record goes.
pub fn forget(trashed_to: &Path) -> io::Result<()> {
    let Some(name) = trashed_to.file_name().and_then(|n| n.to_str()) else {
        return Ok(());
    };
    let Some(rest) = name.strip_prefix("$R") else {
        return Ok(());
    };
    match std::fs::remove_file(trashed_to.with_file_name(format!("$I{rest}"))) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// The shell parses plain absolute paths: no `\\?\` prefix, `\` only.
fn shell_path(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    PathBuf::from(text.replace('/', "\\"))
}

#[derive(Default)]
struct Outcome {
    /// Where the item went.
    recycled_to: Option<PathBuf>,
    /// The shell would have deleted it for good: refused.
    refused: bool,
}

fn recycle(path: &Path) -> io::Result<PathBuf> {
    let outcome = Rc::new(RefCell::new(Outcome::default()));
    let sink: IFileOperationProgressSink = Sink {
        source: path.to_path_buf(),
        outcome: Rc::clone(&outcome),
    }
    .into();
    let wide = HSTRING::from(path.as_os_str());
    // SAFETY: COM is initialised on this thread (see `trash`); every argument
    // is a valid interface or a NUL-terminated string that outlives the
    // call, and the interfaces are released when dropped.
    let performed = unsafe {
        let operation: IFileOperation =
            CoCreateInstance(&FileOperation, None, CLSCTX_ALL).map_err(to_io)?;
        operation
            .SetOperationFlags(FOF_NO_UI | FOF_ALLOWUNDO | FOFX_RECYCLEONDELETE | FOFX_EARLYFAILURE)
            .map_err(to_io)?;
        let item: IShellItem = SHCreateItemFromParsingName(&wide, None).map_err(to_io)?;
        operation.DeleteItem(&item, &sink).map_err(to_io)?;
        operation.PerformOperations()
    };
    let outcome = outcome.take();
    if outcome.refused {
        return Err(io::Error::other(format!(
            "the Recycle Bin cannot take {} (nothing was deleted)",
            path.display()
        )));
    }
    performed.map_err(to_io)?;
    match outcome.recycled_to {
        Some(to) => Ok(to),
        None if path.exists() => Err(io::Error::other(format!(
            "{} was not moved to the Recycle Bin",
            path.display()
        ))),
        None => Err(io::Error::other(format!(
            "{} went to the Recycle Bin, but Windows did not say where",
            path.display()
        ))),
    }
}

fn to_io(e: windows::core::Error) -> io::Error {
    io::Error::other(e.message())
}

/// The file system path of a shell item.
fn item_path(item: &IShellItem) -> Option<PathBuf> {
    // SAFETY: GetDisplayName returns a NUL-terminated string allocated with
    // CoTaskMemAlloc that we own: it is copied once, then freed exactly once.
    unsafe {
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = PathBuf::from(OsString::from_wide(name.as_wide()));
        CoTaskMemFree(Some(name.0.cast_const().cast()));
        Some(path)
    }
}

/// Watches the operation: refuses permanent deletion and records where the
/// item went.
#[implement(IFileOperationProgressSink)]
struct Sink {
    source: PathBuf,
    outcome: Rc<RefCell<Outcome>>,
}

impl IFileOperationProgressSink_Impl for Sink_Impl {
    fn PreDeleteItem(&self, flags: u32, _item: Ref<IShellItem>) -> windows::core::Result<()> {
        // Without this flag the shell is about to delete for good.
        if flags & TSF_DELETE_RECYCLE_IF_POSSIBLE.0.cast_unsigned() == 0 {
            self.outcome.borrow_mut().refused = true;
            return Err(E_ABORT.into());
        }
        Ok(())
    }

    fn PostDeleteItem(
        &self,
        _flags: u32,
        item: Ref<IShellItem>,
        result: HRESULT,
        newly_created: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        let is_source = item
            .as_ref()
            .and_then(item_path)
            .is_some_and(|p| same_path(&p, &self.source));
        if result.is_ok() && is_source {
            self.outcome.borrow_mut().recycled_to = newly_created.as_ref().and_then(item_path);
        }
        Ok(())
    }

    fn StartOperations(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn FinishOperations(&self, _result: HRESULT) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreRenameItem(
        &self,
        _flags: u32,
        _item: Ref<IShellItem>,
        _new_name: &PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostRenameItem(
        &self,
        _flags: u32,
        _item: Ref<IShellItem>,
        _new_name: &PCWSTR,
        _result: HRESULT,
        _created: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreMoveItem(
        &self,
        _flags: u32,
        _item: Ref<IShellItem>,
        _folder: Ref<IShellItem>,
        _new_name: &PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostMoveItem(
        &self,
        _flags: u32,
        _item: Ref<IShellItem>,
        _folder: Ref<IShellItem>,
        _new_name: &PCWSTR,
        _result: HRESULT,
        _created: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreCopyItem(
        &self,
        _flags: u32,
        _item: Ref<IShellItem>,
        _folder: Ref<IShellItem>,
        _new_name: &PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostCopyItem(
        &self,
        _flags: u32,
        _item: Ref<IShellItem>,
        _folder: Ref<IShellItem>,
        _new_name: &PCWSTR,
        _result: HRESULT,
        _created: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PreNewItem(
        &self,
        _flags: u32,
        _folder: Ref<IShellItem>,
        _new_name: &PCWSTR,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn PostNewItem(
        &self,
        _flags: u32,
        _folder: Ref<IShellItem>,
        _new_name: &PCWSTR,
        _template: &PCWSTR,
        _attributes: u32,
        _result: HRESULT,
        _item: Ref<IShellItem>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
    fn UpdateProgress(&self, _total: u32, _so_far: u32) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResetTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn PauseTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
    fn ResumeTimer(&self) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Windows paths compare without regard to case.
fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().to_lowercase().trim_end_matches('\\')
        == b.to_string_lossy().to_lowercase().trim_end_matches('\\')
}
