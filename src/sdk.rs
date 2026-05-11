//! Optional Fanatec SDK FFI — EndorFanatecSdk64_VS2019.dll.
//!
//! # Signature status
//! These signatures are inferred from FanaBridge / SimHub.FanatecManaged.dll
//! usage patterns and common C++ SDK conventions. To verify, decompile
//! SimHub.FanatecManaged.dll (ILSpy / dnSpy) and look for P/Invoke declarations
//! that call FSEnumerateInstance2, FSDeviceQueryInterface, FSTmDataReportRead,
//! FSTmDataSet, FSTmDataSave, FSDeviceRelease.
//!
//! # Param IDs
//! `FSTmDataSet(iface, param_id, value)` — the `param_id` values are currently
//! set to the HID buffer byte-offsets (ADDR_*). Verify against SDK headers or
//! the managed enum in SimHub.FanatecManaged.dll.

use crate::hid::REPORT_SIZE;

// ---------------------------------------------------------------------------
// SDK param IDs (VERIFY THESE against SDK headers / managed enum)
// Current values = HID read-buffer byte offsets (ADDR_* from tuning.rs).
// ---------------------------------------------------------------------------

#[allow(dead_code)]
pub const SDK_PARAM_SEN: i32 = 0x03;
pub const SDK_PARAM_FF: i32 = 0x04;
#[allow(dead_code)]
pub const SDK_PARAM_SHO: i32 = 0x05;
#[allow(dead_code)]
pub const SDK_PARAM_BLI: i32 = 0x06;
#[allow(dead_code)]
pub const SDK_PARAM_FFS: i32 = 0x07;
#[allow(dead_code)]
pub const SDK_PARAM_DRI: i32 = 0x09;
#[allow(dead_code)]
pub const SDK_PARAM_FOR: i32 = 0x0a;
#[allow(dead_code)]
pub const SDK_PARAM_SPR: i32 = 0x0b;
#[allow(dead_code)]
pub const SDK_PARAM_DPR: i32 = 0x0c;
#[allow(dead_code)]
pub const SDK_PARAM_NDP: i32 = 0x0d;
#[allow(dead_code)]
pub const SDK_PARAM_NFR: i32 = 0x0e;
#[allow(dead_code)]
pub const SDK_PARAM_BRF: i32 = 0x10;
#[allow(dead_code)]
pub const SDK_PARAM_FEI: i32 = 0x11;
#[allow(dead_code)]
pub const SDK_PARAM_ACP: i32 = 0x13;
#[allow(dead_code)]
pub const SDK_PARAM_INT: i32 = 0x14;
#[allow(dead_code)]
pub const SDK_PARAM_NIN: i32 = 0x15;
#[allow(dead_code)]
pub const SDK_PARAM_FUL: i32 = 0x16;

// ---------------------------------------------------------------------------
// Windows FFI
// ---------------------------------------------------------------------------

#[cfg(windows)]
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

#[cfg(windows)]
use std::ffi::c_void;

// SIGNATURE UNVERIFIED — adjust if the DLL uses a different layout.
// All 64-bit Windows functions use the Microsoft x64 ABI.
#[cfg(windows)]
mod ffi {
    use std::ffi::c_void;

    // FSEnumerateInstance2(product_id: i32, handle_out: *mut *mut c_void) -> i32
    pub type FnEnumerateInstance2 =
        unsafe extern "system" fn(product_id: i32, handle_out: *mut *mut c_void) -> i32;

    // FSDeviceQueryInterface(handle: *mut c_void, iface_out: *mut *mut c_void) -> i32
    pub type FnDeviceQueryInterface =
        unsafe extern "system" fn(handle: *mut c_void, iface_out: *mut *mut c_void) -> i32;

    // FSTmDataReportRead(iface: *mut c_void, buf: *mut u8, buf_len: i32) -> i32
    pub type FnTmDataReportRead =
        unsafe extern "system" fn(iface: *mut c_void, buf: *mut u8, buf_len: i32) -> i32;

    // FSTmDataSet(iface: *mut c_void, param_id: i32, value: i32) -> i32
    pub type FnTmDataSet =
        unsafe extern "system" fn(iface: *mut c_void, param_id: i32, value: i32) -> i32;

    // FSTmDataSave(iface: *mut c_void) -> i32
    pub type FnTmDataSave = unsafe extern "system" fn(iface: *mut c_void) -> i32;

    // FSDeviceRelease(handle: *mut c_void) -> i32
    pub type FnDeviceRelease = unsafe extern "system" fn(handle: *mut c_void) -> i32;
}

// ---------------------------------------------------------------------------
// SdkLib — loaded DLL + resolved function pointers
// ---------------------------------------------------------------------------

pub struct SdkLib {
    #[cfg(windows)]
    enumerate_instance2: ffi::FnEnumerateInstance2,
    #[cfg(windows)]
    device_query_interface: ffi::FnDeviceQueryInterface,
    #[cfg(windows)]
    tm_data_report_read: ffi::FnTmDataReportRead,
    #[cfg(windows)]
    tm_data_set: ffi::FnTmDataSet,
    #[cfg(windows)]
    tm_data_save: ffi::FnTmDataSave,
    #[cfg(windows)]
    device_release: ffi::FnDeviceRelease,
}

// Function pointers are immutable after init; the DLL stays loaded until process exit.
#[cfg(windows)]
unsafe impl Send for SdkLib {}
#[cfg(windows)]
unsafe impl Sync for SdkLib {}

/// Try to load EndorFanatecSdk64_VS2019.dll from known paths.
/// Returns None (with a warning) if the DLL is not found or is missing expected exports.
pub fn load_sdk() -> Option<SdkLib> {
    #[cfg(windows)]
    {
        let candidates = [
            "EndorFanatecSdk64_VS2019.dll",
            r"C:\Program Files\Fanatec\Fanatec Wheel\fw\EndorFanatecSdk64_VS2019.dll",
        ];

        for path in &candidates {
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let module = unsafe { LoadLibraryW(wide.as_ptr()) };
            if module == 0 {
                continue;
            }
            if let Some(lib) = resolve_all(module) {
                println!("SDK loaded from {}", path);
                return Some(lib);
            }
            // DLL loaded but missing exports; it stays mapped until process exit — acceptable.
        }
        eprintln!(
            "warning: EndorFanatecSdk64_VS2019.dll not found — \
             falling back to raw HID for tuning writes"
        );
        None
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(windows)]
fn resolve_all(module: isize) -> Option<SdkLib> {
    // proc!(name => TargetType) — GetProcAddress + annotated transmute.
    macro_rules! proc {
        ($name:expr => $ty:ty) => {{
            let sym = unsafe { GetProcAddress(module, concat!($name, "\0").as_ptr()) };
            if let Some(f) = sym {
                unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(f) }
            } else {
                eprintln!("SDK: missing export '{}'", $name);
                return None;
            }
        }};
    }

    Some(SdkLib {
        enumerate_instance2: proc!("FSEnumerateInstance2" => ffi::FnEnumerateInstance2),
        device_query_interface: proc!("FSDeviceQueryInterface" => ffi::FnDeviceQueryInterface),
        tm_data_report_read: proc!("FSTmDataReportRead" => ffi::FnTmDataReportRead),
        tm_data_set: proc!("FSTmDataSet" => ffi::FnTmDataSet),
        tm_data_save: proc!("FSTmDataSave" => ffi::FnTmDataSave),
        device_release: proc!("FSDeviceRelease" => ffi::FnDeviceRelease),
    })
}

// ---------------------------------------------------------------------------
// SdkDevice — connected device + tuning interface
// ---------------------------------------------------------------------------

pub struct SdkDevice<'a> {
    #[cfg(windows)]
    lib: &'a SdkLib,
    #[cfg(windows)]
    handle: *mut c_void,
    #[cfg(windows)]
    iface: *mut c_void,
    #[cfg(not(windows))]
    _marker: std::marker::PhantomData<&'a ()>,
}

#[cfg(windows)]
impl Drop for SdkDevice<'_> {
    fn drop(&mut self) {
        unsafe { (self.lib.device_release)(self.handle) };
    }
}

impl SdkLib {
    /// Open the first device with `product_id` and acquire its tuning interface.
    pub fn connect(&self, product_id: u16) -> Result<SdkDevice<'_>, String> {
        #[cfg(windows)]
        unsafe {
            let mut handle: *mut c_void = std::ptr::null_mut();
            let rc = (self.enumerate_instance2)(product_id as i32, &mut handle);
            if rc != 0 || handle.is_null() {
                return Err(format!(
                    "FSEnumerateInstance2(0x{:04X}) failed: rc={}",
                    product_id, rc
                ));
            }
            let mut iface: *mut c_void = std::ptr::null_mut();
            let rc = (self.device_query_interface)(handle, &mut iface);
            if rc != 0 || iface.is_null() {
                (self.device_release)(handle);
                return Err(format!("FSDeviceQueryInterface failed: rc={}", rc));
            }
            Ok(SdkDevice {
                lib: self,
                handle,
                iface,
            })
        }
        #[cfg(not(windows))]
        Err("SDK only available on Windows".to_string())
    }
}

impl SdkDevice<'_> {
    /// Read the current tuning state into a 64-byte buffer.
    /// Buffer layout is expected to match the HID read report.
    pub fn read_tuning(&self) -> Result<[u8; REPORT_SIZE], String> {
        #[cfg(windows)]
        unsafe {
            let mut buf = [0u8; REPORT_SIZE];
            let rc =
                (self.lib.tm_data_report_read)(self.iface, buf.as_mut_ptr(), REPORT_SIZE as i32);
            if rc != 0 {
                return Err(format!("FSTmDataReportRead failed: rc={}", rc));
            }
            Ok(buf)
        }
        #[cfg(not(windows))]
        Err("SDK only available on Windows".to_string())
    }

    /// Set a single tuning parameter. `param_id` is one of `SDK_PARAM_*`.
    /// `value` uses the same wire encoding as the HID protocol.
    pub fn set_param(&self, param_id: i32, value: i32) -> Result<(), String> {
        #[cfg(windows)]
        unsafe {
            let rc = (self.lib.tm_data_set)(self.iface, param_id, value);
            if rc != 0 {
                return Err(format!(
                    "FSTmDataSet(0x{:02X}, {}) failed: rc={}",
                    param_id, value, rc
                ));
            }
            Ok(())
        }
        #[cfg(not(windows))]
        Err("SDK only available on Windows".to_string())
    }

    /// Persist all pending param changes to device flash.
    pub fn save(&self) -> Result<(), String> {
        #[cfg(windows)]
        unsafe {
            let rc = (self.lib.tm_data_save)(self.iface);
            if rc != 0 {
                return Err(format!("FSTmDataSave failed: rc={}", rc));
            }
            Ok(())
        }
        #[cfg(not(windows))]
        Err("SDK only available on Windows".to_string())
    }
}

// ---------------------------------------------------------------------------
// Profile → SDK param set
// ---------------------------------------------------------------------------

/// Apply `params` via FSTmDataSet + FSTmDataSave, then read back the result.
pub fn apply_profile(
    dev: &SdkDevice<'_>,
    params: &[(i32, i32)], // (SDK_PARAM_*, wire_value)
) -> Result<[u8; REPORT_SIZE], String> {
    for &(id, val) in params {
        dev.set_param(id, val)?;
    }
    dev.save()?;
    std::thread::sleep(std::time::Duration::from_millis(200));
    dev.read_tuning()
}
