// Gives the Windows .exe its icon and file details. If the resource compiler
// (windres) can't be found, the build still works, just without the icon.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

include!("src/icon.rs");

/// An .ico file with uncompressed 32-bit images (no extra crates needed).
fn make_ico(sizes: &[u32]) -> Vec<u8> {
    let mut images: Vec<Vec<u8>> = Vec::new();
    for &s in sizes {
        let rgba = icon_rgba(s);
        let mask_row = ((s + 31) / 32 * 4) as usize;
        let mut dib = Vec::new();
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&(s as i32).to_le_bytes());
        dib.extend_from_slice(&((s * 2) as i32).to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&32u16.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());
        dib.extend_from_slice(&((s * s * 4) as u32 + (mask_row as u32 * s)).to_le_bytes());
        dib.extend_from_slice(&[0u8; 16]);
        for y in (0..s).rev() {
            for x in 0..s {
                let i = ((y * s + x) * 4) as usize;
                dib.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
            }
        }
        dib.extend(std::iter::repeat(0u8).take(mask_row * s as usize));
        images.push(dib);
    }
    let mut ico = Vec::new();
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (s, img) in sizes.iter().zip(&images) {
        let b = if *s >= 256 { 0 } else { *s as u8 };
        ico.extend_from_slice(&[b, b, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes());
        ico.extend_from_slice(&32u16.to_le_bytes());
        ico.extend_from_slice(&(img.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += img.len() as u32;
    }
    for img in images {
        ico.extend_from_slice(&img);
    }
    ico
}

fn find_windres() -> Option<PathBuf> {
    if let Some(w) = env::var_os("WINDRES") {
        return Some(PathBuf::from(w));
    }
    if let Some(path) = env::var_os("PATH") {
        for dir in env::split_paths(&path) {
            let w = dir.join("windres.exe");
            if w.exists() {
                return Some(w);
            }
        }
    }
    // WinLibs installed with winget
    let base = PathBuf::from(env::var_os("LOCALAPPDATA")?).join("Microsoft").join("WinGet").join("Packages");
    for e in fs::read_dir(base).ok()?.flatten() {
        if e.file_name().to_string_lossy().starts_with("BrechtSanders.WinLibs") {
            let w = e.path().join("mingw64").join("bin").join("windres.exe");
            if w.exists() {
                return Some(w);
            }
        }
    }
    None
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/icon.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") || env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("gnu") {
        return;
    }
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out.join("app.ico"), make_ico(&[16, 24, 32, 48, 64, 128, 256])).unwrap();
    let version = env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "0.1.0".into());
    let v: Vec<&str> = version.split('.').collect();
    let comma = format!("{},{},{},0", v.first().unwrap_or(&"0"), v.get(1).unwrap_or(&"0"), v.get(2).unwrap_or(&"0"));
    let rc = format!(
        r#"1 ICON "app.ico"
1 VERSIONINFO
FILEVERSION {comma}
PRODUCTVERSION {comma}
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040C04B0"
    BEGIN
      VALUE "FileDescription", "TextCraft"
      VALUE "ProductName", "TextCraft"
      VALUE "FileVersion", "{version}"
      VALUE "ProductVersion", "{version}"
      VALUE "OriginalFilename", "TextCraft.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x040C, 1200
  END
END
"#
    );
    fs::write(out.join("app.rc"), rc).unwrap();
    let Some(windres) = find_windres() else {
        println!("cargo:warning=windres introuvable : le .exe n'aura pas d'icône");
        return;
    };
    let obj = out.join("app_res.o");
    let ok = Command::new(&windres)
        .current_dir(&out)
        .args(["--input", "app.rc", "--output", "app_res.o", "--output-format=coff", "--codepage=65001"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok && Path::new(&obj).exists() {
        println!("cargo:rustc-link-arg-bins={}", obj.display());
    } else {
        println!("cargo:warning=windres a échoué : le .exe n'aura pas d'icône");
    }
}
