mod app;
mod mixer_view;
pub mod patchbay;
pub mod theme;
mod widgets;

pub enum TrayEvent {
    Show,
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Mixer,
    Patchbay,
}

pub struct UiOptions {
    pub engine: flupmix_core::EngineHandle,
    pub initial: flupmix_core::MixerState,
    pub initial_tab: Tab,
    pub tray: Option<std::sync::mpsc::Receiver<TrayEvent>>,
}

pub fn run(opts: UiOptions) {
    gpui_platform::application().run(move |cx: &mut gpui::App| app::launch(opts, cx));
}
