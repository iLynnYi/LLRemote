use super::{unavailable, SourcePickerError};
use std::cell::RefCell;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::rc::Rc;
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND, S_OK};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, IFileDialog, IFileDialogControlEvents, IFileDialogControlEvents_Impl,
    IFileDialogCustomize, IFileDialogEvents, IFileDialogEvents_Impl, IFileOpenDialog, IShellItem,
    IShellItemArray, CDCS_ENABLED, CDCS_VISIBLE, FDEOR_DEFAULT, FDESVR_DEFAULT,
    FDE_OVERWRITE_RESPONSE, FDE_SHAREVIOLATION_RESPONSE, FOS_ALLOWMULTISELECT, FOS_DONTADDTORECENT,
    FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};
use windows_core::{implement, w, Interface, Ref, Result as WinResult, BOOL, HRESULT, PWSTR};

const UPLOAD_SELECTION: u32 = 1001;

pub(super) fn select(owner: isize) -> Result<Vec<PathBuf>, SourcePickerError> {
    select_native(owner).map_err(|_| unavailable())
}

fn select_native(owner: isize) -> WinResult<Vec<PathBuf>> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }
    let _apartment = Apartment;
    let dialog: IFileOpenDialog =
        unsafe { CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)? };
    let customize: IFileDialogCustomize = dialog.cast()?;
    unsafe {
        let options = dialog.GetOptions()?;
        dialog.SetOptions(
            (options
                | FOS_ALLOWMULTISELECT
                | FOS_FORCEFILESYSTEM
                | FOS_PATHMUSTEXIST
                | FOS_FILEMUSTEXIST
                | FOS_DONTADDTORECENT)
                & !FOS_PICKFOLDERS,
        )?;
        dialog.SetTitle(w!("选择要上传的文件或文件夹"))?;
        // The Shell's Open action navigates folders. A documented custom action
        // confirms GetSelectedItems directly, retaining files AND directories.
        // https://learn.microsoft.com/windows/win32/shell/common-file-dialog
        dialog.SetOkButtonLabel(w!("打开"))?;
        customize.AddPushButton(UPLOAD_SELECTION, w!("上传选中项(&S)"))?;
        customize.MakeProminent(UPLOAD_SELECTION)?;
        customize.SetControlState(UPLOAD_SELECTION, CDCS_VISIBLE)?;
    }
    let selection = Rc::new(RefCell::new(None));
    let events: IFileDialogEvents = DialogEvents {
        selection: Rc::clone(&selection),
    }
    .into();
    let advice = DialogAdvice {
        dialog: dialog.clone(),
        cookie: unsafe { dialog.Advise(&events)? },
    };
    let result = unsafe { dialog.Show(Some(HWND(owner as *mut _))) };
    drop(advice);
    if result
        .as_ref()
        .is_err_and(|error| error.code() == HRESULT::from_win32(ERROR_CANCELLED.0))
    {
        return Ok(Vec::new());
    }
    result?;
    let confirmed = selection.borrow_mut().take();
    match confirmed {
        Some(paths) => Ok(paths),
        None => read_paths(&unsafe { dialog.GetResults()? }),
    }
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

struct DialogAdvice {
    dialog: IFileOpenDialog,
    cookie: u32,
}
impl Drop for DialogAdvice {
    fn drop(&mut self) {
        let _ = unsafe { self.dialog.Unadvise(self.cookie) };
    }
}

struct ShellPath {
    pointer: PWSTR,
}
impl Drop for ShellPath {
    fn drop(&mut self) {
        unsafe {
            CoTaskMemFree(Some(self.pointer.0.cast()));
        }
    }
}

fn read_paths(items: &IShellItemArray) -> WinResult<Vec<PathBuf>> {
    let count = unsafe { items.GetCount()? };
    let mut paths = Vec::with_capacity(count as usize);
    for index in 0..count {
        let item = unsafe { items.GetItemAt(index)? };
        let path = ShellPath {
            pointer: unsafe { item.GetDisplayName(SIGDN_FILESYSPATH)? },
        };
        paths.push(PathBuf::from(std::ffi::OsString::from_wide(unsafe {
            path.pointer.as_wide()
        })));
    }
    Ok(paths)
}

#[implement(IFileDialogEvents, IFileDialogControlEvents)]
struct DialogEvents {
    selection: Rc<RefCell<Option<Vec<PathBuf>>>>,
}

impl DialogEvents_Impl {
    fn update_selection(&self, dialog: &IFileDialog) -> WinResult<()> {
        let open: IFileOpenDialog = dialog.cast()?;
        let customize: IFileDialogCustomize = dialog.cast()?;
        let count = unsafe {
            open.GetSelectedItems()
                .and_then(|items| items.GetCount())
                .unwrap_or(0)
        };
        let state = if count > 0 {
            CDCS_VISIBLE | CDCS_ENABLED
        } else {
            CDCS_VISIBLE
        };
        unsafe { customize.SetControlState(UPLOAD_SELECTION, state) }
    }
}

#[allow(non_snake_case)]
impl IFileDialogEvents_Impl for DialogEvents_Impl {
    fn OnFileOk(&self, dialog: Ref<'_, IFileDialog>) -> WinResult<()> {
        // The ordinary file confirmation must preserve selected directories as
        // well; GetResults alone may contain only the file-mode results.
        let open: IFileOpenDialog = dialog.ok()?.cast()?;
        let paths = read_paths(&unsafe { open.GetSelectedItems()? })?;
        *self.selection.borrow_mut() = Some(paths);
        Ok(())
    }
    fn OnFolderChanging(
        &self,
        _dialog: Ref<'_, IFileDialog>,
        _folder: Ref<'_, IShellItem>,
    ) -> WinResult<()> {
        Ok(())
    }
    fn OnFolderChange(&self, dialog: Ref<'_, IFileDialog>) -> WinResult<()> {
        self.update_selection(dialog.ok()?)
    }
    fn OnSelectionChange(&self, dialog: Ref<'_, IFileDialog>) -> WinResult<()> {
        self.update_selection(dialog.ok()?)
    }
    fn OnTypeChange(&self, _dialog: Ref<'_, IFileDialog>) -> WinResult<()> {
        Ok(())
    }
    fn OnShareViolation(
        &self,
        _dialog: Ref<'_, IFileDialog>,
        _item: Ref<'_, IShellItem>,
    ) -> WinResult<FDE_SHAREVIOLATION_RESPONSE> {
        Ok(FDESVR_DEFAULT)
    }
    fn OnOverwrite(
        &self,
        _dialog: Ref<'_, IFileDialog>,
        _item: Ref<'_, IShellItem>,
    ) -> WinResult<FDE_OVERWRITE_RESPONSE> {
        Ok(FDEOR_DEFAULT)
    }
}

#[allow(non_snake_case)]
impl IFileDialogControlEvents_Impl for DialogEvents_Impl {
    fn OnButtonClicked(&self, customize: Ref<'_, IFileDialogCustomize>, id: u32) -> WinResult<()> {
        if id != UPLOAD_SELECTION {
            return Ok(());
        }
        let dialog: IFileOpenDialog = customize.ok()?.cast()?;
        let paths = match unsafe { dialog.GetSelectedItems() }.and_then(|items| read_paths(&items))
        {
            Ok(paths) => paths,
            Err(error) => {
                // Control callback HRESULTs are ignored by the Shell. End the
                // dialog with an error so the caller can show localized feedback.
                unsafe {
                    dialog.Close(error.code())?;
                }
                return Ok(());
            }
        };
        if paths.is_empty() {
            return Ok(());
        }
        *self.selection.borrow_mut() = Some(paths);
        if let Err(error) = unsafe { dialog.Close(S_OK) } {
            self.selection.borrow_mut().take();
            return Err(error);
        }
        Ok(())
    }
    fn OnItemSelected(
        &self,
        _customize: Ref<'_, IFileDialogCustomize>,
        _id: u32,
        _item: u32,
    ) -> WinResult<()> {
        Ok(())
    }
    fn OnCheckButtonToggled(
        &self,
        _customize: Ref<'_, IFileDialogCustomize>,
        _id: u32,
        _checked: BOOL,
    ) -> WinResult<()> {
        Ok(())
    }
    fn OnControlActivating(
        &self,
        _customize: Ref<'_, IFileDialogCustomize>,
        _id: u32,
    ) -> WinResult<()> {
        Ok(())
    }
}
