//! Bottom panel — last status line + a 64-line scrolling log.
//!
//! Console messages are queued via `ConsoleLog::push`; the panel
//! renders them every frame.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use std::collections::VecDeque;

use crate::state::EditorContext;

/// Cap on retained log lines. Older lines are dropped.
pub const CONSOLE_CAP: usize = 64;

/// Scrolling log resource. Push lines via [`ConsoleLog::push`].
#[derive(Resource, Debug, Default)]
pub struct ConsoleLog {
    lines: VecDeque<String>,
}

impl ConsoleLog {
    pub fn push(&mut self, line: impl Into<String>) {
        let line = line.into();
        // Mirror to the tracing log so it shows in the terminal too.
        info!("console: {line}");
        if self.lines.len() == CONSOLE_CAP {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(String::as_str)
    }
}

pub fn draw_console(
    mut egui: EguiContexts,
    log: Res<ConsoleLog>,
    ctx: Res<EditorContext>,
) {
    let Ok(egui_ctx) = egui.ctx_mut() else {
        return;
    };

    egui::TopBottomPanel::bottom("editor_console")
        .resizable(true)
        .min_height(80.0)
        .show(egui_ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Status:");
                ui.label(&ctx.status);
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .max_height(120.0)
                .show(ui, |ui| {
                    for line in log.lines() {
                        ui.monospace(line);
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_log_caps_at_console_cap() {
        let mut log = ConsoleLog::default();
        for i in 0..(CONSOLE_CAP + 10) {
            log.push(format!("line {i}"));
        }
        assert_eq!(log.lines().count(), CONSOLE_CAP);
        // Oldest lines dropped — first surviving line should be `line 10`.
        let first = log.lines().next().unwrap();
        assert_eq!(first, "line 10");
    }
}
