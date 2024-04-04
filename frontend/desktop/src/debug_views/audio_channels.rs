use super::{
    common::regs::{bitfield, BitfieldCommand},
    BaseView, FrameDataSlot, FrameView, FrameViewMessages, InstanceableFrameViewEmuState,
    InstanceableView, RefreshType,
};
use crate::ui::window::Window;
use dust_core::{
    audio::channel::{Control, Format, Index as ChannelIndex, RepeatMode},
    cpu,
    emu::Emu,
};
use imgui::{PlotLines, SliderFlags, StyleVar, TableFlags};
use realfft::{num_complex::Complex, RealFftPlanner as FftPlanner};
use std::cmp::Ordering;

#[derive(Clone)]
struct RingBuffer<T: Copy> {
    buffer: Vec<T>,
    start: usize,
}

impl<T: Copy> RingBuffer<T> {
    pub fn new(len: usize, fill_value: T) -> Self {
        RingBuffer {
            buffer: vec![fill_value; len],
            start: 0,
        }
    }

    pub fn fill(&mut self, value: T) {
        self.buffer.fill(value);
    }

    pub fn extend(&mut self, iter: impl IntoIterator<Item = T>) {
        for elem in iter {
            self.buffer[self.start] = elem;
            self.start += 1;
            if self.start == self.buffer.len() {
                self.start = 0;
            }
        }
    }

    pub fn resize(&mut self, new_len: usize, value: T) {
        let prev_len = self.buffer.len();
        match new_len.cmp(&prev_len) {
            Ordering::Less => {
                let prev_start = self.start;
                self.start = (self.start + prev_len - new_len) % new_len;
                if new_len < prev_start {
                    self.buffer.copy_within(prev_start - new_len..prev_start, 0);
                } else {
                    self.buffer
                        .copy_within(prev_len - (new_len - prev_start)..prev_len, self.start);
                }
                self.buffer.truncate(new_len);
                self.buffer.shrink_to_fit();
            }
            Ordering::Greater => {
                let new_range = self.start..self.start + (new_len - prev_len);
                self.buffer.resize(new_len, value);
                self.buffer.copy_within(self.start..prev_len, new_range.end);
                self.buffer[new_range].fill(value);
            }
            Ordering::Equal => {}
        }
    }
}

pub struct ChannelData {
    channel: Option<ChannelIndex>,
    control: Control,
}

impl Default for ChannelData {
    fn default() -> Self {
        ChannelData {
            channel: None,
            control: Control(0),
        }
    }
}

pub struct EmuState {
    channel_index: ChannelIndex,
}

impl super::FrameViewEmuState for EmuState {
    type InitData = ChannelIndex;
    type Message = ChannelIndex;
    type FrameData = (ChannelData, Vec<i16>);

    fn new<E: cpu::Engine>(channel_index: Self::InitData, visible: bool, emu: &mut Emu<E>) -> Self {
        if visible {
            emu.audio.channel_audio_capture_data.mask |= 1 << channel_index.get();
        }
        EmuState { channel_index }
    }

    fn destroy<E: cpu::Engine>(self, emu: &mut Emu<E>) {
        emu.audio.channel_audio_capture_data.mask = 0;
    }

    fn handle_visibility_changed<E: cpu::Engine>(&mut self, visible: bool, emu: &mut Emu<E>) {
        if visible {
            emu.audio.channel_audio_capture_data.mask |= 1 << self.channel_index.get();
        } else {
            emu.audio.channel_audio_capture_data.mask = 0;
        }
    }

    fn handle_message<E: cpu::Engine>(&mut self, channel_index: Self::Message, emu: &mut Emu<E>) {
        emu.audio.channel_audio_capture_data.mask = 1 << channel_index.get();
        self.channel_index = channel_index;
    }

    fn prepare_frame_data<'a, E: cpu::Engine, S: FrameDataSlot<'a, Self::FrameData>>(
        &mut self,
        emu: &mut Emu<E>,
        frame_data: S,
    ) {
        let frame_data = frame_data.get_or_insert_with(|| (ChannelData::default(), Vec::new()));
        frame_data.0.channel = Some(self.channel_index);
        frame_data.1.clear();
        frame_data.1.extend_from_slice(
            &emu.audio.channel_audio_capture_data.buffers[self.channel_index.get() as usize],
        );
        let channel = &emu.audio.channels[self.channel_index.get() as usize];
        frame_data.0.control = channel.control();
    }
}

impl InstanceableFrameViewEmuState for EmuState {
    const ADDITION_TRIGGERS_REFRESH: bool = false;
    const DELETION_TRIGGERS_REFRESH: bool = true;
    fn visibility_change_triggers_refresh(visible: bool) -> bool {
        !visible
    }
    fn message_triggers_refresh(_message: &Self::Message) -> bool {
        true
    }

    fn refresh<E: cpu::Engine>(&mut self, _ty: RefreshType, visible: bool, emu: &mut Emu<E>) {
        if visible {
            emu.audio.channel_audio_capture_data.mask |= 1 << self.channel_index.get();
        }
    }

    fn finish_preparing_frame_data<E: cpu::Engine>(emu: &mut Emu<E>) {
        for buffer in &mut emu.audio.channel_audio_capture_data.buffers {
            buffer.clear();
        }
    }
}

pub struct AudioChannels {
    cur_channel: ChannelIndex,
    samples_to_show: u32,
    samples: RingBuffer<f32>,
    data: ChannelData,
    fft_planner: FftPlanner<f32>,
    fft_scratch_buf: Vec<Complex<f32>>,
    fft_input_buf: Vec<f32>,
    fft_output_buf: Vec<Complex<f32>>,
    fft_output_f32_buf: Vec<f32>,
}

impl BaseView for AudioChannels {
    const MENU_NAME: &'static str = "Audio channels";
}

impl FrameView for AudioChannels {
    type EmuState = EmuState;

    fn new(_window: &mut Window) -> Self {
        const DEFAULT_SAMPLES: u32 = 512 * 8;
        AudioChannels {
            cur_channel: ChannelIndex::new(0),
            samples_to_show: DEFAULT_SAMPLES,
            samples: RingBuffer::new(DEFAULT_SAMPLES as usize, 0.0),
            data: ChannelData::default(),
            fft_planner: FftPlanner::new(),
            fft_scratch_buf: Vec::new(),
            fft_input_buf: Vec::new(),
            fft_output_buf: Vec::new(),
            fft_output_f32_buf: Vec::new(),
        }
    }

    fn emu_state(&self) -> <Self::EmuState as super::FrameViewEmuState>::InitData {
        self.cur_channel
    }

    fn update_from_frame_data(
        &mut self,
        frame_data: &<Self::EmuState as super::FrameViewEmuState>::FrameData,
        _window: &mut Window,
    ) {
        self.data.channel = frame_data.0.channel;
        self.samples
            .extend(frame_data.1.iter().map(|sample| *sample as f32 / 32768.0));
        self.data.control = frame_data.0.control;
    }

    fn draw(
        &mut self,
        ui: &imgui::Ui,
        window: &mut Window,
        mut messages: impl FrameViewMessages<Self>,
    ) {
        let item_spacing = style!(ui, item_spacing);

        let sliders_width = 0.5 * (ui.content_region_avail()[0] - item_spacing[0]);

        let mut raw_channel_index = self.cur_channel.get();
        ui.set_next_item_width(sliders_width);
        let selection_updated = ui
            .slider_config("##channel", 0, 15)
            .display_format("Channel %d")
            .build(&mut raw_channel_index);

        if selection_updated {
            self.samples.fill(0.0);
            if let Some(channel_index) = ChannelIndex::new_checked(raw_channel_index) {
                self.cur_channel = channel_index;
            }
            messages.push(self.cur_channel);
        }

        ui.same_line();
        ui.set_next_item_width(sliders_width);
        if ui
            .slider_config("##visible_samples", 512, 256 * 1024)
            .flags(SliderFlags::LOGARITHMIC)
            .display_format("Last %d samples")
            .build(&mut self.samples_to_show)
        {
            self.samples_to_show &= !1;
            self.samples.resize(self.samples_to_show as usize, 0.0);
        }

        if self.data.channel != Some(self.cur_channel) {
            return;
        }

        PlotLines::new(ui, "##sample_graph", &self.samples.buffer)
            .graph_size([ui.content_region_avail()[0], 128.0])
            .scale_min(-1.0)
            .scale_max(1.0)
            .values_offset(self.samples.start)
            .build();

        let fft = self
            .fft_planner
            .plan_fft_forward(self.samples_to_show as usize);
        self.fft_input_buf.clear();
        self.fft_input_buf.reserve(self.samples_to_show as usize);
        {
            let mut i = self.samples.start;
            for _ in 0..self.samples.buffer.len() {
                if i >= self.samples.buffer.len() {
                    i = 0;
                }
                self.fft_input_buf.push(self.samples.buffer[i]);
                i += 1;
            }
        }
        self.fft_output_buf.resize(
            self.samples_to_show as usize / 2 + 1,
            Complex { re: 0.0, im: 0.0 },
        );
        self.fft_scratch_buf
            .resize(fft.get_scratch_len(), Complex { re: 0.0, im: 0.0 });
        fft.process_with_scratch(
            &mut self.fft_input_buf,
            &mut self.fft_output_buf,
            &mut self.fft_scratch_buf,
        )
        .unwrap();
        self.fft_output_f32_buf.clear();
        self.fft_output_f32_buf.reserve(self.fft_output_buf.len());
        let scale = 1.0 / (self.samples_to_show as f32).sqrt();
        self.fft_output_f32_buf.extend(
            self.fft_output_buf
                .iter()
                .map(|v| (v.norm() * scale + 1.0).ln()),
        );

        PlotLines::new(ui, "##frequency_graph", &self.fft_output_f32_buf)
            .graph_size([ui.content_region_avail()[0], 128.0])
            .scale_min(0.0)
            .build();

        let _mono_font_token = ui.push_font(window.imgui.mono_font);
        let _item_spacing = ui.push_style_var(StyleVar::ItemSpacing([0.0, item_spacing[1]]));

        ui.text("Control:");
        {
            let _frame_rounding = ui.push_style_var(StyleVar::FrameRounding(0.0));
            bitfield(
                ui,
                2.0,
                false,
                true,
                self.data.control.0,
                &[
                    BitfieldCommand::Field("Vol", 7),
                    BitfieldCommand::Skip(1),
                    BitfieldCommand::Field("VS", 2),
                    BitfieldCommand::Skip(5),
                    BitfieldCommand::Field("H", 1),
                    BitfieldCommand::Field("Pan", 7),
                    BitfieldCommand::Skip(1),
                    BitfieldCommand::Field("WD", 3),
                    BitfieldCommand::Field("RM", 2),
                    BitfieldCommand::Field("F", 2),
                    BitfieldCommand::Field("R", 1),
                ],
            );
        }

        if let Some(_table_token) = ui.begin_table_with_flags(
            "##control_fields",
            2,
            TableFlags::BORDERS_INNER_V | TableFlags::SIZING_STRETCH_SAME | TableFlags::NO_CLIP,
        ) {
            ui.table_next_column();

            selectable_value!(ui, "Volume", "000", "{}", self.data.control.volume());
            selectable_value!(
                ui,
                "Volume shift",
                "0",
                "{}",
                self.data.control.volume_shift()
            );

            ui.align_text_to_frame_padding();
            ui.text("Hold: ");
            ui.same_line();
            ui.checkbox("##hold", &mut self.data.control.hold());

            selectable_value!(ui, "Pan", "000", "{}", self.data.control.pan());

            ui.table_next_column();

            selectable_value!(
                ui,
                "PSG wave duty",
                "0",
                "{}",
                self.data.control.psg_wave_duty()
            );

            ui.align_text_to_frame_padding();
            ui.text(format!(
                "Repeat mode: {}",
                match self.data.control.repeat_mode() {
                    RepeatMode::Manual => "Manual",
                    RepeatMode::OneShot => "One-shot",
                    RepeatMode::LoopInfinite => "Loop",
                }
            ));

            let format = self.data.control.format(self.cur_channel);
            ui.align_text_to_frame_padding();
            ui.text(if format == Format::Silence {
                format!("Format: Invalid ({})", self.data.control.format_raw())
            } else {
                format!(
                    "Format: {}",
                    match format {
                        Format::Pcm8 => "PCM8",
                        Format::Pcm16 => "PCM16",
                        Format::Adpcm => "IMA-ADPCM",
                        Format::PsgWave => "PSG wave",
                        Format::PsgNoise => "PSG noise",
                        _ => unreachable!(),
                    }
                )
            });

            ui.align_text_to_frame_padding();
            ui.text("Running: ");
            ui.same_line();
            ui.checkbox("##running", &mut self.data.control.running());
        }
    }
}

impl InstanceableView for AudioChannels {}
