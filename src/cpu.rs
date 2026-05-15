/// Exclude CPU 0 from this process's affinity mask.
///
/// iRacing's sim thread is hardcoded to CPU 0. Running on CPU 0
/// causes Type B frame time spikes. This moves our process off
/// CPU 0 entirely so we never compete with the sim thread.
///
/// Reference: https://rcsracing93.github.io/iracing-stutter-fix/
#[cfg(windows)]
pub fn avoid_cpu0() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetProcessAffinityMask, SetProcessAffinityMask,
    };
    unsafe {
        let process = GetCurrentProcess();
        let mut process_mask: usize = 0;
        let mut system_mask: usize = 0;
        if GetProcessAffinityMask(process, &mut process_mask, &mut system_mask) != 0 {
            let new_mask = process_mask & !1usize;
            if new_mask != 0 && SetProcessAffinityMask(process, new_mask) != 0 {
                eprintln!("[cpu] excluded CPU 0 (iRacing sim thread protection)");
            }
        }
    }
}

#[cfg(not(windows))]
pub fn avoid_cpu0() {}
