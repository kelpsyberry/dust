use super::{window::Window, EmuState};
#[cfg(target_os = "macos")]
use crate::config::TitleBarMode;
use crate::{
    config::{Config, GameIconMode},
    emu::ds_slot_rom::DsSlotRom,
    utils::icon_data_to_rgba8,
};
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
use dust_core::ds_slot::rom::icon_title;
use imgui::Ui;
#[cfg(target_os = "macos")]
use imgui::{Image, TextureId};
#[cfg(target_os = "macos")]
use std::path::PathBuf;
use std::{fmt::Write, path::Path};
#[cfg(target_os = "macos")]
use tempfile::TempDir;

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct TitleComponents: u8 {
        const EMU_NAME = 1 << 0;
        const GAME_TITLE = 1 << 1;
        const FPS = 1 << 2;
    }
}

pub struct TitleMenuBarState {
    fps_fixed: Option<u64>,
    menu_bar_is_visible: bool,

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    game_icon_rgba8_pixels: Option<Box<[u8; 32 * 32 * 4]>>,
    #[cfg(target_os = "macos")]
    game_icon_texture_id: Option<TextureId>,
    #[cfg(target_os = "macos")]
    game_file_path: Option<PathBuf>,
    #[cfg(target_os = "macos")]
    temp_icon_dir: Option<TempDir>,
    #[cfg(target_os = "macos")]
    file_path: Option<PathBuf>,
    #[cfg(target_os = "macos")]
    shown_file_path: Option<PathBuf>,
}

impl TitleMenuBarState {
    pub fn new(_config: &Config) -> Self {
        TitleMenuBarState {
            fps_fixed: None,
            menu_bar_is_visible: true,

            #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
            game_icon_rgba8_pixels: None,
            #[cfg(target_os = "macos")]
            game_icon_texture_id: None,
            #[cfg(target_os = "macos")]
            game_file_path: None,
            #[cfg(target_os = "macos")]
            temp_icon_dir: if config!(_config, game_icon_mode) == GameIconMode::Game {
                tempfile::Builder::new().prefix("dust-icons").tempdir().ok()
            } else {
                None
            },
            #[cfg(target_os = "macos")]
            file_path: None,
            #[cfg(target_os = "macos")]
            shown_file_path: None,
        }
    }

    fn title(&self, components: TitleComponents, emu: &Option<EmuState>) -> String {
        let mut needs_separator = false;
        let mut buffer = if components.contains(TitleComponents::EMU_NAME) {
            needs_separator = true;
            "Dust".to_string()
        } else {
            String::new()
        };
        if let Some(emu) = emu {
            if components.contains(TitleComponents::GAME_TITLE) {
                if needs_separator {
                    buffer.push_str(" - ");
                }
                buffer.push_str(&emu.title);
                needs_separator = true;
            }
            if components.contains(TitleComponents::FPS) {
                if let Some(fps_fixed) = self.fps_fixed {
                    if needs_separator {
                        buffer.push_str(" - ");
                    }
                    let _ = write!(buffer, "{:.01} FPS", fps_fixed as f32 / 10.0);
                }
            }
        } else if components.contains(TitleComponents::GAME_TITLE) {
            if needs_separator {
                buffer.push_str(" - ");
            }
            buffer.push_str("No game loaded");
        }
        buffer
    }

    pub fn start_game(
        &mut self,
        ds_slot_rom: Option<&mut DsSlotRom>,
        _ds_slot_rom_path: Option<&Path>,
        config: &Config,
        _window: &Window,
    ) {
        self.menu_bar_is_visible = !config!(config, full_window_screen);

        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
        {
            self.game_icon_rgba8_pixels = ds_slot_rom.and_then(|rom_contents| {
                let icon_title_offset = icon_title::read_icon_title_offset(rom_contents)?;
                let icon =
                    icon_title::DefaultIcon::decode_at_offset(icon_title_offset, rom_contents)?;
                Some(icon_data_to_rgba8(&icon.palette, &icon.pixels))
            });
            #[cfg(target_os = "macos")]
            {
                self.game_file_path = _ds_slot_rom_path.map(Path::to_path_buf);
            }

            self.update_game_icon(config, _window);
        }
    }

    pub fn stop_game(&mut self, config: &Config, _window: &Window) {
        self.menu_bar_is_visible = true;

        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
        {
            self.game_icon_rgba8_pixels = None;
            #[cfg(target_os = "macos")]
            {
                self.game_file_path = None;
            }

            self.update_game_icon(config, _window);
        }
    }

    pub fn update_fps(&mut self, fps: f32) {
        self.fps_fixed = Some((fps * 10.0).round() as u64);
    }

    pub fn menu_bar_is_visible(&self) -> bool {
        self.menu_bar_is_visible
    }

    pub fn toggle_menu_bar(&mut self, config: &Config) {
        if config!(config, full_window_screen) {
            self.menu_bar_is_visible = !self.menu_bar_is_visible;
        }
    }

    #[cfg(target_os = "macos")]
    fn create_game_icon_file(&self) -> Option<PathBuf> {
        let temp_icon_dir = self.temp_icon_dir.as_ref()?;
        let pixels = self.game_icon_rgba8_pixels.as_deref()?;

        // Create the icon
        let mut output =
            b"P7\nWIDTH 32\nHEIGHT 32\nDEPTH 4\nMAXVAL 255\nTUPLTYPE RGB_ALPHA\nENDHDR".to_vec();
        output.extend_from_slice(pixels);
        let icon_path = temp_icon_dir.path().join("icon.ppm");
        std::fs::write(&icon_path, output).ok()?;

        // Set the icon's content as its thumbnail
        unsafe {
            use cocoa::base::{id, nil, BOOL, NO};

            let path: id = {
                const UTF8_ENCODING: usize = 4;
                let string: id = msg_send![class!(NSString), alloc];
                let bytes = icon_path.as_os_str().as_encoded_bytes();
                msg_send![string,
                    initWithBytes:bytes.as_ptr()
                    length:bytes.len()
                    encoding:UTF8_ENCODING as id]
            };

            let icon_image: id = {
                let image: id = msg_send![class!(NSImage), alloc];
                msg_send![image, initByReferencingFile:path]
            };
            if icon_image == nil {
                return None;
            }

            let workspace: id = msg_send![class!(NSWorkspace), sharedWorkspace];
            let success: BOOL = msg_send![workspace,
                setIcon:icon_image
                forFile:path
                options:0 as id
            ];
            if success == NO {
                return None;
            }
        }

        Some(icon_path)
    }

    #[cfg(target_os = "macos")]
    fn create_game_icon_texture(&self, window: &Window) -> Option<TextureId> {
        let pixels = self.game_icon_rgba8_pixels.as_deref()?;
        let texture = window.imgui_gfx.create_owned_texture(
            Some("Game icon".into()),
            imgui_wgpu::TextureDescriptor {
                width: 32,
                height: 32,
                format: wgpu::TextureFormat::Rgba8Unorm,
                ..Default::default()
            },
            imgui_wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            },
        );
        texture.set_data(
            window.gfx_device(),
            window.gfx_queue(),
            pixels,
            Default::default(),
        );
        Some(
            window
                .imgui_gfx
                .add_texture(imgui_wgpu::Texture::Owned(texture)),
        )
    }

    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    fn update_game_icon(&mut self, config: &Config, _window: &Window) {
        #[cfg(target_os = "macos")]
        {
            if let Some(texture_id) = self.game_icon_texture_id.take() {
                _window.imgui_gfx.remove_texture(texture_id);
            }

            if config!(config, title_bar_mode) == TitleBarMode::Imgui {
                self.file_path = None;
                self.game_icon_texture_id = match config!(config, game_icon_mode) {
                    GameIconMode::None | GameIconMode::File => None,
                    GameIconMode::Game => self.create_game_icon_texture(_window),
                };
            } else {
                self.file_path = match config!(config, game_icon_mode) {
                    GameIconMode::None => None,
                    GameIconMode::File => self.game_file_path.clone(),
                    GameIconMode::Game => self
                        .create_game_icon_file()
                        .or_else(|| self.game_file_path.clone()),
                };
                self.game_icon_texture_id = None;
            }
        }

        #[cfg(any(target_os = "linux", target_os = "windows"))]
        match config!(config, game_icon_mode) {
            GameIconMode::None | GameIconMode::File => {
                _window.set_icon(None);
            }
            GameIconMode::Game => {
                _window.set_icon(self.game_icon_rgba8_pixels.as_deref().and_then(|pixels| {
                    winit::window::Icon::from_rgba(pixels.to_vec(), 32, 32).ok()
                }));
            }
        }
    }

    pub fn update_config(&mut self, config: &Config, _window: &mut Window) {
        if let Some(full_window_screen) = config_changed_value!(config, full_window_screen) {
            self.menu_bar_is_visible |= !full_window_screen;
        }

        #[cfg(target_os = "macos")]
        {
            if config_changed!(config, game_icon_mode | title_bar_mode) {
                self.temp_icon_dir = if config!(config, title_bar_mode) == TitleBarMode::Imgui {
                    None
                } else {
                    match config!(config, game_icon_mode) {
                        GameIconMode::None => None,
                        GameIconMode::File => None,
                        GameIconMode::Game => {
                            tempfile::Builder::new().prefix("dust-icons").tempdir().ok()
                        }
                    }
                };
                self.update_game_icon(config, _window);
            }

            if let Some(mode) = config_changed_value!(config, title_bar_mode) {
                _window.set_macos_title_bar_transparent(mode.system_title_bar_is_transparent());
            }
        }

        #[cfg(any(target_os = "linux", target_os = "windows"))]
        if config_changed!(config, game_icon_mode) {
            self.update_game_icon(config, _window);
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn update_system_title_bar(
        &mut self,
        emu: &Option<EmuState>,
        _config: &Config,
        window: &Window,
    ) {
        window.set_title(&self.title(TitleComponents::all(), emu));
    }

    #[cfg(target_os = "macos")]
    pub fn update_system_title_bar(
        &mut self,
        emu: &Option<EmuState>,
        config: &Config,
        window: &Window,
    ) {
        let show_system_title_bar =
            config!(config, title_bar_mode).should_show_system_title_bar(self.menu_bar_is_visible);
        let shown_file_path = if show_system_title_bar {
            window.set_title(&self.title(TitleComponents::all(), emu));
            self.file_path.as_deref()
        } else {
            window.set_title("");
            None
        };
        if shown_file_path != self.shown_file_path.as_deref() {
            self.shown_file_path = shown_file_path.map(Path::to_path_buf);
            window.set_file_path(shown_file_path);
        }
    }

    #[allow(unused_variables)]
    pub fn draw_imgui_title(
        &self,
        right_title_limit: f32,
        ui: &Ui,
        emu: &Option<EmuState>,
        config: &Config,
    ) {
        #[cfg(target_os = "macos")]
        {
            if config!(config, title_bar_mode) != TitleBarMode::Imgui {
                return;
            }

            // TODO: When imgui-rs provides RenderTextEllipsis, use it; for now, the
            //       title just gets replaced by the FPS and then hidden.
            let item_spacing = style!(ui, item_spacing)[0];

            let draw_title_icon = move |text: &str, icon_visible: bool| {
                let mut width = ui.calc_text_size(text)[0];
                if icon_visible {
                    width += ui.frame_height() + item_spacing;
                }
                let max_start_x = right_title_limit - (width + item_spacing); // Add right spacing

                let orig_cursor_pos = ui.cursor_pos();

                let mut start_x = orig_cursor_pos[0] + item_spacing; // Add left spacing
                if start_x > max_start_x {
                    return false;
                }

                let centered_start_x = ui.window_size()[0] * 0.5 - width * 0.5;
                if centered_start_x > start_x {
                    start_x = centered_start_x.min(max_start_x);
                }

                ui.separator();
                ui.set_cursor_pos([start_x, orig_cursor_pos[1]]);
                if icon_visible {
                    if let Some(texture_id) = self.game_icon_texture_id {
                        ui.set_cursor_screen_pos([
                            ui.cursor_screen_pos()[0],
                            ui.window_pos()[1] + (ui.window_size()[1] - ui.frame_height()) * 0.5,
                        ]);
                        Image::new(texture_id, [ui.frame_height(); 2]).build(ui);
                        ui.same_line();
                    }
                }
                ui.text(text);

                true
            };

            for &(components, icon_visible) in &[
                (TitleComponents::all(), true),
                (TitleComponents::all(), false),
                (TitleComponents::GAME_TITLE | TitleComponents::FPS, false),
                (TitleComponents::FPS, false),
            ][self.game_icon_texture_id.is_none() as usize..]
            {
                let title = self.title(components, emu);
                if title.is_empty() || draw_title_icon(&title, icon_visible) {
                    break;
                }
            }
        }
    }
}
