use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;

#[cfg(windows)]
use windows_sys::Win32::{
    Devices::{
        DeviceAndDriverInstallation::{
            SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
            SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO,
            SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
        },
        HumanInterfaceDevice::{
            HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetFeature, HidD_GetHidGuid,
            HidD_GetPreparsedData, HidD_GetProductString, HidD_SetFeature, HidD_SetOutputReport,
            HidP_GetCaps, HIDD_ATTRIBUTES, HIDP_CAPS, HIDP_STATUS_SUCCESS, PHIDP_PREPARSED_DATA,
        },
    },
    Foundation::{
        CloseHandle, GetLastError, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
    },
    Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, FILE_FLAG_OVERLAPPED, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING,
    },
    System::Threading::{CreateEventW, WaitForSingleObject},
    System::IO::{CancelIo, GetOverlappedResult, OVERLAPPED},
};

pub const FANATEC_VID: u16 = 0x0EB7;
pub const REPORT_SIZE: usize = 64;

pub struct HidCaps {
    pub input_report_len: u16,
    pub output_report_len: u16,
    pub feature_report_len: u16,
}

pub struct FanatecDevice {
    pub handle: HANDLE,
    pub product_id: u16,
    pub product_name: String,
    pub device_path: String,
}

#[cfg(windows)]
impl Drop for FanatecDevice {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.handle) };
    }
}

#[cfg(not(windows))]
impl Drop for FanatecDevice {
    fn drop(&mut self) {}
}

#[derive(Debug)]
#[allow(dead_code)]
pub enum HidError {
    EnumerationFailed(u32),
    OpenFailed(u32),
    WriteFailed(u32),
    ReadFailed(u32),
    Timeout,
}

impl std::fmt::Display for HidError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HidError::EnumerationFailed(e) => write!(f, "enumeration failed (error {})", e),
            HidError::OpenFailed(e) => write!(f, "open failed (error {})", e),
            HidError::WriteFailed(e) => write!(f, "write failed (error {})", e),
            HidError::ReadFailed(e) => write!(f, "read failed (error {})", e),
            HidError::Timeout => write!(f, "read timed out"),
        }
    }
}

/// Enumerates all HID devices with Fanatec's vendor ID.
/// Returns device paths for matching devices without opening them.
#[cfg(windows)]
pub fn enumerate_fanatec() -> Result<Vec<FanatecDevice>, HidError> {
    use std::mem::size_of;

    let mut devices = Vec::new();

    unsafe {
        let mut hid_guid = std::mem::zeroed();
        HidD_GetHidGuid(&mut hid_guid);

        let dev_info = SetupDiGetClassDevsW(
            &hid_guid,
            std::ptr::null(),
            0,
            DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
        );
        if dev_info == INVALID_HANDLE_VALUE as HDEVINFO {
            return Err(HidError::EnumerationFailed(GetLastError()));
        }

        let mut iface_data: SP_DEVICE_INTERFACE_DATA = std::mem::zeroed();
        iface_data.cbSize = size_of::<SP_DEVICE_INTERFACE_DATA>() as u32;

        let mut index = 0u32;
        loop {
            let ok = SetupDiEnumDeviceInterfaces(
                dev_info,
                std::ptr::null_mut(),
                &hid_guid,
                index,
                &mut iface_data,
            );
            if ok == 0 {
                break; // no more interfaces
            }
            index += 1;

            // First call: get required buffer size
            let mut required = 0u32;
            SetupDiGetDeviceInterfaceDetailW(
                dev_info,
                &iface_data,
                std::ptr::null_mut(),
                0,
                &mut required,
                std::ptr::null_mut(),
            );
            if required == 0 {
                continue;
            }

            // Second call: get the device path
            let mut buf = vec![0u8; required as usize];
            let detail = buf.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;
            (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;

            let ok = SetupDiGetDeviceInterfaceDetailW(
                dev_info,
                &iface_data,
                detail,
                required,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if ok == 0 {
                continue;
            }

            // DevicePath starts at offset 4 (after cbSize u32)
            let path_ptr = buf.as_ptr().add(4) as *const u16;
            let path_len = (0..).take_while(|&i| *path_ptr.add(i) != 0).count();
            let path_slice = std::slice::from_raw_parts(path_ptr, path_len);
            let path_os = OsString::from_wide(path_slice);
            let path_str = path_os.to_string_lossy().into_owned();

            // Open with shared access so FanaLab can coexist
            let path_wide: Vec<u16> = path_slice
                .iter()
                .copied()
                .chain(std::iter::once(0))
                .collect();
            let handle = CreateFileW(
                path_wide.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                0,
            );
            if handle == INVALID_HANDLE_VALUE {
                continue;
            }

            // Check vendor ID
            let mut attrs: HIDD_ATTRIBUTES = std::mem::zeroed();
            attrs.Size = size_of::<HIDD_ATTRIBUTES>() as u32;
            if HidD_GetAttributes(handle, &mut attrs) == 0 {
                CloseHandle(handle);
                continue;
            }
            if attrs.VendorID != FANATEC_VID {
                CloseHandle(handle);
                continue;
            }

            // Read product string (max 126 wide chars + null)
            let mut name_buf = [0u16; 128];
            let _ = HidD_GetProductString(
                handle,
                name_buf.as_mut_ptr() as *mut _,
                (name_buf.len() * 2) as u32,
            );
            let name_len = name_buf.iter().take_while(|&&c| c != 0).count();
            let product_name = OsString::from_wide(&name_buf[..name_len])
                .to_string_lossy()
                .into_owned();

            devices.push(FanatecDevice {
                handle,
                product_id: attrs.ProductID,
                product_name,
                device_path: path_str,
            });
        }

        SetupDiDestroyDeviceInfoList(dev_info);
    }

    Ok(devices)
}

#[cfg(not(windows))]
pub fn enumerate_fanatec() -> Result<Vec<FanatecDevice>, HidError> {
    eprintln!("HID enumeration is only supported on Windows.");
    Ok(vec![])
}

/// Sends a 64-byte output report. buf[0] must be the report ID (0xFF).
#[cfg(windows)]
pub fn write_report(device: &FanatecDevice, buf: &[u8; REPORT_SIZE]) -> Result<(), HidError> {
    unsafe {
        let mut overlapped: OVERLAPPED = std::mem::zeroed();
        let mut written = 0u32;

        let ok = WriteFile(
            device.handle,
            buf.as_ptr(),
            REPORT_SIZE as u32,
            &mut written,
            &mut overlapped,
        );

        if ok == 0 {
            let err = GetLastError();
            const ERROR_IO_PENDING: u32 = 997;
            if err != ERROR_IO_PENDING {
                return Err(HidError::WriteFailed(err));
            }
            // Wait for completion
            if GetOverlappedResult(device.handle, &overlapped, &mut written, 1) == 0 {
                return Err(HidError::WriteFailed(GetLastError()));
            }
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn write_report(_device: &FanatecDevice, _buf: &[u8; REPORT_SIZE]) -> Result<(), HidError> {
    Err(HidError::WriteFailed(0))
}

/// Reads one interrupt IN report with a timeout. Returns the number of bytes read.
/// Retries until a report with the expected header is found or timeout_ms elapses.
#[cfg(windows)]
pub fn read_report(
    device: &FanatecDevice,
    buf: &mut [u8; REPORT_SIZE],
    timeout_ms: u32,
) -> Result<(), HidError> {
    unsafe {
        let event = CreateEventW(
            std::ptr::null(),
            1, // manual reset
            0,
            std::ptr::null(),
        );
        if event == 0 {
            return Err(HidError::ReadFailed(GetLastError()));
        }
        let _guard = EventGuard(event);

        let mut overlapped: OVERLAPPED = std::mem::zeroed();
        overlapped.hEvent = event;

        let mut read = 0u32;
        let ok = ReadFile(
            device.handle,
            buf.as_mut_ptr(),
            REPORT_SIZE as u32,
            &mut read,
            &mut overlapped,
        );

        if ok == 0 {
            let err = GetLastError();
            const ERROR_IO_PENDING: u32 = 997;
            if err != ERROR_IO_PENDING {
                return Err(HidError::ReadFailed(err));
            }
        }

        const WAIT_TIMEOUT: u32 = 0x00000102;
        const WAIT_OBJECT_0: u32 = 0x00000000;

        let wait = WaitForSingleObject(event, timeout_ms);
        if wait == WAIT_TIMEOUT {
            // Cancel pending I/O
            CancelIo(device.handle);
            return Err(HidError::Timeout);
        }
        if wait != WAIT_OBJECT_0 {
            return Err(HidError::ReadFailed(GetLastError()));
        }

        if GetOverlappedResult(device.handle, &overlapped, &mut read, 0) == 0 {
            return Err(HidError::ReadFailed(GetLastError()));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn read_report(
    _device: &FanatecDevice,
    _buf: &mut [u8; REPORT_SIZE],
    _timeout_ms: u32,
) -> Result<(), HidError> {
    Err(HidError::ReadFailed(0))
}

#[cfg(windows)]
struct EventGuard(HANDLE);

#[cfg(windows)]
impl Drop for EventGuard {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

// ---------------------------------------------------------------------------
// Diagnostic helpers
// ---------------------------------------------------------------------------

/// Returns the HID report byte lengths for a device, or None if unavailable.
#[cfg(windows)]
pub fn get_hid_caps(device: &FanatecDevice) -> Option<HidCaps> {
    unsafe {
        let mut preparsed: PHIDP_PREPARSED_DATA = 0;
        if HidD_GetPreparsedData(device.handle, &mut preparsed) == 0 {
            return None;
        }
        let mut caps: HIDP_CAPS = std::mem::zeroed();
        let status = HidP_GetCaps(preparsed, &mut caps);
        HidD_FreePreparsedData(preparsed);
        if status != HIDP_STATUS_SUCCESS {
            return None;
        }
        Some(HidCaps {
            input_report_len: caps.InputReportByteLength,
            output_report_len: caps.OutputReportByteLength,
            feature_report_len: caps.FeatureReportByteLength,
        })
    }
}

#[cfg(not(windows))]
pub fn get_hid_caps(_device: &FanatecDevice) -> Option<HidCaps> {
    None
}

/// WriteFile with an arbitrary-length byte slice (used by the diag command).
#[cfg(windows)]
pub fn write_raw(device: &FanatecDevice, buf: &[u8]) -> Result<(), HidError> {
    unsafe {
        let mut overlapped: OVERLAPPED = std::mem::zeroed();
        let mut written = 0u32;
        let ok = WriteFile(
            device.handle,
            buf.as_ptr(),
            buf.len() as u32,
            &mut written,
            &mut overlapped,
        );
        if ok == 0 {
            let err = GetLastError();
            const ERROR_IO_PENDING: u32 = 997;
            if err != ERROR_IO_PENDING {
                return Err(HidError::WriteFailed(err));
            }
            if GetOverlappedResult(device.handle, &overlapped, &mut written, 1) == 0 {
                return Err(HidError::WriteFailed(GetLastError()));
            }
        }
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn write_raw(_device: &FanatecDevice, _buf: &[u8]) -> Result<(), HidError> {
    Err(HidError::WriteFailed(0))
}

/// ReadFile with an arbitrary-length buffer (used by the diag command).
#[cfg(windows)]
pub fn read_raw(device: &FanatecDevice, buf: &mut [u8], timeout_ms: u32) -> Result<(), HidError> {
    unsafe {
        let event = CreateEventW(std::ptr::null(), 1, 0, std::ptr::null());
        if event == 0 {
            return Err(HidError::ReadFailed(GetLastError()));
        }
        let _guard = EventGuard(event);
        let mut overlapped: OVERLAPPED = std::mem::zeroed();
        overlapped.hEvent = event;
        let mut read = 0u32;
        let ok = ReadFile(
            device.handle,
            buf.as_mut_ptr(),
            buf.len() as u32,
            &mut read,
            &mut overlapped,
        );
        if ok == 0 {
            let err = GetLastError();
            const ERROR_IO_PENDING: u32 = 997;
            if err != ERROR_IO_PENDING {
                return Err(HidError::ReadFailed(err));
            }
        }
        const WAIT_TIMEOUT: u32 = 0x0000_0102;
        const WAIT_OBJECT_0: u32 = 0x0000_0000;
        let wait = WaitForSingleObject(event, timeout_ms);
        if wait == WAIT_TIMEOUT {
            CancelIo(device.handle);
            return Err(HidError::Timeout);
        }
        if wait != WAIT_OBJECT_0 {
            return Err(HidError::ReadFailed(GetLastError()));
        }
        if GetOverlappedResult(device.handle, &overlapped, &mut read, 0) == 0 {
            return Err(HidError::ReadFailed(GetLastError()));
        }
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn read_raw(
    _device: &FanatecDevice,
    _buf: &mut [u8],
    _timeout_ms: u32,
) -> Result<(), HidError> {
    Err(HidError::ReadFailed(0))
}

/// HidD_SetFeature — sends a feature report (byte 0 = report ID).
#[cfg(windows)]
pub fn set_feature(device: &FanatecDevice, buf: &[u8]) -> Result<(), HidError> {
    unsafe {
        if HidD_SetFeature(device.handle, buf.as_ptr().cast(), buf.len() as u32) != 0 {
            Ok(())
        } else {
            Err(HidError::WriteFailed(GetLastError()))
        }
    }
}

#[cfg(not(windows))]
pub fn set_feature(_device: &FanatecDevice, _buf: &[u8]) -> Result<(), HidError> {
    Err(HidError::WriteFailed(0))
}

/// HidD_GetFeature — receives a feature report (byte 0 = report ID).
#[cfg(windows)]
pub fn get_feature(device: &FanatecDevice, buf: &mut [u8]) -> Result<(), HidError> {
    unsafe {
        if HidD_GetFeature(device.handle, buf.as_mut_ptr().cast(), buf.len() as u32) != 0 {
            Ok(())
        } else {
            Err(HidError::ReadFailed(GetLastError()))
        }
    }
}

#[cfg(not(windows))]
pub fn get_feature(_device: &FanatecDevice, _buf: &mut [u8]) -> Result<(), HidError> {
    Err(HidError::ReadFailed(0))
}

/// HidD_SetOutputReport — sends an output report via the USB control endpoint
/// (SET_REPORT, type=output). Equivalent to Linux hid_hw_request(HID_REQ_SET_REPORT).
/// Byte 0 of buf must be the report ID.
#[cfg(windows)]
pub fn set_output_report(device: &FanatecDevice, buf: &[u8]) -> Result<(), HidError> {
    unsafe {
        if HidD_SetOutputReport(device.handle, buf.as_ptr().cast(), buf.len() as u32) != 0 {
            Ok(())
        } else {
            Err(HidError::WriteFailed(GetLastError()))
        }
    }
}

#[cfg(not(windows))]
pub fn set_output_report(_device: &FanatecDevice, _buf: &[u8]) -> Result<(), HidError> {
    Err(HidError::WriteFailed(0))
}
