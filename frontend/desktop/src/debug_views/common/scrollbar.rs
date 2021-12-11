use super::y_pos::{SignedYPos, YPos};
use imgui::{MouseButton, StyleColor, Ui};

pub struct Scrollbar {
    pub scroll: YPos,
    grabbing: bool,
    grab_start_y: f32,
    grab_start_scroll: YPos,
}

impl Scrollbar {
    #[inline]
    pub fn new() -> Self {
        Scrollbar {
            scroll: YPos(0),
            grabbing: false,
            grab_start_y: 0.0,
            grab_start_scroll: YPos(0),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        ui: &Ui,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        mouse_pos: [f32; 2],
        mut grab_height: f32,
        scroll_max_int: YPos,
    ) {
        grab_height = (grab_height * height).max(width);
        let grab_start =
            y + (height - grab_height).max(0.0) * self.scroll.div_into_f32(scroll_max_int);
        let grab_end = (grab_start + grab_height).min(y + height);
        grab_height = grab_end - grab_start;
        let draw_list = ui.get_window_draw_list();
        if ui.is_mouse_released(MouseButton::Left) {
            self.grabbing = false;
        }
        let hovered = ui.is_window_hovered()
            && (x..x + width).contains(&mouse_pos[0])
            && (y..y + height).contains(&mouse_pos[1]);
        let mut process_grab = self.grabbing;
        if hovered && ui.is_mouse_clicked(MouseButton::Left) {
            self.grabbing = true;
            process_grab = if (grab_start..grab_end).contains(&mouse_pos[1]) {
                true
            } else {
                let new_scroll_ratio = ((mouse_pos[1] - y - grab_height * 0.5)
                    / (height - grab_height))
                    .clamp(0.0, 1.0);
                self.scroll = scroll_max_int * new_scroll_ratio;
                false
            };
            self.grab_start_y = mouse_pos[1];
            self.grab_start_scroll = self.scroll;
        };
        if process_grab {
            let delta = (mouse_pos[1] - self.grab_start_y) / (height - grab_height);
            self.scroll = (self.grab_start_scroll.as_signed()
                + scroll_max_int.as_signed() * SignedYPos::from(delta))
            .min(scroll_max_int.as_signed())
            .max(SignedYPos(0))
            .as_unsigned();
        }
        let grab_style_color = if self.grabbing {
            StyleColor::ScrollbarGrabActive
        } else if hovered {
            StyleColor::ScrollbarGrabHovered
        } else {
            StyleColor::ScrollbarGrab
        };
        draw_list
            .add_rect(
                [x, y],
                [x + width, y + height],
                ui.style_color(StyleColor::ScrollbarBg),
            )
            .filled(true)
            .build();
        draw_list
            .add_rect(
                [x, grab_start],
                [x + width, grab_end],
                ui.style_color(grab_style_color),
            )
            .filled(true)
            .rounding(width * 0.5)
            .build();
    }
}
