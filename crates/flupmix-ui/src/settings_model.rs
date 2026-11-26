//! Shared application settings and their live side effects.

use std::time::Duration;

use flupmix_core::{
    settings::{self, Settings},
    Command, EngineHandle,
};
use gpui::{Context, Task};

use crate::theme;

pub(crate) struct SettingsModel {
    pub(crate) values: Settings,
    engine: EngineHandle,
    save_task: Option<Task<()>>,
}

impl SettingsModel {
    pub(crate) fn new(mut values: Settings, engine: EngineHandle, cx: &mut Context<Self>) -> Self {
        values.sanitize();
        engine.send(Command::SetAppRouting(values.app_routing));
        theme::set_palette(values.palette);
        cx.refresh_windows();
        Self {
            values,
            engine,
            save_task: None,
        }
    }

    pub(crate) fn change(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Settings)) {
        let previous = self.values;
        change(&mut self.values);
        self.values.sanitize();
        if self.values == previous {
            return;
        }
        if self.values.app_routing != previous.app_routing {
            self.engine
                .send(Command::SetAppRouting(self.values.app_routing));
        }
        if self.values.palette != previous.palette {
            theme::set_palette(self.values.palette);
            cx.refresh_windows();
        }
        // Replacing the retained task cancels the preceding debounce timer.
        self.save_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let _ = this.update(cx, |this, _| this.save());
        }));
        cx.notify();
    }

    pub(crate) fn save(&self) {
        if let Err(error) = settings::save(&self.values) {
            eprintln!("flupmix: saving settings: {error}");
        }
    }
}
