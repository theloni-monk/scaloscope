use atomic_float::AtomicF32;
use nih_plug::prelude::{util, Editor, GuiContext, Param};
use nih_plug_iced::iced::{
    self, Center, PollSubNotifier, Theme,
    widget::{Column, ProgressBar, button, column, slider, text},
};
use nih_plug_iced::{EditorState, NihGuiContext, WindowState, application, create_iced_editor};

use std::sync::{Arc, atomic::Ordering};
use std::time::Duration;

use crate::ScaloscopeParams;

#[derive(Debug, Clone, Copy)]
pub enum GuiMessage {
    /// Sent when the application should poll parameters/meters and redraw.
    Poll,
    Increment,
    Decrement,
    GainChanged(f32),
}

/// State relating to the editor itself (not necessarly the GUI). Put any
/// state that should persist between editor opens here.
pub struct ScaloscopeEditorState {
    pub params: Arc<ScaloscopeParams>,
    pub peak_meter: Arc<AtomicF32>,
}

pub struct ScaloscopeGui {
    /// The editor state is stored inside of a wrapper which allows the
    /// state to persist across editor opens.
    editor_state: EditorState<ScaloscopeEditorState>,

    /// A handle that can be used to request operations from nih-plug, like
    /// resizing the window.
    #[allow(unused)]
    nih_ctx: NihGuiContext,

    value: i64,
    peak_meter_db: f32,
}

impl ScaloscopeGui {
    pub fn new(editor_state: EditorState<ScaloscopeEditorState>, nih_ctx: NihGuiContext) -> Self {
        Self {
            editor_state,
            nih_ctx,
            value: 0,
            peak_meter_db: nih_plug::util::gain_to_db(0.0),
        }
    }

    pub fn theme(&self) -> Option<Theme> {
        Some(Theme::Dark)
    }

    pub fn update(&mut self, message: GuiMessage) {
        let setter = self.nih_ctx.param_setter();
        let params = &self.editor_state.params;

        match message {
            GuiMessage::Poll => {
                self.peak_meter_db = nih_plug::util::gain_to_db(
                    self.editor_state.peak_meter.load(Ordering::Relaxed),
                );
            }
            GuiMessage::Increment => {
                self.value += 1;
            }
            GuiMessage::Decrement => {
                self.value -= 1;
            }
            GuiMessage::GainChanged(value) => {
                // TODO: Add generic slider widget
                setter.begin_set_parameter(&params.gain);
                setter.set_parameter_normalized(&params.gain, value);
                setter.end_set_parameter(&params.gain);
            }
        }
    }

    pub fn view(&self) -> Column<'_, GuiMessage> {
        let params = &self.editor_state.params;

        column![
            button("Increment").on_press(GuiMessage::Increment),
            text(self.value).size(30),
            button("Decrement").on_press(GuiMessage::Decrement),
            // TODO: Add generic slider widget
            slider(
                0.0..=1.0,
                params.gain.modulated_normalized_value(),
                GuiMessage::GainChanged
            )
            .step(0.001),
            text(
                params
                    .gain
                    .normalized_value_to_string(params.gain.modulated_normalized_value(), true)
            ),
            ProgressBar::new(-80.0..=0.0, self.peak_meter_db),
        ]
        .padding(20)
        .spacing(12.0)
        .align_x(Center)
    }
}