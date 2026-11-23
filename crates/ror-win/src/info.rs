//! What an app looks like to a person: its display name and icon, read from
//! the program file, plus finding that program file in the first place.

use std::collections::{HashMap, HashSet};
use std::ffi::c_void;

use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
    GetObjectW, HGDIOBJ, ReleaseDC,
};
use windows::Win32::Storage::FileSystem::{
    FILE_FLAGS_AND_ATTRIBUTES, GetFileVersionInfoSizeW, GetFileVersionInfoW, SearchPathW,
    VerQueryValueW,
};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Environment::ExpandEnvironmentStringsW;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::UI::Shell::{SHDefExtractIconW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
use windows::core::{HSTRING, PCWSTR, w};

use crate::{file_name, process_path};

/// Expand `%VARIABLES%`.
pub fn expand_env(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let src = HSTRING::from(s);
    // SAFETY: the first call sizes the buffer, the second fills it.
    unsafe {
        let needed = ExpandEnvironmentStringsW(&src, None);
        if needed == 0 {
            return s.to_string();
        }
        let mut buf = vec![0u16; needed as usize];
        let len = ExpandEnvironmentStringsW(&src, Some(&mut buf));
        if len == 0 || len > needed {
            return s.to_string();
        }
        String::from_utf16_lossy(&buf[..len as usize - 1])
    }
}

/// Full path of a program given by name (`wt.exe`, `firefox.exe`): searched
/// on the PATH, then in the "App Paths" registry key Windows uses for Run.
pub fn find_program(name: &str) -> Option<String> {
    let file = HSTRING::from(name);
    let mut buf = [0u16; 1024];
    // SAFETY: the buffer outlives the call.
    let len = unsafe { SearchPathW(PCWSTR::null(), &file, w!(".exe"), Some(&mut buf), None) };
    if len > 0 && (len as usize) < buf.len() {
        return Some(String::from_utf16_lossy(&buf[..len as usize]));
    }
    let key = HSTRING::from(format!(r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\{name}"));
    for root in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        let mut bytes = (buf.len() * 2) as u32;
        // SAFETY: `buf` holds `bytes` bytes.
        let status = unsafe {
            RegGetValueW(root, &key, PCWSTR::null(), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr().cast()), Some(&mut bytes))
        };
        if status == ERROR_SUCCESS {
            let s = String::from_utf16_lossy(&buf[..bytes as usize / 2]);
            let s = s.trim_end_matches('\0').trim().trim_matches('"').to_string();
            if !s.is_empty() {
                return Some(s);
            }
        }
    }
    None
}

/// Full exe paths of running processes with any of these names (matched
/// case-insensitively), keyed by lowercase name. Includes processes with no
/// window, e.g. apps closed to the tray.
pub fn running_process_paths(names: &[String]) -> HashMap<String, String> {
    let wanted: HashSet<String> = names.iter().map(|n| n.to_ascii_lowercase()).collect();
    let mut found = HashMap::new();
    // SAFETY: the snapshot handle is closed before returning.
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return found };
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut entry).is_ok();
        while ok {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_ascii_lowercase();
            if wanted.contains(&name)
                && !found.contains_key(&name)
                && let Some(path) = process_path(entry.th32ProcessID)
            {
                found.insert(name, path);
            }
            ok = Process32NextW(snap, &mut entry).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    found
}

/// The program's own display name ("Visual Studio Code"), from its version
/// info. `None` if it has none.
pub fn file_description(path: &str) -> Option<String> {
    let file = HSTRING::from(path);
    // SAFETY: `data` holds the whole version block; pointers from
    // VerQueryValueW point into it and are read before it is dropped.
    unsafe {
        let size = GetFileVersionInfoSizeW(&file, None);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        GetFileVersionInfoW(&file, None, size, data.as_mut_ptr().cast()).ok()?;

        let query = |sub: &str| -> Option<(*const c_void, u32)> {
            let mut ptr: *mut c_void = std::ptr::null_mut();
            let mut len = 0u32;
            VerQueryValueW(data.as_ptr().cast(), &HSTRING::from(sub), &mut ptr, &mut len)
                .as_bool()
                .then_some((ptr as *const c_void, len))
        };

        // The file's own language first, then US English.
        let mut langs = Vec::new();
        if let Some((ptr, len)) = query(r"\VarFileInfo\Translation")
            && len >= 4
        {
            let pair = ptr as *const u16;
            langs.push(format!("{:04x}{:04x}", *pair, *pair.add(1)));
        }
        langs.extend(["040904b0".to_string(), "040904e4".to_string()]);

        langs.iter().find_map(|lang| {
            let (ptr, len) = query(&format!(r"\StringFileInfo\{lang}\FileDescription"))?;
            let text = std::slice::from_raw_parts(ptr as *const u16, len as usize);
            let text = String::from_utf16_lossy(text);
            let text = text.trim_end_matches('\0').trim();
            (!text.is_empty()).then(|| text.to_string())
        })
    }
}

/// The program's icon as a `size`×`size` PNG (Windows picks the closest
/// size the file has). `None` if it has no usable icon.
pub fn icon_png(path: &str, size: u32) -> Option<Vec<u8>> {
    let file = HSTRING::from(path);
    // SAFETY: the icon is destroyed after conversion.
    unsafe {
        let mut icon = HICON::default();
        let _ = SHDefExtractIconW(&file, 0, 0, Some(&mut icon), None, size);
        if icon.is_invalid() {
            // e.g. app execution aliases (wt.exe): ask the shell instead.
            let mut info = SHFILEINFOW::default();
            SHGetFileInfoW(
                &file,
                FILE_FLAGS_AND_ATTRIBUTES(0),
                Some(&mut info),
                size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_LARGEICON,
            );
            icon = info.hIcon;
        }
        if icon.is_invalid() {
            return None;
        }
        let png = icon_to_png(icon);
        let _ = DestroyIcon(icon);
        png
    }
}

/// Read an icon's pixels (with transparency) and encode them as PNG.
unsafe fn icon_to_png(icon: HICON) -> Option<Vec<u8>> {
    // SAFETY: the bitmaps from GetIconInfo are ours to delete; the DC is released.
    unsafe {
        let mut info = ICONINFO::default();
        GetIconInfo(icon, &mut info).ok()?;
        let color = info.hbmColor;
        let mask = info.hbmMask;
        let free = || {
            let _ = DeleteObject(HGDIOBJ(color.0));
            let _ = DeleteObject(HGDIOBJ(mask.0));
        };
        if color.is_invalid() {
            free(); // monochrome icon; not worth supporting
            return None;
        }

        let mut bm = BITMAP::default();
        GetObjectW(HGDIOBJ(color.0), size_of::<BITMAP>() as i32, Some((&mut bm as *mut BITMAP).cast()));
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        if w <= 0 || h <= 0 || w > 512 || h > 512 {
            free();
            return None;
        }

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let read = |bitmap, out: &mut Vec<u8>, bmi: &mut BITMAPINFO| {
            let dc = GetDC(None);
            let lines = GetDIBits(dc, bitmap, 0, h as u32, Some(out.as_mut_ptr().cast()), bmi, DIB_RGB_COLORS);
            ReleaseDC(None, dc);
            lines == h
        };
        let mut px = vec![0u8; (w * h * 4) as usize];
        if !read(color, &mut px, &mut bmi) {
            free();
            return None;
        }
        // Old icons have no alpha channel; take transparency from the mask.
        if px.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
            let mut m = vec![0u8; px.len()];
            if read(mask, &mut m, &mut bmi) {
                for (p, mp) in px.as_chunks_mut::<4>().0.iter_mut().zip(m.as_chunks::<4>().0) {
                    p[3] = if mp[0] == 0 { 255 } else { 0 };
                }
            }
        }
        free();

        for p in px.as_chunks_mut::<4>().0 {
            p.swap(0, 2); // BGRA -> RGBA
        }
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w as u32, h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(&px).ok()?;
        writer.finish().ok()?;
        Some(out)
    }
}

/// A display name and icon for an app, from the best program file we can
/// find for it.
#[derive(Debug, Clone, Default)]
pub struct AppLook {
    pub name: String,
    /// PNG bytes.
    pub icon: Option<Vec<u8>>,
}

/// Name and icon for an app given its config: `processes` and the launch
/// program (already split from its arguments). `running` maps lowercase
/// process names to exe paths (see [`running_process_paths`]).
pub fn app_look(processes: &[String], launch_program: &str, running: &HashMap<String, String>) -> AppLook {
    let program = expand_env(launch_program);
    let launch_path = if program.contains('\\') {
        std::path::Path::new(&program).exists().then_some(program)
    } else if program.is_empty() {
        None
    } else {
        find_program(&program)
    };
    let process_path = processes.iter().find_map(|p| running.get(&p.to_ascii_lowercase()).cloned());

    // The launch program usually names the app best (steam.exe, not
    // steamwebhelper.exe); a generic launcher like Squirrel's Update.exe doesn't.
    let generic = launch_path.as_deref().is_some_and(|p| file_name(p).eq_ignore_ascii_case("Update.exe"));
    let candidates: Vec<String> = if generic {
        [process_path, launch_path]
    } else {
        [launch_path, process_path]
    }
    .into_iter()
    .flatten()
    .collect();

    let fallback = processes
        .first()
        .map(|p| {
            let stem = p.strip_suffix(".exe").or_else(|| p.strip_suffix(".EXE")).unwrap_or(p);
            let mut c = stem.chars();
            c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
        })
        .unwrap_or_default();
    AppLook {
        name: candidates.iter().find_map(|p| file_description(p)).unwrap_or(fallback),
        icon: candidates.iter().find_map(|p| icon_png(p, 48)),
    }
}
