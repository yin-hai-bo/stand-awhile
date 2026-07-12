use std::{fs, path::PathBuf, ptr::copy_nonoverlapping};

use windows::{
    Data::Xml::Dom::XmlDocument,
    UI::Notifications::{ToastNotification, ToastNotificationManager},
    Win32::{
        Foundation::E_OUTOFMEMORY,
        Storage::EnhancedStorage::PKEY_AppUserModel_ID,
        System::{
            Com::StructuredStorage::{
                PROPVARIANT, PROPVARIANT_0, PROPVARIANT_0_0, PROPVARIANT_0_0_0, PropVariantClear,
            },
            Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemAlloc, CoTaskMemFree, IPersistFile},
            Variant::VT_LPWSTR,
            WinRT::{RO_INIT_SINGLETHREADED, RoInitialize},
        },
        UI::Shell::{
            FOLDERID_Programs, IShellLinkW, KF_FLAG_DEFAULT, PropertiesSystem::IPropertyStore, SHGetKnownFolderPath,
            SetCurrentProcessExplicitAppUserModelID, ShellLink,
        },
    },
    core::{Error, HSTRING, Interface, PWSTR, Result},
};

const APP_USER_MODEL_ID: &str = "YHB.StandAwhile";
const SHORTCUT_NAME: &str = "Stand Awhile.lnk";

pub fn initialize() -> Result<()> {
    unsafe {
        RoInitialize(RO_INIT_SINGLETHREADED)?;
        SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(APP_USER_MODEL_ID))?;
    }

    ensure_start_menu_shortcut()
}

pub fn show(title: &str, message: &str) -> Result<()> {
    let xml = XmlDocument::new()?;
    xml.LoadXml(&HSTRING::from(build_toast_xml(title, message)))?;

    let toast = ToastNotification::CreateToastNotification(&xml)?;
    let notifier = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(APP_USER_MODEL_ID))?;
    notifier.Show(&toast)
}

fn ensure_start_menu_shortcut() -> Result<()> {
    let shortcut_path = shortcut_path()?;
    if let Some(parent) = shortcut_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| Error::new(windows::core::HRESULT(0x8000_4005u32 as i32), error.to_string()))?;
    }

    let executable_path = std::env::current_exe()
        .map_err(|error| Error::new(windows::core::HRESULT(0x8000_4005u32 as i32), error.to_string()))?;
    let working_directory = executable_path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let shell_link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)? };
    let executable_path_wide = wide_null(executable_path.to_string_lossy().as_ref());
    let working_directory_wide = wide_null(working_directory.to_string_lossy().as_ref());
    let description_wide = wide_null("Stand Awhile");

    unsafe {
        shell_link.SetPath(windows::core::PCWSTR(executable_path_wide.as_ptr()))?;
        shell_link.SetWorkingDirectory(windows::core::PCWSTR(working_directory_wide.as_ptr()))?;
        shell_link.SetDescription(windows::core::PCWSTR(description_wide.as_ptr()))?;
        shell_link.SetIconLocation(windows::core::PCWSTR(executable_path_wide.as_ptr()), 0)?;
    }

    let property_store: IPropertyStore = shell_link.cast()?;
    let mut app_id_variant = propvariant_from_string(APP_USER_MODEL_ID)?;
    unsafe {
        property_store.SetValue(&PKEY_AppUserModel_ID, &app_id_variant)?;
        property_store.Commit()?;
        PropVariantClear(&mut app_id_variant)?;
    }

    let persist_file: IPersistFile = shell_link.cast()?;
    let shortcut_path_wide = wide_null(shortcut_path.to_string_lossy().as_ref());
    unsafe {
        persist_file.Save(windows::core::PCWSTR(shortcut_path_wide.as_ptr()), true)?;
    }

    Ok(())
}

fn shortcut_path() -> Result<PathBuf> {
    let programs_path = unsafe { SHGetKnownFolderPath(&FOLDERID_Programs, KF_FLAG_DEFAULT, None)? };
    let path = pwstr_to_pathbuf(programs_path);
    unsafe {
        CoTaskMemFree(Some(programs_path.0.cast()));
    }
    Ok(path.join(SHORTCUT_NAME))
}

fn pwstr_to_pathbuf(pwstr: PWSTR) -> PathBuf {
    let mut len = 0usize;
    unsafe {
        while *pwstr.0.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(pwstr.0, len);
        PathBuf::from(String::from_utf16_lossy(slice))
    }
}

fn propvariant_from_string(value: &str) -> Result<PROPVARIANT> {
    let wide_value = wide_null(value);
    let byte_len = wide_value.len() * std::mem::size_of::<u16>();
    let raw_ptr = unsafe { CoTaskMemAlloc(byte_len) } as *mut u16;
    if raw_ptr.is_null() {
        return Err(Error::from(E_OUTOFMEMORY));
    }

    unsafe {
        copy_nonoverlapping(wide_value.as_ptr(), raw_ptr, wide_value.len());
    }

    Ok(PROPVARIANT {
        Anonymous: PROPVARIANT_0 {
            Anonymous: std::mem::ManuallyDrop::new(PROPVARIANT_0_0 {
                vt: VT_LPWSTR,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: PROPVARIANT_0_0_0 {
                    pwszVal: PWSTR(raw_ptr),
                },
            }),
        },
    })
}

fn build_toast_xml(title: &str, message: &str) -> String {
    format!(
        "<toast><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual></toast>",
        escape_xml(title),
        escape_xml(message)
    )
}

fn escape_xml(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

#[cfg(test)]
mod tests {
    use super::escape_xml;

    #[test]
    fn escapes_xml_special_characters() {
        assert_eq!(escape_xml("<a&\"b'>"), "&lt;a&amp;&quot;b&apos;&gt;");
    }
}
