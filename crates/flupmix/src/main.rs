use std::sync::mpsc;
use std::time::Duration;

use flupmix_core::{config, Command};
use flupmix_ui::{Tab, TrayEvent, UiOptions};

struct Tray {
    events: mpsc::Sender<TrayEvent>,
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "flupmix".into()
    }

    fn title(&self) -> String {
        "flupmix".into()
    }

    fn icon_name(&self) -> String {
        "multimedia-volume-control".into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.events.send(TrayEvent::Show);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: "Show mixer".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.events.send(TrayEvent::Show);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|this: &mut Self| {
                    let _ = this.events.send(TrayEvent::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// `--tab mixer|patchbay` picks the tab shown at startup.
fn initial_tab() -> Tab {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--tab" {
            return match args.next().as_deref() {
                Some("patchbay") => Tab::Patchbay,
                _ => Tab::Mixer,
            };
        }
    }
    Tab::Mixer
}

fn main() {
    let state = config::load_or_starter();
    let engine = flupmix_engine::spawn(state.clone());

    // Ctrl-C / SIGTERM: give the engine time to release routed applications.
    let handle = engine.handle();
    let _ = ctrlc::set_handler(move || {
        handle.send(Command::Quit);
        std::thread::sleep(Duration::from_millis(500));
        // Skip atexit/static destructors: the GPU and Wayland libraries can
        // crash while their worker threads are still running.
        // SAFETY: `_exit` has no preconditions.
        unsafe { libc::_exit(0) }
    });

    // Without a tray host (no StatusNotifier watcher) closing the window quits.
    let (tray_tx, tray_rx) = mpsc::channel();
    let tray = ksni::blocking::TrayMethods::spawn(Tray { events: tray_tx });
    let tray_events = match &tray {
        Ok(_) => Some(tray_rx),
        Err(err) => {
            eprintln!("flupmix: no system tray ({err}); closing the window will quit");
            None
        }
    };

    flupmix_ui::run(UiOptions {
        engine: engine.handle(),
        initial: state,
        tray: tray_events,
        initial_tab: initial_tab(),
    });

    drop(tray);
    drop(engine);
}
