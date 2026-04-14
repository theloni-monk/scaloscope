use atomic_float::AtomicF32;
use nih_plug::prelude::*;
use nih_plug_iced::{application, create_iced_editor, WindowState};
use nih_plug_iced::iced::{
    self, PollSubNotifier
};
use std::sync::Arc;

use crate::editor::*;

mod editor;

/// The time it takes for the peak meter to decay by 12 dB after switching to complete silence.
const PEAK_METER_DECAY_MS: f64 = 150.0;

const WINDOW_WIDTH: u32 = 640;
const WINDOW_HEIGHT: u32 = 480;

#[derive(Params)]
pub struct ScaloscopeParams {
    /// The editor state, saved together with the parameter state so the custom scaling can be
    /// restored.
    #[persist = "window-state"]
    window_state: Arc<WindowState>,

    #[id = "gain"]
    pub gain: FloatParam,
}

/// This is mostly identical to the gain example, minus some fluff, and with a GUI.
pub struct Scaloscope {
    params: Arc<ScaloscopeParams>,

    /// Needed to normalize the peak meter's response based on the sample rate.
    peak_meter_decay_weight: f32,
    /// The current data for the peak meter. This is stored as an [`Arc`] so we can share it between
    /// the GUI and the audio processing parts. If you have more state to share, then it's a good
    /// idea to put all of that in a struct behind a single `Arc`.
    ///
    /// This is stored as voltage gain.
    peak_meter: Arc<AtomicF32>,

    /// An atomic flag used to notify the program when it should poll for new updates
    /// and redraw (i.e. as a result of the host updating parameters or the audio thread
    /// updating the state of meters). This flag is polled every frame right before
    /// drawing. If the flag is set then the [`poll_events`] subscription will be called, and
    /// the program will update and redraw.
    notifier: PollSubNotifier,
}

impl Default for Scaloscope {
    fn default() -> Self {
        Self {
            
notifier: PollSubNotifier::new(),
            params: Arc::new(ScaloscopeParams::default()),
            peak_meter_decay_weight: 1.0,
            peak_meter: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
        }
    }
}

impl Default for ScaloscopeParams {
    fn default() -> Self {
        Self {
            window_state: WindowState::from_logical_size(WINDOW_WIDTH, WINDOW_HEIGHT),

            // See the main gain example for more details
            gain: FloatParam::new(
                "Scaloscope",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-30.0),
                    max: util::db_to_gain(30.0),
                    factor: FloatRange::gain_skew_factor(-30.0, 30.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

impl Plugin for Scaloscope {
    const NAME: &'static str = "Scaloscope GUI (iced)";
    const VENDOR: &'static str = "thelonious c";
    const URL: &'static str = "https://theloniouscoop.dev";
    const EMAIL: &'static str = "theloni@eecs.berkeley.edu";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    // fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
    //     editor::create(
    //         self.params.clone(),
    //         self.peak_meter.clone(),
    //         self.params.editor_state.clone(),
    //     )
    // }
    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        create_iced_editor(
            self.params.window_state.clone(),
            ScaloscopeEditorState {
                params: self.params.clone(),
                peak_meter: self.peak_meter.clone(),
            },
            self.notifier.clone(),
            Default::default(),
            |editor_state, nih_ctx| {
                application(
                    editor_state,
                    nih_ctx,
                    ScaloscopeGui::new,
                    ScaloscopeGui::update,
                    ScaloscopeGui::view,
                )
                .theme(ScaloscopeGui::theme)
                // Subscribe to the poller which detects when the application should poll
                // parameters/meters and redraw.
                .subscription(|_| iced::poll_events().map(|_| GuiMessage::Poll))
                .run()
            },
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        // After `PEAK_METER_DECAY_MS` milliseconds of pure silence, the peak meter's value should
        // have dropped by 12 dB
        self.peak_meter_decay_weight = 0.25f64
            .powf((buffer_config.sample_rate as f64 * PEAK_METER_DECAY_MS / 1000.0).recip())
            as f32;

        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        for channel_samples in buffer.iter_samples() {
            let mut amplitude = 0.0;
            let num_samples = channel_samples.len();

            let gain = self.params.gain.smoothed.next();
            for sample in channel_samples {
                *sample *= gain;
                amplitude += *sample;
            }

            // To save resources, a plugin can (and probably should!) only perform expensive
            // calculations that are only displayed on the GUI while the GUI is open
            if self.params.window_state.is_open() {
                amplitude = (amplitude / num_samples as f32).abs();
                let current_peak_meter = self.peak_meter.load(std::sync::atomic::Ordering::Relaxed);
                let new_peak_meter = if amplitude > current_peak_meter {
                    amplitude
                } else {
                    current_peak_meter * self.peak_meter_decay_weight
                        + amplitude * (1.0 - self.peak_meter_decay_weight)
                };

                self.peak_meter
                    .store(new_peak_meter, std::sync::atomic::Ordering::Relaxed)
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Scaloscope {
    const CLAP_ID: &'static str = "com.theloni.scaloscope-gui-iced";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A wavelet scalogram terminal plotter plugin built with the NIH-plug framework and the iced GUI library.");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Utility,
    ];
}


nih_export_clap!(Scaloscope);