use super::{
    y_pos::{SignedYPos, YPos, YPosRaw},
    RangeInclusive, Scrollbar,
};
use core::{fmt::Write, num::NonZeroU8};
use imgui::{Drag, Key, MouseButton, StyleColor, StyleVar, Ui, WindowFocusedFlags};

// TODO:
// - Add an `access_rights` callback that returns whether a given address's access rights are
//  `None`, `ReadOnly` or `ReadWrite`.
// - Add a data preview (8-bit, 16-bit, 32-bit, string, etc)
// - Add editing... somehow

bitflags::bitflags! {
    pub struct Flags: u16 {
        // Creation flags
        const READ_ONLY = 1 << 0;
        const SHOW_VIEW_OPTIONS = 1 << 1;
        const SHOW_RANGE = 1 << 2;
        const SHOW_DATA_PREVIEW = 1 << 3;

        // Options
        const GRAY_OUT_ZEROS = 1 << 8;
        const UPPERCASE_HEX = 1 << 9;
        const LITTLE_ENDIAN_COLS = 1 << 10;
        const SHOW_HEXII = 1 << 11;
        const SHOW_ASCII = 1 << 12;
    }
}

pub type Addr = u64;
pub type WAddr = u128;

pub struct MemoryEditor {
    cols: NonZeroU8,
    col_size: NonZeroU8,
    bytes_per_row: Addr,
    addr_digits: Option<NonZeroU8>,
    flags: Flags,
    addr_range: RangeInclusive<Addr>,

    scrollbar: Scrollbar,
    visible_data_rows: RangeInclusive<Addr>,

    selected_addr: Addr,
    addr_input: String,
    selected_addr_changed: bool,

    str_buffer: String,
    layout: Option<Layout>,
}

impl MemoryEditor {
    #[inline]
    pub fn new() -> Self {
        MemoryEditor {
            cols: NonZeroU8::new(16).unwrap(),
            col_size: NonZeroU8::new(1).unwrap(),
            bytes_per_row: 16,
            addr_digits: None,
            flags: Flags::SHOW_VIEW_OPTIONS
                | Flags::SHOW_RANGE
                | Flags::SHOW_DATA_PREVIEW
                | Flags::GRAY_OUT_ZEROS
                | Flags::UPPERCASE_HEX
                | Flags::SHOW_ASCII,
            addr_range: (0, 0).into(),

            scrollbar: Scrollbar::new(),
            visible_data_rows: (0, 0).into(),

            selected_addr: 0,
            addr_input: String::new(),
            selected_addr_changed: true,

            str_buffer: String::new(),
            layout: None,
        }
    }

    #[inline]
    pub fn calc_bytes_per_row(&self) -> Addr {
        self.cols.get() as Addr * self.col_size.get() as Addr
    }

    #[inline]
    pub fn cols(mut self, cols: NonZeroU8) -> Self {
        self.cols = cols;
        self.bytes_per_row = self.calc_bytes_per_row();
        self.layout = None;
        self
    }

    #[inline]
    pub fn col_size(mut self, col_size: NonZeroU8) -> Self {
        self.col_size = col_size;
        self.bytes_per_row = self.calc_bytes_per_row();
        self.layout = None;
        self
    }

    #[inline]
    pub fn addr_digits(mut self, addr_digits: Option<NonZeroU8>) -> Self {
        self.addr_digits = addr_digits;
        self.layout = None;
        self
    }

    #[inline]
    pub fn flags(mut self, flags: Flags) -> Self {
        self.flags = flags;
        self.layout = None;
        self
    }

    #[inline]
    pub fn read_only(mut self, read_only: bool) -> Self {
        self.flags.set(Flags::READ_ONLY, read_only);
        self
    }

    #[inline]
    pub fn show_options(mut self, show_options: bool) -> Self {
        self.flags.set(Flags::SHOW_VIEW_OPTIONS, show_options);
        self.layout = None;
        self
    }

    #[inline]
    pub fn show_range(mut self, show_range: bool) -> Self {
        self.flags.set(Flags::SHOW_RANGE, show_range);
        self.layout = None;
        self
    }

    #[inline]
    pub fn show_data_preview(mut self, show_data_preview: bool) -> Self {
        self.flags.set(Flags::SHOW_DATA_PREVIEW, show_data_preview);
        self.layout = None;
        self
    }

    #[inline]
    pub fn gray_out_zeros(mut self, gray_out_zeros: bool) -> Self {
        self.flags.set(Flags::GRAY_OUT_ZEROS, gray_out_zeros);
        self
    }

    #[inline]
    pub fn uppercase_hex(mut self, uppercase_hex: bool) -> Self {
        self.flags.set(Flags::UPPERCASE_HEX, uppercase_hex);
        self
    }

    #[inline]
    pub fn little_endian_cols(mut self, little_endian_cols: bool) -> Self {
        self.flags
            .set(Flags::LITTLE_ENDIAN_COLS, little_endian_cols);
        self
    }

    #[inline]
    pub fn show_hexii(mut self, show_hexii: bool) -> Self {
        self.flags.set(Flags::SHOW_HEXII, show_hexii);
        self
    }

    #[inline]
    pub fn show_ascii(mut self, show_ascii: bool) -> Self {
        self.flags.set(Flags::SHOW_ASCII, show_ascii);
        self.layout = None;
        self
    }

    #[inline]
    pub fn addr_range(mut self, addr_range: RangeInclusive<Addr>) -> Self {
        self.addr_range = addr_range;
        self.selected_addr = self.selected_addr.clamp(addr_range.start, addr_range.end);
        self.layout = None;
        self
    }
}

#[derive(Clone)]
struct Layout {
    data_row_height: f32,
    data_row_height_with_spacing_int: YPos,
    data_row_height_with_spacing: f32,

    glyph_width: f32,
    hex_byte_width: f32,

    hex_col_width_with_spacing: f32,
    ascii_col_width_with_spacing: f32,

    addr_digits: NonZeroU8,

    hex_start_win_x: f32,
    hex_end_win_x: f32,
    ascii_sep_win_x: f32,
    ascii_start_win_x: f32,
    ascii_end_scrollbar_start_win_x: f32,

    item_spacing_x: f32,
    range_width: f32,
    addr_input_width: f32,
    scrollbar_size: f32,

    win_width: f32,
    footer_height: f32,

    total_rows: WAddr,
    data_height_int: YPos,
}

impl MemoryEditor {
    #[inline]
    pub fn visible_addrs(&self, context_rows: Addr) -> RangeInclusive<Addr> {
        (
            self.addr_range.start
                + self.visible_data_rows.start.saturating_sub(context_rows) * self.bytes_per_row,
            self.addr_range
                .start
                .saturating_add(
                    self.visible_data_rows
                        .end
                        .saturating_add(context_rows)
                        .saturating_mul(self.bytes_per_row)
                        .saturating_add(self.bytes_per_row - 1),
                )
                .min(self.addr_range.end),
        )
            .into()
    }

    fn compute_layout(&mut self, ui: &Ui) {
        if self.layout.is_some() {
            return;
        }

        let style = unsafe { ui.style() };

        let data_row_height = ui.text_line_height();
        let data_row_height_with_spacing_int: YPos =
            (data_row_height + style.item_spacing[1]).into();
        let data_row_height_with_spacing = data_row_height_with_spacing_int.into();

        let glyph_width = ui.calc_text_size("0")[0];
        let hex_byte_width = glyph_width * 2.0;

        let hex_col_width = hex_byte_width * self.col_size.get() as f32;
        let ascii_col_width = glyph_width * self.col_size.get() as f32;
        let data_col_spacing = style.item_spacing[0];
        let hex_col_width_with_spacing = hex_col_width + data_col_spacing;
        let ascii_col_width_with_spacing = ascii_col_width + data_col_spacing;

        let addr_digits = self.addr_digits.unwrap_or_else(|| {
            NonZeroU8::new(((67 - self.addr_range.end.leading_zeros() as u8) >> 2).max(1)).unwrap()
        });
        let data_addr_width = ui.calc_text_size(str_buf!(
            self.str_buffer,
            "{:0addr_digits$X}:",
            0,
            addr_digits = addr_digits.get() as usize
        ))[0];

        let v_spacer_width = style.item_spacing[0].max(glyph_width);

        let hex_start_win_x = data_addr_width + v_spacer_width;
        let hex_end_win_x = hex_start_win_x
            + hex_col_width_with_spacing * (self.cols.get() - 1) as f32
            + hex_col_width;
        let (ascii_sep_win_x, ascii_start_win_x, ascii_end_scrollbar_start_win_x) =
            if self.flags.contains(Flags::SHOW_ASCII) {
                let ascii_sep_width = v_spacer_width;
                let ascii_start_win_x = hex_end_win_x + ascii_sep_width;
                (
                    hex_end_win_x + ascii_sep_width * 0.5 - 0.5,
                    ascii_start_win_x,
                    ascii_start_win_x + ascii_col_width_with_spacing * self.cols.get() as f32,
                )
            } else {
                let adjusted_hex_end = hex_end_win_x + data_col_spacing;
                (adjusted_hex_end, adjusted_hex_end, adjusted_hex_end)
            };
        let win_width = ascii_end_scrollbar_start_win_x + style.scrollbar_size;

        let mut x_remaining = win_width;
        let mut line_height = 0.0;
        let mut footer_height = style.item_spacing[1] * 2.0;
        if self.flags.contains(Flags::SHOW_VIEW_OPTIONS) {
            let options_size = {
                let size = ui.calc_text_size("Options...");
                [
                    size[0] + style.frame_padding[0] * 2.0,
                    size[1] + style.frame_padding[1] * 2.0,
                ]
            };
            line_height = options_size[1];
            x_remaining -= options_size[0] + style.item_spacing[0];
        }
        let range_width = if self.flags.contains(Flags::SHOW_RANGE) {
            let range_size = ui.calc_text_size(str_buf!(
                self.str_buffer,
                "Range {:0addr_digits$X}..{:0addr_digits$X}",
                self.addr_range.start,
                self.addr_range.end,
                addr_digits = addr_digits.get() as usize
            ));
            if x_remaining < range_size[0] {
                x_remaining = win_width;
                footer_height += line_height + style.item_spacing[1];
                line_height = 0.0;
            }
            line_height = line_height.max(range_size[1]);
            x_remaining -= range_size[0] + style.item_spacing[0];
            range_size[0]
        } else {
            0.0
        };
        let addr_input_width = {
            let addr_text_size = ui.calc_text_size(str_buf!(
                self.str_buffer,
                "{:0addr_digits$X}",
                0,
                addr_digits = addr_digits.get() as usize
            ));
            let addr_size = [
                addr_text_size[0] + style.frame_padding[0] * 2.0,
                addr_text_size[1] + style.frame_padding[1] * 2.0,
            ];
            if x_remaining < addr_size[0] {
                footer_height += line_height + style.item_spacing[1];
            }
            line_height = line_height.max(addr_size[1]);
            addr_size[0]
        };
        footer_height += line_height;

        let total_rows = ((self.addr_range.end - self.addr_range.start) as WAddr + 1)
            / self.bytes_per_row as WAddr;
        let data_height_int = data_row_height_with_spacing_int * total_rows as YPosRaw;

        self.layout = Some(Layout {
            data_row_height,
            data_row_height_with_spacing_int,
            data_row_height_with_spacing,

            glyph_width,
            hex_byte_width,

            hex_col_width_with_spacing,
            ascii_col_width_with_spacing,

            addr_digits,

            hex_start_win_x,
            hex_end_win_x,
            ascii_sep_win_x,
            ascii_start_win_x,
            ascii_end_scrollbar_start_win_x,

            item_spacing_x: style.item_spacing[0],
            range_width,
            addr_input_width,
            scrollbar_size: style.scrollbar_size,

            win_width,
            footer_height,

            total_rows,
            data_height_int,
        });
    }

    #[inline]
    pub fn window_width(&mut self, ui: &Ui) -> f32 {
        self.compute_layout(ui);
        let layout = self.layout.as_ref().unwrap();
        layout.ascii_end_scrollbar_start_win_x
            + layout.scrollbar_size
            + unsafe { ui.style().window_padding[0] } * 2.0
    }

    fn focus_on_selected_addr(&mut self, ui: &Ui) {
        let layout = self.layout.as_ref().unwrap();
        let content_height = ui.window_size()[1];

        let selected_row = self.selected_addr / self.bytes_per_row;
        let selection_start_scroll =
            layout.data_row_height_with_spacing_int * selected_row as YPosRaw;

        if self.scrollbar.scroll >= selection_start_scroll {
            self.scrollbar.scroll = selection_start_scroll;
        } else {
            let selection_end_scroll_minus_content_height = (selection_start_scroll
                + layout.data_row_height_with_spacing_int)
                .saturating_sub(content_height.into());
            if self.scrollbar.scroll <= selection_end_scroll_minus_content_height {
                self.scrollbar.scroll = selection_end_scroll_minus_content_height;
            }
        }
    }

    pub fn set_selected_addr(&mut self, addr: Addr) {
        self.selected_addr = addr.clamp(self.addr_range.start, self.addr_range.end);
        self.selected_addr_changed = true;
    }

    #[inline]
    pub fn handle_options_right_click(&mut self, ui: &Ui) {
        if self.flags.contains(Flags::SHOW_VIEW_OPTIONS)
            && ui.is_window_focused_with_flags(WindowFocusedFlags::ROOT_AND_CHILD_WINDOWS)
            && ui.is_mouse_clicked(MouseButton::Right)
        {
            ui.open_popup("options");
        }
    }

    #[inline]
    pub fn draw_buffer(&mut self, ui: &Ui, window_title: Option<&str>, buffer: &mut [u8]) {
        assert!(
            buffer.len() as WAddr == (self.addr_range.end - self.addr_range.start) as WAddr + 1
        );
        let base_addr = self.addr_range.start;
        self.draw_callbacks(
            ui,
            window_title,
            buffer,
            move |buffer, addr| Some(buffer[(addr - base_addr) as usize]),
            // move |buffer, addr, value| buffer[(addr - base_addr) as usize] = value,
        );
    }

    pub fn draw_callbacks<T: ?Sized>(
        &mut self,
        ui: &Ui,
        window_title: Option<&str>,
        cb_data: &mut T,
        mut read: impl FnMut(&mut T, Addr) -> Option<u8>,
        // mut write: impl FnMut(&mut T, Addr, u8),
    ) {
        self.compute_layout(ui);

        let window_token = if let Some(window_title) = window_title {
            let layout = self.layout.as_ref().unwrap();
            let token = if let Some(token) = ui
                .window(window_title)
                .size_constraints([layout.win_width, -1.0], [layout.win_width, -1.0])
                .begin()
            {
                token
            } else {
                return;
            };
            self.handle_options_right_click(ui);
            Some(token)
        } else {
            None
        };

        let layout = self.layout.as_ref().unwrap();
        let mut invalidate_layout = false;

        let frame_padding = ui.push_style_var(StyleVar::FramePadding([0.0; 2]));
        let item_spacing = ui.push_style_var(StyleVar::ItemSpacing([0.0; 2]));

        ui.child_window("##memory")
            .movable(false)
            .no_nav()
            .scroll_bar(false)
            .focused(self.selected_addr_changed)
            .size([
                layout.win_width,
                ui.content_region_avail()[1] - layout.footer_height,
            ])
            .build(|| {
                let layout = self.layout.as_ref().unwrap();

                let win_height_int = YPos::from(ui.window_size()[1]);
                let scroll_max_int = layout.data_height_int - win_height_int;

                self.scrollbar.scroll = if ui.is_window_hovered() {
                    self.scrollbar.scroll.as_signed()
                        - SignedYPos::from(ui.io().mouse_wheel * 3.0)
                            * layout.data_row_height_with_spacing_int.as_signed()
                } else {
                    self.scrollbar.scroll.as_signed()
                }
                .min(scroll_max_int.as_signed())
                .max(SignedYPos(0))
                .as_unsigned();

                {
                    let mut new_addr = None;
                    if ui.is_window_focused() {
                        if ui.is_key_pressed(Key::UpArrow) {
                            new_addr = Some(self.selected_addr.saturating_sub(self.bytes_per_row));
                        }
                        if ui.is_key_pressed(Key::DownArrow) {
                            new_addr = Some(self.selected_addr.saturating_add(self.bytes_per_row));
                        }
                        if ui.is_key_pressed(Key::LeftArrow) {
                            new_addr = Some(self.selected_addr.saturating_sub(1));
                        }
                        if ui.is_key_pressed(Key::RightArrow) {
                            new_addr = Some(self.selected_addr.saturating_add(1));
                        }
                    }
                    if let Some(new_addr) = new_addr {
                        self.set_selected_addr(new_addr);
                    }
                }

                let layout = self.layout.as_ref().unwrap();

                let win_pos = ui.window_pos();
                let mouse_pos = ui.io().mouse_pos;

                let hex_start_screen_x = win_pos[0] + layout.hex_start_win_x;
                let ascii_start_screen_x = win_pos[0] + layout.ascii_start_win_x;

                if ui.is_window_hovered() && ui.is_mouse_clicked(MouseButton::Left) {
                    let scroll_offset_y =
                        f32::from(self.scrollbar.scroll % layout.data_row_height_with_spacing_int);
                    let hex_end_screen_x = layout.hex_end_win_x + win_pos[0];
                    let ascii_end_screen_x = layout.ascii_end_scrollbar_start_win_x + win_pos[0];
                    let row_base = (((mouse_pos[1] - win_pos[1] + scroll_offset_y)
                        / layout.data_row_height_with_spacing)
                        as WAddr
                        + self
                            .scrollbar
                            .scroll
                            .div_into_int(layout.data_row_height_with_spacing_int)
                            as WAddr)
                        * self.bytes_per_row as WAddr;
                    if (hex_start_screen_x..hex_end_screen_x).contains(&mouse_pos[0]) {
                        let rel_x = mouse_pos[0] - hex_start_screen_x;
                        let col = ((rel_x + layout.item_spacing_x * 0.5)
                            / layout.hex_col_width_with_spacing)
                            .min(self.cols.get() as f32) as WAddr;
                        let col_byte = ((rel_x - col as f32 * layout.hex_col_width_with_spacing)
                            / layout.hex_byte_width)
                            .clamp(0.0, (self.col_size.get() - 1) as f32)
                            as WAddr;
                        self.set_selected_addr(
                            (row_base + col * self.col_size.get() as WAddr + col_byte)
                                .min(self.addr_range.end as WAddr)
                                as Addr,
                        );
                    } else if (ascii_start_screen_x..ascii_end_screen_x).contains(&mouse_pos[0]) {
                        let rel_x = mouse_pos[0] - ascii_start_screen_x;
                        let col = ((rel_x + layout.item_spacing_x * 0.5)
                            / layout.ascii_col_width_with_spacing)
                            .min(self.cols.get() as f32) as WAddr;
                        let col_byte = ((rel_x - col as f32 * layout.ascii_col_width_with_spacing)
                            / layout.glyph_width)
                            .clamp(0.0, (self.col_size.get() - 1) as f32)
                            as WAddr;
                        self.set_selected_addr(
                            (row_base + col * self.col_size.get() as WAddr + col_byte)
                                .min(self.addr_range.end as WAddr)
                                as Addr,
                        );
                    }
                }

                if self.selected_addr_changed {
                    self.focus_on_selected_addr(ui);
                }

                let layout = self.layout.as_ref().unwrap();

                self.scrollbar.draw(
                    ui,
                    win_pos[0] + layout.ascii_end_scrollbar_start_win_x,
                    win_pos[1],
                    layout.scrollbar_size,
                    ui.window_size()[1],
                    mouse_pos,
                    win_height_int.div_into_f32(layout.data_height_int),
                    scroll_max_int,
                );

                if self.flags.contains(Flags::SHOW_ASCII) {
                    {
                        let sep_screen_x = win_pos[0] + layout.ascii_sep_win_x;
                        let sep_start_y = win_pos[1];
                        ui.get_window_draw_list()
                            .add_line(
                                [sep_screen_x, sep_start_y],
                                [sep_screen_x, sep_start_y + ui.window_size()[1]],
                                ui.style_color(StyleColor::Border),
                            )
                            .build();
                    }
                }

                let scroll_offset_y_int =
                    self.scrollbar.scroll % layout.data_row_height_with_spacing_int;
                let scroll_offset_y = f32::from(scroll_offset_y_int);
                let content_start_y = win_pos[1] - scroll_offset_y;

                self.visible_data_rows = (
                    self.scrollbar
                        .scroll
                        .div_into_int(layout.data_row_height_with_spacing_int)
                        as Addr,
                    (self.scrollbar.scroll + scroll_offset_y_int + win_height_int)
                        .div_into_int(layout.data_row_height_with_spacing_int)
                        .min((layout.total_rows - 1) as YPosRaw) as Addr,
                )
                    .into();

                let mut cur_base_addr =
                    self.addr_range.start + self.visible_data_rows.start * self.bytes_per_row;
                for rel_row in 0..=self.visible_data_rows.end - self.visible_data_rows.start {
                    let row_start_screen_y =
                        content_start_y + rel_row as f32 * layout.data_row_height_with_spacing;

                    ui.set_cursor_screen_pos([win_pos[0], row_start_screen_y]);
                    ui.text(if self.flags.contains(Flags::UPPERCASE_HEX) {
                        str_buf!(
                            self.str_buffer,
                            "{:0addr_digits$X}:",
                            cur_base_addr,
                            addr_digits = layout.addr_digits.get() as usize
                        )
                    } else {
                        str_buf!(
                            self.str_buffer,
                            "{:0addr_digits$x}:",
                            cur_base_addr,
                            addr_digits = layout.addr_digits.get() as usize
                        )
                    });

                    for col_i in 0..self.cols.get() {
                        let hex_col_start_screen_x =
                            hex_start_screen_x + col_i as f32 * layout.hex_col_width_with_spacing;
                        let ascii_col_start_screen_x = ascii_start_screen_x
                            + col_i as f32 * layout.ascii_col_width_with_spacing;

                        let col_base_addr = if self.flags.contains(Flags::LITTLE_ENDIAN_COLS) {
                            cur_base_addr += self.col_size.get() as Addr;
                            cur_base_addr - 1
                        } else {
                            let addr = cur_base_addr;
                            cur_base_addr = cur_base_addr.wrapping_add(self.col_size.get() as Addr);
                            addr
                        };

                        for byte_i in 0..self.col_size.get() {
                            let addr = if self.flags.contains(Flags::LITTLE_ENDIAN_COLS) {
                                col_base_addr - byte_i as Addr
                            } else {
                                col_base_addr + byte_i as Addr
                            };

                            let hex_byte_start_screen_x =
                                hex_col_start_screen_x + byte_i as f32 * layout.hex_byte_width;
                            let ascii_byte_start_screen_x =
                                ascii_col_start_screen_x + byte_i as f32 * layout.glyph_width;

                            if self.selected_addr == addr {
                                let draw_list = ui.get_window_draw_list();
                                draw_list
                                    .add_rect(
                                        [hex_byte_start_screen_x, row_start_screen_y],
                                        [
                                            hex_byte_start_screen_x + layout.hex_byte_width,
                                            row_start_screen_y + layout.data_row_height,
                                        ],
                                        ui.style_color(StyleColor::TextSelectedBg),
                                    )
                                    .filled(true)
                                    .build();

                                if self.flags.contains(Flags::SHOW_ASCII) {
                                    draw_list
                                        .add_rect(
                                            [ascii_byte_start_screen_x, row_start_screen_y],
                                            [
                                                ascii_byte_start_screen_x + layout.glyph_width,
                                                row_start_screen_y + layout.data_row_height,
                                            ],
                                            ui.style_color(StyleColor::TextSelectedBg),
                                        )
                                        .filled(true)
                                        .build();
                                }
                            }

                            if let Some(data) = read(cb_data, addr) {
                                let text_color = ui.style_color(
                                    if self.flags.contains(Flags::GRAY_OUT_ZEROS) && data == 0 {
                                        StyleColor::TextDisabled
                                    } else {
                                        StyleColor::Text
                                    },
                                );

                                ui.set_cursor_screen_pos([
                                    hex_byte_start_screen_x,
                                    row_start_screen_y,
                                ]);
                                ui.text_colored(
                                    text_color,
                                    if self.flags.contains(Flags::SHOW_HEXII) {
                                        if (0x20..0x7F).contains(&data) {
                                            str_buf!(self.str_buffer, ".{}", data as char)
                                        } else if data == 0 {
                                            str_buf!(self.str_buffer, "  ")
                                        } else if data == 0xFF {
                                            str_buf!(self.str_buffer, "##")
                                        } else if self.flags.contains(Flags::UPPERCASE_HEX) {
                                            str_buf!(self.str_buffer, "{:02X}", data)
                                        } else {
                                            str_buf!(self.str_buffer, "{:02x}", data)
                                        }
                                    } else if self.flags.contains(Flags::UPPERCASE_HEX) {
                                        str_buf!(self.str_buffer, "{:02X}", data)
                                    } else {
                                        str_buf!(self.str_buffer, "{:02x}", data)
                                    },
                                );

                                if self.flags.contains(Flags::SHOW_ASCII) {
                                    ui.set_cursor_screen_pos([
                                        ascii_byte_start_screen_x,
                                        row_start_screen_y,
                                    ]);
                                    ui.text_colored(
                                        text_color,
                                        str_buf!(
                                            self.str_buffer,
                                            "{}",
                                            if (0x20..0x7F).contains(&data) {
                                                data as char
                                            } else {
                                                '.'
                                            }
                                        ),
                                    );
                                }
                            } else {
                                continue;
                            }
                        }
                    }
                }
            });

        drop((item_spacing, frame_padding));

        ui.spacing();
        ui.separator();

        if self.flags.contains(Flags::SHOW_VIEW_OPTIONS) {
            if ui.button("Options...") {
                ui.open_popup("options");
            }

            ui.popup("options", || {
                let mut cols = self.cols.get();
                if Drag::new("##cols")
                    .display_format("Cols: %d")
                    .build(ui, &mut cols)
                {
                    invalidate_layout = true;
                    self.cols = NonZeroU8::new(cols.max(1)).unwrap();
                    self.bytes_per_row = self.calc_bytes_per_row();
                }

                ui.same_line();

                let mut col_size = self.col_size.get();
                if Drag::new("##col_size")
                    .display_format("Col size: %d")
                    .build(ui, &mut col_size)
                {
                    invalidate_layout = true;
                    self.col_size = NonZeroU8::new(col_size.max(1)).unwrap();
                    self.bytes_per_row = self.calc_bytes_per_row();
                }

                ui.checkbox_flags("Gray out zeros", &mut self.flags, Flags::GRAY_OUT_ZEROS);
                ui.checkbox_flags("Uppercase hex", &mut self.flags, Flags::UPPERCASE_HEX);
                ui.checkbox_flags(
                    "Little endian cols",
                    &mut self.flags,
                    Flags::LITTLE_ENDIAN_COLS,
                );
                ui.checkbox_flags("Show HexII", &mut self.flags, Flags::SHOW_HEXII);
                if ui.checkbox_flags("Show ASCII", &mut self.flags, Flags::SHOW_ASCII) {
                    invalidate_layout = true;
                }
            });
            ui.same_line();
        }

        let layout = self.layout.as_ref().unwrap();

        if self.flags.contains(Flags::SHOW_RANGE) {
            if ui.content_region_avail()[0] < layout.range_width {
                ui.new_line();
            }
            ui.text(if self.flags.contains(Flags::UPPERCASE_HEX) {
                str_buf!(
                    self.str_buffer,
                    "Range {:0addr_digits$X}..{:0addr_digits$X}",
                    self.addr_range.start,
                    self.addr_range.end,
                    addr_digits = layout.addr_digits.get() as usize
                )
            } else {
                str_buf!(
                    self.str_buffer,
                    "Range {:0addr_digits$x}..{:0addr_digits$x}",
                    self.addr_range.start,
                    self.addr_range.end,
                    addr_digits = layout.addr_digits.get() as usize
                )
            });
            ui.same_line();
        }

        if ui.content_region_avail()[0] < layout.addr_input_width {
            ui.new_line();
        }
        ui.set_next_item_width(layout.addr_input_width);

        if self.selected_addr_changed {
            self.addr_input.clear();
            write!(
                self.addr_input,
                "{:0addr_digits$X}",
                self.selected_addr,
                addr_digits = layout.addr_digits.get() as usize
            )
            .unwrap();
        }

        self.selected_addr_changed = false;
        if ui
            .input_text("##address", &mut self.addr_input)
            .auto_select_all(true)
            .chars_hexadecimal(true)
            .enter_returns_true(true)
            .no_horizontal_scroll(true)
            .build()
        {
            if let Ok(addr) = Addr::from_str_radix(&self.addr_input, 16) {
                self.set_selected_addr(addr);
            }
        };

        if invalidate_layout {
            self.layout = None;
        }

        if let Some(token) = window_token {
            token.end();
        }
    }
}
