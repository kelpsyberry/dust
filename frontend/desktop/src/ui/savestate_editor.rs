use super::{window::Window, EmuState};
use crate::{config::Config, emu};
use chrono::DateTime;
use dust_core::{
    gpu::{Framebuffer, SCREEN_HEIGHT, SCREEN_WIDTH},
    utils::mem_prelude::*,
};
use imgui::{Image, StyleColor, TableFlags, TextureId, Ui, WindowHoveredFlags};
use miniz_oxide::{
    deflate::{compress_to_vec, CompressionLevel},
    inflate::{decompress_to_vec, DecompressError},
};
use std::{
    fmt, fs, io, mem,
    path::{Path, PathBuf},
    slice,
    time::SystemTime,
};

struct Savestate {
    contents: Vec<u8>,
    save: Option<BoxedByteSlice>,
    framebuffer: Box<Framebuffer>,
    texture_id: TextureId,
}

enum EntryKind {
    Savestate(Savestate),
    InProgress,
    Failed,
}

struct Entry {
    name: String,
    kind: EntryKind,
}

#[derive(Debug)]
pub enum SavestateError {
    Io(io::Error),
    Decompression(DecompressError),
    InvalidData,
}

impl From<io::Error> for SavestateError {
    fn from(value: io::Error) -> Self {
        SavestateError::Io(value)
    }
}

impl From<DecompressError> for SavestateError {
    fn from(value: DecompressError) -> Self {
        SavestateError::Decompression(value)
    }
}

impl fmt::Display for SavestateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SavestateError::Io(err) => write!(f, "I/O error: {err}"),
            SavestateError::Decompression(err) => write!(f, "decompression error: {err}"),
            SavestateError::InvalidData => f.write_str("invalid data"),
        }
    }
}
const SCREEN_SIZE: usize = SCREEN_WIDTH * SCREEN_HEIGHT;

proc_bitfield::bitfield! {
    #[derive(Clone, Copy)]
    struct SavestateInfo(pub u32) {
        save_len: u32 @ 0..=27,
        fb_is_le: bool @ 30,
        has_save: bool @ 31,
    }
}

impl Savestate {
    fn create_texture(window: &Window, framebuffer: &Framebuffer) -> TextureId {
        let texture = window.imgui_gfx.create_owned_texture(
            Some("Savestate framebuffer".into()),
            imgui_wgpu::TextureDescriptor {
                width: SCREEN_WIDTH as u32,
                height: SCREEN_HEIGHT as u32 * 2,
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
            unsafe {
                slice::from_raw_parts(framebuffer.as_ptr().cast::<u8>(), 2 * 4 * SCREEN_SIZE)
            },
            imgui_wgpu::TextureSetRange::default(),
        );
        window
            .imgui_gfx
            .add_texture(imgui_wgpu::Texture::Owned(texture))
    }

    fn load(path: &Path, window: &Window) -> Result<Self, SavestateError> {
        let compressed_contents = fs::read(path)?;
        let mut contents = decompress_to_vec(&compressed_contents)?;

        let info = {
            let pos = contents
                .len()
                .checked_sub(4)
                .ok_or(SavestateError::InvalidData)?;
            let value = SavestateInfo(contents[pos..].read_le(0));
            contents.truncate(pos);
            value
        };

        let framebuffer = {
            let mut buffer: Box<Framebuffer> = unsafe { Box::new_zeroed().assume_init() };
            let start_pos = contents
                .len()
                .checked_sub(2 * 4 * SCREEN_SIZE)
                .ok_or(SavestateError::InvalidData)?;
            unsafe {
                let [buffer_0, buffer_1] = &mut *buffer;
                let mut src = contents.as_ptr().add(start_pos).cast::<u32>();
                if info.fb_is_le() == cfg!(target_endian = "little") {
                    for pixel in buffer_0.iter_mut().chain(buffer_1) {
                        *pixel = src.read_unaligned();
                        src = src.add(1);
                    }
                } else {
                    for pixel in buffer_0.iter_mut().chain(buffer_1) {
                        *pixel = src.read_unaligned().swap_bytes();
                        src = src.add(1);
                    }
                }
            }
            contents.truncate(start_pos);
            buffer
        };

        let save = if info.has_save() {
            let save_len = info.save_len() as usize;
            let mut buffer = BoxedByteSlice::new_zeroed(save_len);
            let start_pos = contents
                .len()
                .checked_sub(save_len)
                .ok_or(SavestateError::InvalidData)?;
            unsafe {
                buffer
                    .as_mut_ptr()
                    .copy_from_nonoverlapping(contents.as_ptr().add(start_pos), save_len);
            }
            contents.truncate(start_pos);
            Some(buffer)
        } else {
            None
        };

        contents.shrink_to_fit();

        let texture_id = Self::create_texture(window, &framebuffer);

        Ok(Savestate {
            contents,
            save,
            framebuffer,
            texture_id,
        })
    }

    fn create(
        name: &str,
        mut contents: Vec<u8>,
        save: Option<BoxedByteSlice>,
        framebuffer: Box<Framebuffer>,
        savestate_dir: &Path,
        window: &Window,
    ) -> io::Result<Self> {
        let orig_len = contents.len();

        let mut info = SavestateInfo(0).with_fb_is_le(cfg!(target_endian = "little"));
        if let Some(save) = &save {
            contents.extend_from_slice(save);
            info.set_has_save(true);
            info.set_save_len(save.len() as u32);
        }

        contents.reserve(2 * 4 * SCREEN_SIZE);

        unsafe {
            let prev_len = contents.len();
            let mut dest = contents.as_mut_ptr().add(prev_len).cast::<u32>();
            for pixel in framebuffer[0].iter().chain(&framebuffer[1]) {
                dest.write_unaligned(*pixel);
                dest = dest.add(1);
            }
            contents.set_len(prev_len + 2 * 4 * SCREEN_SIZE);
        }

        contents.extend_from_slice(&info.0.to_le_bytes());

        fs::write(
            savestate_dir.join(format!("{name}.state")),
            compress_to_vec(&contents, CompressionLevel::BestSpeed as u8),
        )?;

        contents.truncate(orig_len);
        contents.shrink_to_fit();

        let texture_id = Self::create_texture(window, &framebuffer);

        Ok(Savestate {
            contents,
            save,
            framebuffer,
            texture_id,
        })
    }

    fn rename(
        &mut self,
        prev_name: String,
        new_name: &str,
        savestate_dir: &Path,
    ) -> io::Result<()> {
        let mut prev_file_name = prev_name;
        prev_file_name.push_str(".state");
        fs::rename(
            savestate_dir.join(&prev_file_name),
            savestate_dir.join(format!("{new_name}.state")),
        )
    }

    fn delete(self, name: &str, savestate_dir: &Path, window: &Window) -> io::Result<()> {
        window.imgui_gfx.remove_texture(self.texture_id);
        fs::remove_file(savestate_dir.join(format!("{name}.state")))
    }

    fn emu_savestate(&self) -> emu::Savestate {
        emu::Savestate {
            contents: self.contents.clone(),
            save: self.save.clone(),
            framebuffer: self.framebuffer.clone(),
        }
    }
}

pub(super) struct Editor {
    dir_path: Option<PathBuf>,
    entries: Vec<Entry>,
    editing_i: Option<usize>,
}

impl Editor {
    pub fn new() -> Self {
        Editor {
            dir_path: None,
            entries: Vec::new(),
            editing_i: None,
        }
    }

    pub fn update_game(&mut self, window: &Window, config: &Config, game_title: Option<&str>) {
        let new_dir_path =
            game_title.map(|title| config!(config, savestate_dir_path).0.join(title));
        if new_dir_path == self.dir_path {
            return;
        }
        self.dir_path = new_dir_path;

        for entry in self.entries.drain(..) {
            if let EntryKind::Savestate(savestate) = entry.kind {
                window.imgui_gfx.remove_texture(savestate.texture_id);
            }
        }

        if let Some(dir_path) = &self.dir_path {
            let dir_entries =
                match fs::create_dir_all(dir_path).and_then(|_| fs::read_dir(dir_path)) {
                    Ok(dir_entries) => dir_entries,
                    Err(err) => {
                        error!(
                            "Savestate directory error",
                            "Couldn't create/read savestate directory: {err}"
                        );
                        self.dir_path = None;
                        return;
                    }
                };
            let mut warnings = Vec::new();
            for entry in dir_entries {
                let Ok(entry) = entry else { continue };
                let path = entry.path();
                if path.extension() != Some("state".as_ref()) {
                    continue;
                }
                let Some(name) = path.file_stem().and_then(|p| p.to_str()) else {
                    continue;
                };
                match Savestate::load(&path, window) {
                    Ok(savestate) => self.entries.push(Entry {
                        name: name.to_owned(),
                        kind: EntryKind::Savestate(savestate),
                    }),
                    Err(err) => {
                        warnings.push(format!("Couldn't load savestate at {:?}: {err}", path));
                    }
                }
            }
            if !warnings.is_empty() {
                warning!(
                    "Missing savestates",
                    "Not all savestates could be loaded:{}",
                    format_list!(warnings)
                );
            }
        }
    }

    pub fn savestate_created(&mut self, name: String, savestate: emu::Savestate, window: &Window) {
        if let Some(dir_path) = &self.dir_path {
            if let Ok(savestate) = Savestate::create(
                &name,
                savestate.contents,
                savestate.save,
                savestate.framebuffer,
                dir_path,
                window,
            ) {
                if let Some(entry) = self.entries.iter_mut().find(|e| {
                    matches!(e, Entry {
                    name: entry_name,
                    kind: EntryKind::InProgress
                } if *entry_name == name)
                }) {
                    entry.kind = EntryKind::Savestate(savestate);
                }
            } else {
                self.savestate_failed(name);
            }
        }
    }

    pub fn savestate_failed(&mut self, name: String) {
        if let Some(entry) = self.entries.iter_mut().find(|e| {
            matches!(e, Entry {
            name: entry_name,
            kind: EntryKind::InProgress
        } if *entry_name == name)
        }) {
            entry.kind = EntryKind::Failed;
        }
    }

    pub fn draw(
        &mut self,
        ui: &Ui,
        window: &Window,
        config: &Config,
        emu_state: &Option<EmuState>,
    ) {
        let mut shown = false;
        ui.menu_with_enabled("\u{f02e} Savestates", self.dir_path.is_some(), || {
            shown = true;

            let frame_padding = style!(ui, frame_padding);
            let cell_padding = style!(ui, cell_padding);
            let item_spacing = style!(ui, item_spacing);
            let frame_rounding = style!(ui, frame_rounding);
            let text_line_height = ui.text_line_height();
            let frame_height = ui.frame_height();
            let image_width =
                (ui.io().display_size[0] * 0.25 - cell_padding[0] - frame_padding[0] * 2.0)
                    .min(SCREEN_WIDTH as f32 * 0.6);
            let image_height = image_width * (SCREEN_HEIGHT as f32 * 2.0) / SCREEN_WIDTH as f32;
            let cell_width = image_width + frame_padding[0] * 2.0;

            let mut text_heights = Vec::with_capacity((self.entries.len() + 1) >> 1);
            {
                let text_height =
                    |entry: &Entry| ui.calc_text_size_with_opts(&entry.name, false, image_width)[1];
                let (chunks, last) = self.entries.as_chunks::<2>();
                for [a, b] in chunks {
                    text_heights.push(text_height(a).max(text_height(b)));
                }
                if let Some(last) = last.first() {
                    text_heights.push(text_height(last));
                }
            }

            let cell_base_height = image_height + frame_padding[1] * 4.0 + item_spacing[1];

            let mut bg_color = ui.style_color(StyleColor::WindowBg);
            bg_color[3] *= 0.33;

            let mut hover_overlay_color = bg_color;
            hover_overlay_color[3] = 0.5;

            let scrollbar_size = if self.entries.len() >= 4 {
                style!(ui, scrollbar_size)
            } else {
                0.0
            };

            ui.child_window("savestates")
                .movable(false)
                .size([
                    if self.entries.is_empty() {
                        cell_width
                    } else {
                        (cell_width + cell_padding[0]) * 2.0 + scrollbar_size
                    },
                    {
                        let cell_base_height_with_padding =
                            cell_base_height + cell_padding[1] * 2.0;
                        let create_button_height = cell_base_height_with_padding + text_line_height;
                        match self.entries.len() {
                            0 => create_button_height,
                            1 => cell_base_height_with_padding + text_heights[0],
                            2 => {
                                cell_base_height_with_padding
                                    + text_heights[0]
                                    + create_button_height
                            }
                            3 => {
                                cell_base_height_with_padding
                                    + text_heights[0]
                                    + cell_base_height_with_padding
                                    + text_heights[1]
                            }
                            _ => (cell_base_height_with_padding + text_line_height * 2.0) * 2.5,
                        }
                    },
                ])
                .build(|| {
                    macro_rules! entry_layout {
                        (
                            $cell_height: expr, $upper_left: ident, $image_upper_left: ident,
                            $text_upper_left: ident, $hovered: ident, $hovered_color: expr
                        ) => {
                            ui.table_next_column();

                            let $upper_left = ui.cursor_screen_pos();
                            let lower_right =
                                [$upper_left[0] + cell_width, $upper_left[1] + $cell_height];
                            let $image_upper_left = [
                                $upper_left[0] + frame_padding[0],
                                $upper_left[1] + frame_padding[1],
                            ];
                            let $text_upper_left = [
                                $image_upper_left[0] + frame_padding[0],
                                $image_upper_left[1]
                                    + image_height
                                    + item_spacing[1]
                                    + frame_padding[1],
                            ];

                            let mut $hovered = false;
                            if ui.is_window_hovered_with_flags(
                                WindowHoveredFlags::ALLOW_WHEN_BLOCKED_BY_ACTIVE_ITEM,
                            ) {
                                let mouse_pos = ui.io().mouse_pos;
                                $hovered = ($upper_left[0]..lower_right[0]).contains(&mouse_pos[0])
                                    && ($upper_left[1]..lower_right[1]).contains(&mouse_pos[1]);
                            }

                            ui.get_window_draw_list()
                                .add_rect(
                                    $upper_left,
                                    lower_right,
                                    if $hovered { $hovered_color } else { bg_color },
                                )
                                .filled(true)
                                .rounding(frame_rounding)
                                .build();
                        };
                    }

                    macro_rules! entry_icon {
                        ($upper_left: ident, $cell_height: ident, $icon: expr) => {{
                            let _font = ui.push_font(window.imgui.large_icon_font);
                            let icon = $icon;
                            let icon_size = ui.calc_text_size(icon);
                            ui.set_cursor_screen_pos([
                                $upper_left[0] + (cell_width - icon_size[0]) * 0.5,
                                $upper_left[1] + ($cell_height - icon_size[1]) * 0.5,
                            ]);
                            ui.text(icon);
                        }};
                    }

                    if let Some(_table) = ui.begin_table_with_flags("", 2, TableFlags::NO_CLIP) {
                        let mut remove = None;

                        for (i, entry) in self.entries.iter_mut().enumerate() {
                            let _id = ui.push_id_usize(i);

                            let cell_height = cell_base_height + text_heights[i >> 1];

                            entry_layout!(
                                cell_height,
                                upper_left,
                                image_upper_left,
                                text_upper_left,
                                hovered,
                                ui.style_color(StyleColor::FrameBgHovered)
                            );

                            let draw_text = || {
                                ui.set_cursor_screen_pos(text_upper_left);
                                let _wrap_pos = ui.push_text_wrap_pos_with_pos(
                                    ui.cursor_pos()[0] + image_width - frame_padding[0],
                                );
                                ui.text(&entry.name);
                            };

                            if let EntryKind::Savestate(savestate) = &mut entry.kind {
                                ui.set_cursor_screen_pos(image_upper_left);
                                Image::new(savestate.texture_id, [image_width, image_height])
                                    .build(ui);

                                if hovered {
                                    ui.get_window_draw_list()
                                        .add_rect(
                                            image_upper_left,
                                            [
                                                image_upper_left[0] + image_width,
                                                image_upper_left[1] + image_height,
                                            ],
                                            hover_overlay_color,
                                        )
                                        .filled(true)
                                        .build();

                                    let buttons_size = [
                                        (frame_padding[0] * 4.0 + item_spacing[0] + 2.0 * 20.0)
                                            .max(
                                                frame_padding[0] * 2.0
                                                    + ui.calc_text_size("Apply")[0],
                                            ),
                                        frame_height * 2.0 + item_spacing[1],
                                    ];
                                    ui.set_cursor_screen_pos([
                                        image_upper_left[0] + (image_width - buttons_size[0]) * 0.5,
                                        image_upper_left[1]
                                            + (image_height - buttons_size[1]) * 0.5,
                                    ]);

                                    let _button_colors = [
                                        (StyleColor::Button, 0.65),
                                        (StyleColor::ButtonHovered, 0.9),
                                        (StyleColor::ButtonActive, 0.9),
                                    ]
                                    .map(
                                        |(style_color, alpha)| {
                                            let mut color = ui.style_color(style_color);
                                            color[3] = alpha;
                                            ui.push_style_color(style_color, color)
                                        },
                                    );

                                    let x = ui.cursor_screen_pos()[0];

                                    let top_button_width =
                                        (buttons_size[0] - item_spacing[0]) * 0.5;

                                    if ui.button_with_size("\u{f303}", [top_button_width, 0.0]) {
                                        self.editing_i = Some(i);
                                    }

                                    ui.same_line();

                                    if ui.button_with_size("\u{f1f8}", [top_button_width, 0.0]) {
                                        remove = Some(i);
                                    }

                                    ui.set_cursor_screen_pos([x, ui.cursor_screen_pos()[1]]);

                                    if ui.button_with_size("Apply", [buttons_size[0], 0.0]) {
                                        emu_state.as_ref().unwrap().send_message(
                                            emu::Message::ApplySavestate(savestate.emu_savestate()),
                                        );
                                    }
                                }

                                if Some(i) == self.editing_i {
                                    ui.set_cursor_screen_pos([
                                        text_upper_left[0] - frame_padding[0],
                                        text_upper_left[1] + text_heights[i >> 1] * 0.5
                                            - frame_height * 0.5,
                                    ]);
                                    ui.set_keyboard_focus_here();
                                    ui.set_next_item_width(image_width);
                                    let mut buffer = entry.name.clone();
                                    if ui
                                        .input_text("##name_input", &mut buffer)
                                        .auto_select_all(true)
                                        .enter_returns_true(true)
                                        .build()
                                    {
                                        self.editing_i = None;
                                        let prev_name = mem::replace(&mut entry.name, buffer);
                                        if savestate
                                            .rename(
                                                prev_name,
                                                &entry.name,
                                                self.dir_path.as_ref().unwrap(),
                                            )
                                            .is_err()
                                        {
                                            entry.kind = EntryKind::Failed;
                                        }
                                    }
                                } else {
                                    draw_text();
                                }

                                ui.set_cursor_screen_pos(upper_left);
                                ui.dummy([cell_width, cell_height]);
                            } else {
                                entry_icon!(
                                    upper_left,
                                    cell_height,
                                    match entry.kind {
                                        EntryKind::InProgress => "\u{f141}",
                                        EntryKind::Failed =>
                                            if hovered {
                                                "\u{f1f8}"
                                            } else {
                                                "\u{f06a}"
                                            },
                                        _ => unreachable!(),
                                    }
                                );

                                draw_text();

                                ui.set_cursor_screen_pos(upper_left);
                                if let EntryKind::Failed = &entry.kind {
                                    if ui.invisible_button("##remove", [cell_width, cell_height]) {
                                        remove = Some(i);
                                    }
                                } else {
                                    ui.dummy([cell_width, cell_height]);
                                }
                            }
                        }

                        {
                            let cell_height = cell_base_height
                                + text_heights
                                    .get(self.entries.len() >> 1)
                                    .copied()
                                    .unwrap_or(text_line_height);

                            entry_layout!(
                                cell_height,
                                upper_left,
                                _image_upper_left,
                                _text_upper_left,
                                _hovered,
                                ui.style_color(StyleColor::ButtonHovered)
                            );
                            entry_icon!(upper_left, cell_height, "+");

                            ui.set_cursor_screen_pos(upper_left);
                            if ui.invisible_button("##create", [cell_width, cell_height]) {
                                let name = DateTime::<chrono::Local>::from(SystemTime::now())
                                    .format("%Y-%m-%d %H:%M:%S%.3f")
                                    .to_string();
                                emu_state.as_ref().unwrap().send_message(
                                    emu::Message::CreateSavestate {
                                        name: name.clone(),
                                        include_save: config!(config, include_save_in_savestates),
                                    },
                                );
                                self.entries.push(Entry {
                                    name,
                                    kind: EntryKind::InProgress,
                                })
                            }
                        }

                        if let Some(i) = remove {
                            let entry = &mut self.entries[i];
                            let kind = mem::replace(&mut entry.kind, EntryKind::Failed);
                            if let EntryKind::Savestate(savestate) = kind {
                                if savestate
                                    .delete(&entry.name, self.dir_path.as_ref().unwrap(), window)
                                    .is_ok()
                                {
                                    self.entries.remove(i);
                                }
                            }
                        }
                    }
                });
        });

        if !shown {
            self.editing_i = None;
        }
    }
}
