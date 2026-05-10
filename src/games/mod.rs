pub mod ac;
pub mod iracing;

#[derive(Debug, PartialEq)]
pub struct CarDetected {
    pub game: String,
    pub car: String,
}

/// Polls each game in priority order: iRacing → AC EVO → AC1/ACC.
/// Returns the first active session found.
pub fn detect_car() -> Option<CarDetected> {
    #[cfg(windows)]
    {
        if let Some(name) = iracing::car_name() {
            return Some(CarDetected {
                game: "iRacing".to_string(),
                car: name,
            });
        }
        if let Some((variant, name)) = ac::car_name() {
            let game = match variant {
                ac::AcVariant::Evo => "Assetto Corsa EVO",
                ac::AcVariant::Ac1 => "Assetto Corsa",
            };
            return Some(CarDetected {
                game: game.to_string(),
                car: name,
            });
        }
    }
    None
}

// ---------------------------------------------------------------------------
// SharedMem — RAII wrapper around an OpenFileMappingW / MapViewOfFile pair.
// Used by iracing.rs and ac.rs.
// ---------------------------------------------------------------------------

#[cfg(windows)]
pub(crate) struct SharedMem {
    h_map: windows_sys::Win32::Foundation::HANDLE,
    view: windows_sys::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS,
    size: usize,
}

#[cfg(windows)]
impl SharedMem {
    pub fn open(name: &str) -> Option<Self> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Memory::{
            MapViewOfFile, OpenFileMappingW, VirtualQuery, FILE_MAP_READ, MEMORY_BASIC_INFORMATION,
        };

        let wide: Vec<u16> = OsStr::new(name).encode_wide().chain(Some(0)).collect();
        let h = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, wide.as_ptr()) };
        if h == 0 {
            return None;
        }

        let view = unsafe { MapViewOfFile(h, FILE_MAP_READ, 0, 0, 0) };
        if view.Value.is_null() {
            unsafe { CloseHandle(h) };
            return None;
        }

        let mut mbi = unsafe { std::mem::zeroed::<MEMORY_BASIC_INFORMATION>() };
        let qret = unsafe {
            VirtualQuery(
                view.Value as *const core::ffi::c_void,
                &mut mbi,
                std::mem::size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        let size = if qret > 0 { mbi.RegionSize } else { 0 };

        Some(SharedMem {
            h_map: h,
            view,
            size,
        })
    }

    pub fn bytes(&self) -> &[u8] {
        if self.size == 0 {
            return &[];
        }
        unsafe { std::slice::from_raw_parts(self.view.Value as *const u8, self.size) }
    }
}

#[cfg(windows)]
impl Drop for SharedMem {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Memory::UnmapViewOfFile;
        unsafe {
            UnmapViewOfFile(self.view);
            CloseHandle(self.h_map);
        }
    }
}

#[cfg(windows)]
unsafe impl Send for SharedMem {}
