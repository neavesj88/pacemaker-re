//! The audio device behind the live mix player: the default output, opened through WASAPI in
//! shared mode with Windows converting from the engine's 44.1 kHz float stereo to whatever the
//! device runs at. A thread waits for the device's buffer event and copies queued frames out of
//! the [`Ring`]; it does no DSP, no allocation and takes no locks while playing.
//!
//! cpal would normally do this, but it is not in the offline crate registry (checked 14 Nov
//! 2026), so this talks to WASAPI directly through the `windows` crate that tauri already pulls
//! in. Other platforms get an error and no sound.

use std::sync::Arc;

/// Called from the device thread when the device goes away or fails mid-play.
pub type OnError = Arc<dyn Fn(String) + Send + Sync>;

#[cfg(windows)]
pub use win::Device;

#[cfg(not(windows))]
use pm_mixer::ring::Ring;

#[cfg(not(windows))]
pub struct Device;

#[cfg(not(windows))]
impl Device {
    pub fn open(_ring: Arc<Ring>, _on_error: OnError) -> Result<Device, String> {
        Err("Live mix playback needs Windows audio in this build.".into())
    }
}

#[cfg(windows)]
mod win {
    use std::sync::atomic::{AtomicBool, Ordering::*};
    use std::sync::mpsc::{channel, Sender};
    use std::sync::Arc;
    use std::thread::JoinHandle;

    use pm_mixer::ring::{Consumer, Ring};
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioClient, IAudioRenderClient, IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, WAVEFORMATEX,
    };
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED};
    use windows::Win32::System::Threading::{CreateEventW, GetCurrentThread, SetThreadPriority, WaitForSingleObject, THREAD_PRIORITY_HIGHEST};

    use super::OnError;

    /// Length of the device's own buffer (the engine keeps 250 ms ahead of it in the ring).
    const BUFFER_HNS: i64 = 400_000; // 40 ms

    pub struct Device {
        stop: Arc<AtomicBool>,
        join: Option<JoinHandle<()>>,
    }

    impl Device {
        /// Opens the default output and starts reading `ring`. Returns once the device runs (or
        /// has failed to).
        pub fn open(ring: Arc<Ring>, on_error: OnError) -> Result<Device, String> {
            let stop = Arc::new(AtomicBool::new(false));
            let started = Arc::new(AtomicBool::new(false));
            let (tx, rx) = channel::<Result<(), String>>();
            let (stop2, started2) = (stop.clone(), started.clone());
            let join = std::thread::Builder::new()
                .name("mix-live-audio".into())
                .spawn(move || {
                    // SAFETY: plain WASAPI calls on this thread; the COM objects never leave it
                    let r = unsafe { run(&ring, &stop2, &started2, &tx) };
                    if let Err(e) = r {
                        // before the device ran the error is the answer to `open`, afterwards it goes to the player
                        if started2.load(Acquire) {
                            on_error(e);
                        } else {
                            let _ = tx.send(Err(e));
                        }
                    }
                })
                .map_err(|e| e.to_string())?;
            match rx.recv() {
                Ok(Ok(())) => Ok(Device { stop, join: Some(join) }),
                Ok(Err(e)) => {
                    let _ = join.join();
                    Err(e)
                }
                Err(_) => Err("The audio thread ended before it started.".into()),
            }
        }
    }

    impl Drop for Device {
        fn drop(&mut self) {
            self.stop.store(true, Release);
            if let Some(j) = self.join.take() {
                let _ = j.join();
            }
        }
    }

    fn msg(what: &str, e: windows::core::Error) -> String {
        format!("Audio output: {what} failed ({})", e.message().trim())
    }

    unsafe fn run(ring: &Arc<Ring>, stop: &AtomicBool, started: &AtomicBool, ready: &Sender<Result<(), String>>) -> Result<(), String> {
        unsafe {
            let com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
            let out = stream(ring, stop, started, ready);
            if com {
                CoUninitialize();
            }
            out
        }
    }

    unsafe fn stream(ring: &Arc<Ring>, stop: &AtomicBool, started: &AtomicBool, ready: &Sender<Result<(), String>>) -> Result<(), String> {
        unsafe {
            let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| msg("listing the devices", e))?;
            let dev = en.GetDefaultAudioEndpoint(eRender, eConsole).map_err(|_| "No audio output device was found.".to_string())?;
            let client: IAudioClient = dev.Activate(CLSCTX_ALL, None).map_err(|e| msg("opening the device", e))?;
            // 44.1 kHz 32-bit float stereo; Windows converts to the device's own rate and layout
            let fmt = WAVEFORMATEX { wFormatTag: 3, nChannels: 2, nSamplesPerSec: 44_100, nAvgBytesPerSec: 44_100 * 8, nBlockAlign: 8, wBitsPerSample: 32, cbSize: 0 };
            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_EVENTCALLBACK | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                    BUFFER_HNS,
                    0,
                    &fmt,
                    None,
                )
                .map_err(|e| msg("setting up the stream", e))?;
            let event: HANDLE = CreateEventW(None, false, false, None).map_err(|e| msg("creating the buffer event", e))?;
            let result = (|| -> Result<(), String> {
                client.SetEventHandle(event).map_err(|e| msg("the buffer event", e))?;
                let render: IAudioRenderClient = client.GetService().map_err(|e| msg("the render service", e))?;
                let size = client.GetBufferSize().map_err(|e| msg("the buffer size", e))?;
                let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
                let mut st = Consumer::default();
                // the first buffer is silence: the ring holds nothing yet
                let p = render.GetBuffer(size).map_err(|e| msg("the first buffer", e))?;
                std::ptr::write_bytes(p, 0, size as usize * 8);
                render.ReleaseBuffer(size, 0).map_err(|e| msg("the first buffer", e))?;
                client.Start().map_err(|e| msg("starting", e))?;
                started.store(true, Release);
                let _ = ready.send(Ok(()));
                while !stop.load(Acquire) {
                    // 100 ms at most, so the stop flag is looked at even when the device stalls
                    WaitForSingleObject(event, 100);
                    let pad = client.GetCurrentPadding().map_err(|e| msg("reading the buffer (was the device removed?)", e))?;
                    let free = size.saturating_sub(pad);
                    if free == 0 {
                        continue;
                    }
                    let p = render.GetBuffer(free).map_err(|e| msg("getting a buffer", e))?;
                    let out = std::slice::from_raw_parts_mut(p as *mut f32, free as usize * 2);
                    ring.fill(out, &mut st);
                    render.ReleaseBuffer(free, 0).map_err(|e| msg("releasing a buffer", e))?;
                }
                let _ = client.Stop();
                Ok(())
            })();
            let _ = CloseHandle(event);
            result
        }
    }
}
