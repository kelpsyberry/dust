use super::{
    y_pos::{SignedYPos, YPos, YPosRaw},
    RangeInclusive, Scrollbar,
};
use imgui::{Key, MouseButton, StyleColor, StyleVar, Ui, WindowHoveredFlags};
use std::{fmt::Write, num::NonZeroU8};

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Flags: u8 {
        // Creation flags
        const SHOW_VIEW_OPTIONS = 1 << 0;
        const SHOW_RANGE = 1 << 1;

        // Options
        const UPPERCASE_HEX = 1 << 2;
    }
}

pub type Addr = u64;
pub type WAddr = u128;

pub struct DisassemblyView {
    bytes_per_line: Addr,
    addr_digits: Option<NonZeroU8>,
    flags: Flags,
    addr_range: RangeInclusive<Addr>,

    scrollbar: Scrollbar,
    visible_disasm_lines: RangeInclusive<Addr>,

    selected_addr: Addr,
    addr_input: String,
    selected_addr_changed: bool,

    str_buffer: String,
    last_win_width: f32,
    layout: Option<Layout>,
}

impl DisassemblyView {
    #[inline]
    pub fn new() -> Self {
        DisassemblyView {
            addr_digits: None,
            bytes_per_line: 1,
            flags: Flags::SHOW_VIEW_OPTIONS | Flags::SHOW_RANGE | Flags::UPPERCASE_HEX,
            addr_range: (0, 0).into(),

            scrollbar: Scrollbar::new(),
            visible_disasm_lines: (0, 0).into(),

            selected_addr: 0,
            addr_input: String::new(),
            selected_addr_changed: true,

            str_buffer: String::new(),
            last_win_width: 0.0,
            layout: None,
        }
    }

    #[inline]
    pub fn bytes_per_line(mut self, bytes_per_line: Addr) -> Self {
        self.set_bytes_per_line(bytes_per_line);
        self
    }

    #[inline]
    pub fn set_bytes_per_line(&mut self, bytes_per_line: Addr) {
        self.bytes_per_line = bytes_per_line;
        self.layout = None;
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
    pub fn uppercase_hex(mut self, uppercase_hex: bool) -> Self {
        self.flags.set(Flags::UPPERCASE_HEX, uppercase_hex);
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
    disasm_line_height: f32,
    disasm_line_height_with_spacing_int: YPos,
    disasm_line_height_with_spacing: f32,

    addr_digits: NonZeroU8,
    addr_width: f32,

    opcodes_start_win_x: f32,
    scrollbar_start_win_x: f32,

    range_width: f32,
    addr_input_width: f32,
    scrollbar_size: f32,

    footer_height: f32,

    total_lines: WAddr,
    disasm_height_int: YPos,
}

impl DisassemblyView {
    #[inline]
    pub fn visible_addrs(&self, context_lines: Addr) -> RangeInclusive<Addr> {
        (
            self.addr_range.start
                + self
                    .visible_disasm_lines
                    .start
                    .saturating_sub(context_lines)
                    * self.bytes_per_line,
            self.addr_range
                .start
                .saturating_add(
                    self.visible_disasm_lines
                        .end
                        .saturating_add(context_lines)
                        .saturating_mul(self.bytes_per_line)
                        .saturating_add(self.bytes_per_line - 1),
                )
                .min(self.addr_range.end),
        )
            .into()
    }

    fn compute_layout(&mut self, ui: &Ui) {
        if self.layout.is_some() {
            return;
        }

        let item_spacing = style!(ui, item_spacing);
        let frame_padding = style!(ui, frame_padding);
        let scrollbar_size = style!(ui, scrollbar_size);

        let disasm_line_height = ui.text_line_height();
        let disasm_line_height_with_spacing_int: YPos =
            (disasm_line_height + item_spacing[1]).into();
        let disasm_line_height_with_spacing = disasm_line_height_with_spacing_int.into();

        let addr_digits = self.addr_digits.unwrap_or_else(|| {
            NonZeroU8::new(((67 - self.addr_range.end.leading_zeros() as u8) >> 2).max(1)).unwrap()
        });
        let addr_width = ui.calc_text_size(str_buf!(
            self.str_buffer,
            "{:0addr_digits$X}:",
            0,
            addr_digits = addr_digits.get() as usize
        ))[0];

        let v_spacer_width = item_spacing[0].max(ui.calc_text_size("0")[0]);

        let opcodes_start_win_x = addr_width + v_spacer_width;
        let win_width = ui.window_content_region_max()[0] - ui.window_content_region_min()[0];
        let scrollbar_start_win_x = win_width - scrollbar_size;

        let mut x_remaining = win_width;
        let mut line_height = 0.0;
        let mut footer_height = item_spacing[1] * 2.0;
        if self.flags.contains(Flags::SHOW_VIEW_OPTIONS) {
            let options_size = {
                let size = ui.calc_text_size("Options...");
                [
                    size[0] + frame_padding[0] * 2.0,
                    size[1] + frame_padding[1] * 2.0,
                ]
            };
            line_height = options_size[1];
            x_remaining -= options_size[0] + item_spacing[0];
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
                footer_height += line_height + item_spacing[1];
                line_height = 0.0;
            }
            line_height = line_height.max(range_size[1]);
            x_remaining -= range_size[0] + item_spacing[0];
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
                addr_text_size[0] + frame_padding[0] * 2.0,
                addr_text_size[1] + frame_padding[1] * 2.0,
            ];
            if x_remaining < addr_size[0] {
                footer_height += line_height + item_spacing[1];
            }
            line_height = line_height.max(addr_size[1]);
            addr_size[0]
        };
        footer_height += line_height;

        let total_lines = ((self.addr_range.end - self.addr_range.start) as WAddr + 1)
            / self.bytes_per_line as WAddr;
        let disasm_height_int = disasm_line_height_with_spacing_int * total_lines as YPosRaw;

        self.layout = Some(Layout {
            disasm_line_height,
            disasm_line_height_with_spacing_int,
            disasm_line_height_with_spacing,

            addr_digits,
            addr_width,

            opcodes_start_win_x,
            scrollbar_start_win_x,

            range_width,
            addr_input_width,
            scrollbar_size,

            footer_height,

            total_lines,
            disasm_height_int,
        });
    }

    fn focus_on_selected_addr(&mut self, ui: &Ui) {
        let layout = self.layout.as_ref().unwrap();
        let content_height = ui.window_size()[1];

        let selected_line = self.selected_addr / self.bytes_per_line;
        let selection_start_scroll =
            layout.disasm_line_height_with_spacing_int * selected_line as YPosRaw;

        if self.scrollbar.scroll >= selection_start_scroll {
            self.scrollbar.scroll = selection_start_scroll;
        } else {
            let selection_end_scroll_minus_content_height = (selection_start_scroll
                + layout.disasm_line_height_with_spacing_int)
                .saturating_sub(content_height.into());
            if self.scrollbar.scroll <= selection_end_scroll_minus_content_height {
                self.scrollbar.scroll = selection_end_scroll_minus_content_height;
            }
        }
    }

    pub fn set_selected_addr(&mut self, addr: Addr) {
        self.selected_addr = addr.clamp(self.addr_range.start, self.addr_range.end);
        self.selected_addr -= (self.selected_addr - self.addr_range.start) % self.bytes_per_line;
        self.selected_addr_changed = true;
    }

    #[inline]
    pub fn handle_options_right_click(&mut self, ui: &Ui) {
        if self.flags.contains(Flags::SHOW_VIEW_OPTIONS)
            && ui.is_window_hovered_with_flags(WindowHoveredFlags::ROOT_AND_CHILD_WINDOWS)
            && ui.is_mouse_clicked(MouseButton::Right)
        {
            ui.open_popup("options");
        }
    }

    pub fn draw_callbacks<T: ?Sized>(
        &mut self,
        ui: &Ui,
        window_title: Option<&str>,
        cb_data: &mut T,
        mut read: impl FnMut(&Ui, &mut T, Addr),
    ) {
        let window_token = if let Some(window_title) = window_title {
            let token = if let Some(token) = ui.window(window_title).begin() {
                token
            } else {
                return;
            };
            self.handle_options_right_click(ui);
            Some(token)
        } else {
            None
        };

        let win_content_size = ui.content_region_avail();

        if win_content_size[0] != self.last_win_width {
            self.last_win_width = win_content_size[0];
            self.layout = None;
        }

        self.compute_layout(ui);

        let frame_padding = ui.push_style_var(StyleVar::FramePadding([0.0; 2]));
        let item_spacing = ui.push_style_var(StyleVar::ItemSpacing([0.0; 2]));

        let layout = self.layout.as_ref().unwrap();

        ui.child_window("disasm")
            .no_nav()
            .scroll_bar(false)
            .focused(self.selected_addr_changed)
            .size([
                win_content_size[0],
                win_content_size[1] - layout.footer_height,
            ])
            .build(|| {
                let layout = self.layout.as_ref().unwrap();

                let win_height_int = YPos::from(ui.window_size()[1]);
                let scroll_max_int = layout.disasm_height_int - win_height_int;

                self.scrollbar.scroll = if ui.is_window_hovered() {
                    self.scrollbar.scroll.as_signed()
                        - SignedYPos::from(ui.io().mouse_wheel * 3.0)
                            * layout.disasm_line_height_with_spacing_int.as_signed()
                } else {
                    self.scrollbar.scroll.as_signed()
                }
                .clamp(SignedYPos(0), scroll_max_int.as_signed())
                .as_unsigned();

                if ui.is_window_focused() {
                    if ui.is_key_pressed(Key::UpArrow) || ui.is_key_pressed(Key::LeftArrow) {
                        self.set_selected_addr(
                            self.selected_addr.saturating_sub(self.bytes_per_line),
                        );
                    }
                    if ui.is_key_pressed(Key::DownArrow) || ui.is_key_pressed(Key::RightArrow) {
                        self.set_selected_addr(
                            self.selected_addr.saturating_add(self.bytes_per_line),
                        );
                    }
                }

                let layout = self.layout.as_ref().unwrap();

                let win_pos = ui.window_pos();
                let mouse_pos = ui.io().mouse_pos;

                let opcodes_start_screen_x = win_pos[0] + layout.opcodes_start_win_x;

                if ui.is_window_hovered() && ui.is_mouse_clicked(MouseButton::Left) {
                    let scroll_offset_y = f32::from(
                        self.scrollbar.scroll % layout.disasm_line_height_with_spacing_int,
                    );
                    let opcodes_end_screen_x = layout.scrollbar_start_win_x + win_pos[0];
                    let row_base = (((mouse_pos[1] - win_pos[1] + scroll_offset_y)
                        / layout.disasm_line_height_with_spacing)
                        as WAddr
                        + self
                            .scrollbar
                            .scroll
                            .div_into_int(layout.disasm_line_height_with_spacing_int)
                            as WAddr)
                        * self.bytes_per_line as WAddr;
                    if (win_pos[0]..opcodes_end_screen_x).contains(&mouse_pos[0]) {
                        self.set_selected_addr(row_base.min(self.addr_range.end as WAddr) as Addr);
                    }
                }

                if self.selected_addr_changed {
                    self.focus_on_selected_addr(ui);
                }

                let layout = self.layout.as_ref().unwrap();

                self.scrollbar.draw(
                    ui,
                    win_pos[0] + layout.scrollbar_start_win_x,
                    win_pos[1],
                    layout.scrollbar_size,
                    ui.window_size()[1],
                    mouse_pos,
                    win_height_int.div_into_f32(layout.disasm_height_int),
                    scroll_max_int,
                );

                let scroll_offset_y_int =
                    self.scrollbar.scroll % layout.disasm_line_height_with_spacing_int;
                let scroll_offset_y = f32::from(scroll_offset_y_int);
                let content_start_y = win_pos[1] - scroll_offset_y;

                self.visible_disasm_lines = (
                    self.scrollbar
                        .scroll
                        .div_into_int(layout.disasm_line_height_with_spacing_int)
                        as Addr,
                    (self.scrollbar.scroll + scroll_offset_y_int + win_height_int)
                        .div_into_int(layout.disasm_line_height_with_spacing_int)
                        .min((layout.total_lines - 1) as YPosRaw) as Addr,
                )
                    .into();

                let mut addr =
                    self.addr_range.start + self.visible_disasm_lines.start * self.bytes_per_line;
                for rel_line in 0..=self.visible_disasm_lines.end - self.visible_disasm_lines.start
                {
                    let row_start_screen_y =
                        content_start_y + rel_line as f32 * layout.disasm_line_height_with_spacing;

                    ui.set_cursor_screen_pos([win_pos[0], row_start_screen_y]);
                    ui.text(if self.flags.contains(Flags::UPPERCASE_HEX) {
                        str_buf!(
                            self.str_buffer,
                            "{:0addr_digits$X}:",
                            addr,
                            addr_digits = layout.addr_digits.get() as usize
                        )
                    } else {
                        str_buf!(
                            self.str_buffer,
                            "{:0addr_digits$x}:",
                            addr,
                            addr_digits = layout.addr_digits.get() as usize
                        )
                    });

                    if self.selected_addr == addr {
                        let draw_list = ui.get_window_draw_list();
                        draw_list
                            .add_rect(
                                [win_pos[0], row_start_screen_y],
                                [
                                    win_pos[0] + layout.addr_width,
                                    row_start_screen_y + layout.disasm_line_height,
                                ],
                                ui.style_color(StyleColor::TextSelectedBg),
                            )
                            .filled(true)
                            .build();
                    }

                    ui.set_cursor_screen_pos([opcodes_start_screen_x, row_start_screen_y]);
                    read(ui, cb_data, addr);

                    addr = addr.wrapping_add(self.bytes_per_line);
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
                ui.checkbox_flags("Uppercase hex", &mut self.flags, Flags::UPPERCASE_HEX);
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

        if let Some(token) = window_token {
            token.end();
        }
    }
}
