use super::{
    super::{engine_3d, vram::Vram, Scanline},
    AffineBgData, AffineBgIndex, Bg, BlendCoeffsRaw, BrightnessControl, CaptureControl,
    ColorEffectsControl, Control, Data, WindowControl, WindowRanges, WindowsActive,
};

pub trait Renderer {
    fn post_load(&mut self, data: &Data);
    fn update_color_effects_control(&mut self, value: ColorEffectsControl);

    fn render_scanline(
        &mut self,
        line: u8,
        scanline_buffer: &mut Scanline<u32>,
        data: &mut Data,
        vram: &mut Vram,
        engine_3d_renderer: &mut dyn engine_3d::Renderer,
    );
    fn prerender_sprites(&mut self, line: u8, data: &mut Data, vram: &Vram);
}

impl Data {
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    #[inline]
    pub fn engine_3d_enabled_in_frame(&self) -> bool {
        self.engine_3d_enabled_in_frame
    }

    #[inline]
    pub fn control(&self) -> Control {
        self.control
    }

    #[inline]
    pub fn master_brightness_control(&self) -> BrightnessControl {
        self.master_brightness_control
    }

    #[inline]
    pub fn master_brightness_factor(&self) -> u32 {
        self.master_brightness_factor
    }

    #[inline]
    pub fn bgs(&self) -> &[Bg; 4] {
        &self.bgs
    }

    #[inline]
    pub fn affine_bg_data(&self) -> &[AffineBgData; 2] {
        &self.affine_bg_data
    }

    #[inline]
    pub fn set_affine_bg_pos(&mut self, i: AffineBgIndex, value: [i32; 2]) {
        self.affine_bg_data[i.get() as usize].pos = value;
    }

    #[inline]
    pub fn window_ranges(&self) -> &[WindowRanges; 2] {
        &self.window_ranges
    }

    #[inline]
    pub fn window_control(&self) -> &[WindowControl; 4] {
        &self.window_control
    }

    #[inline]
    pub fn color_effects_control(&self) -> ColorEffectsControl {
        self.color_effects_control
    }

    #[inline]
    pub fn blend_coeffs_raw(&self) -> BlendCoeffsRaw {
        self.blend_coeffs_raw
    }

    #[inline]
    pub fn blend_coeffs(&self) -> (u8, u8) {
        self.blend_coeffs
    }

    #[inline]
    pub fn brightness_coeff(&self) -> u8 {
        self.brightness_coeff
    }

    #[inline]
    pub fn windows_active(&self) -> WindowsActive {
        self.windows_active
    }

    #[inline]
    pub fn capture_control(&self) -> CaptureControl {
        self.capture_control
    }

    #[inline]
    pub fn capture_enabled_in_frame(&self) -> bool {
        self.capture_enabled_in_frame
    }

    #[inline]
    pub fn capture_height(&self) -> u8 {
        self.capture_height
    }
}
