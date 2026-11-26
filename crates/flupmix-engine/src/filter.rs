//! The mixer node: one `pw_filter` with a stereo pair of DSP input ports per
//! strip slot and a stereo pair of output ports per destination. All mixing
//! happens in a single real-time `process` callback, like a JACK client.
//!
//! pipewire-rs has no safe `pw_filter` wrapper, so the FFI is confined here.

use std::ffi::{c_void, CString};
use std::sync::Arc;

use flupmix_core::mix::{ChannelIn, ChannelOut, Matrix, Mixer, MAX_STRIPS, N_DEST};
use flupmix_core::Shared;
use pipewire::core::Core;
use pipewire::properties::properties;
use pipewire::spa::sys as spa_sys;
use pipewire::sys as pw_sys;

use crate::gains::Controls;
use crate::mirror::{mixer_in_port, mixer_out_port, MIXER_NODE};

/// Everything the real-time callback touches. Owned by [`MixerFilter`] and only
/// ever accessed from the data thread after `connect`.
struct CallbackData {
    mixer: Mixer,
    matrix: Matrix,
    controls: Arc<Controls>,
    shared: Arc<Shared>,
    in_ports: [[*mut c_void; 2]; MAX_STRIPS],
    out_ports: [[*mut c_void; 2]; N_DEST],
}

pub struct MixerFilter {
    filter: *mut pw_sys::pw_filter,
    // Kept alive (and at a stable address) until `pw_filter_destroy`.
    _hook: Box<spa_sys::spa_hook>,
    _events: Box<pw_sys::pw_filter_events>,
    _data: Box<CallbackData>,
}

unsafe extern "C" fn on_process(data: *mut c_void, position: *mut spa_sys::spa_io_position) {
    // SAFETY: `data` is the `CallbackData` registered in `MixerFilter::new`, it
    // outlives the filter, and process is never re-entered.
    let d = unsafe { &mut *(data as *mut CallbackData) };
    let n = unsafe { (*position).clock.duration } as u32;
    if n == 0 {
        return;
    }
    d.controls.load(&mut d.matrix);
    let mono = d.controls.mono_mask();

    let mut ins: [[ChannelIn; 2]; MAX_STRIPS] = Default::default();
    for (s, pair) in d.in_ports.iter().enumerate() {
        for (c, port) in pair.iter().enumerate() {
            if c == 1 && mono & (1 << s) != 0 {
                continue;
            }
            let buf = unsafe { pw_sys::pw_filter_get_dsp_buffer(*port, n) } as *const f32;
            if !buf.is_null() {
                ins[s][c] = Some(unsafe { std::slice::from_raw_parts(buf, n as usize) });
            }
        }
    }
    let mut outs: [[ChannelOut; 2]; N_DEST] = Default::default();
    for (dest, pair) in d.out_ports.iter().enumerate() {
        for (c, port) in pair.iter().enumerate() {
            let buf = unsafe { pw_sys::pw_filter_get_dsp_buffer(*port, n) } as *mut f32;
            if !buf.is_null() {
                outs[dest][c] = Some(unsafe { std::slice::from_raw_parts_mut(buf, n as usize) });
            }
        }
    }
    d.mixer
        .process(&d.matrix, n as usize, &ins, &mut outs, &d.shared.meters);
}

unsafe extern "C" fn on_state_changed(
    _data: *mut c_void,
    _old: pw_sys::pw_filter_state,
    state: pw_sys::pw_filter_state,
    error: *const std::os::raw::c_char,
) {
    if state == pw_sys::pw_filter_state_PW_FILTER_STATE_ERROR && !error.is_null() {
        let msg = unsafe { std::ffi::CStr::from_ptr(error) }.to_string_lossy();
        eprintln!("flupmix: mixer filter error: {msg}");
    }
}

fn add_port(
    filter: *mut pw_sys::pw_filter,
    dir: u32,
    name: &str,
    channel: &str,
) -> Result<*mut c_void, String> {
    let props = properties! {
        "format.dsp" => "32 bit float mono audio",
        "port.name" => name,
        "audio.channel" => channel,
    };
    // SAFETY: the filter is valid and not yet connected; ownership of the
    // properties moves to PipeWire.
    let port = unsafe {
        pw_sys::pw_filter_add_port(
            filter,
            dir,
            pw_sys::pw_filter_port_flags_PW_FILTER_PORT_FLAG_MAP_BUFFERS,
            0,
            props.into_raw(),
            std::ptr::null_mut(),
            0,
        )
    };
    if port.is_null() {
        Err(format!("pw_filter_add_port({name}) failed"))
    } else {
        Ok(port)
    }
}

impl MixerFilter {
    /// Creates and connects the mixer node. Must run on the PipeWire main loop thread.
    pub fn new(core: &Core, controls: Arc<Controls>, shared: Arc<Shared>) -> Result<Self, String> {
        let props = properties! {
            "node.name" => MIXER_NODE,
            "node.description" => "Flupmix mixer",
            "media.type" => "Audio",
            "media.category" => "Filter",
            "media.role" => "DSP",
            "node.always-process" => "true",
            "node.autoconnect" => "false",
            "node.dont-reconnect" => "true",
        };
        let name = CString::new("flupmix").expect("static name");
        // SAFETY: core is a live connection; ownership of props moves to PipeWire.
        let filter =
            unsafe { pw_sys::pw_filter_new(core.as_raw_ptr(), name.as_ptr(), props.into_raw()) };
        if filter.is_null() {
            return Err("pw_filter_new failed".into());
        }

        let mut data = Box::new(CallbackData {
            mixer: Mixer::new(),
            matrix: Matrix::SILENT,
            controls,
            shared,
            in_ports: [[std::ptr::null_mut(); 2]; MAX_STRIPS],
            out_ports: [[std::ptr::null_mut(); 2]; N_DEST],
        });
        // SAFETY: all-zero is a valid (empty) value for these C structs.
        let mut events: Box<pw_sys::pw_filter_events> = Box::new(unsafe { std::mem::zeroed() });
        events.version = pw_sys::PW_VERSION_FILTER_EVENTS;
        events.process = Some(on_process);
        events.state_changed = Some(on_state_changed);
        let mut hook: Box<spa_sys::spa_hook> = Box::new(unsafe { std::mem::zeroed() });
        unsafe {
            pw_sys::pw_filter_add_listener(
                filter,
                &mut *hook,
                &*events,
                &mut *data as *mut CallbackData as *mut c_void,
            );
        }

        let this = Self {
            filter,
            _hook: hook,
            _events: events,
            _data: data,
        };
        // From here on `this` owns the filter, so errors destroy it via Drop.
        let mut this = this;
        let channels = ["FL", "FR"];
        for (s, pair) in this._data.in_ports.iter_mut().enumerate() {
            for (c, channel) in channels.iter().enumerate() {
                pair[c] = add_port(
                    filter,
                    spa_sys::SPA_DIRECTION_INPUT,
                    &mixer_in_port(s, c),
                    channel,
                )?;
            }
        }
        for (d, pair) in this._data.out_ports.iter_mut().enumerate() {
            for (c, channel) in channels.iter().enumerate() {
                pair[c] = add_port(
                    filter,
                    spa_sys::SPA_DIRECTION_OUTPUT,
                    &mixer_out_port(d, c),
                    channel,
                )?;
            }
        }
        let res = unsafe {
            pw_sys::pw_filter_connect(
                filter,
                pw_sys::pw_filter_flags_PW_FILTER_FLAG_RT_PROCESS,
                std::ptr::null_mut(),
                0,
            )
        };
        if res < 0 {
            return Err(format!("pw_filter_connect failed ({res})"));
        }
        Ok(this)
    }
}

impl Drop for MixerFilter {
    fn drop(&mut self) {
        // SAFETY: destroys the filter (and unhooks our listener) on the thread
        // that created it, before the boxed callback data is freed.
        unsafe { pw_sys::pw_filter_destroy(self.filter) };
    }
}
