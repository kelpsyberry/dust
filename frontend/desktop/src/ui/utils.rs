use crate::config::{self, File};
use imgui::{StyleColor, Ui};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    ops::{Add, Mul, Sub},
    path::Path,
};

macro_rules! config_warning {
    (yes_no, $($desc: tt)*) => {
        warning!(yes_no, "Configuration warning", $($desc)*)
    };
    ($($desc: tt)*) => {
        warning!("Configuration warning", $($desc)*)
    };
}

macro_rules! config_error {
    (yes_no, $($desc: tt)*) => {
        error!(yes_no, "Configuration error", $($desc)*)
    };
    ($($desc: tt)*) => {
        error!("Configuration error", $($desc)*)
    };
}

impl<T: Default + Serialize + for<'de> Deserialize<'de>> File<T> {
    pub(super) fn read_or_show_dialog(dir_path: &Path, filename: &str) -> Self {
        let path = dir_path.join(filename);
        match File::read(&path, true) {
            Ok(config) => config,
            Err(err) => {
                let should_overwrite = match err {
                    config::FileError::Io(err) => {
                        config_error!(
                            "Couldn't read `{}`: {}\n\nThe default values will be used, new \
                             changes will not be saved.",
                            path.display(),
                            err,
                        );
                        false
                    }
                    config::FileError::Json(err) => config_error!(
                        yes_no,
                        "Couldn't parse `{}`: {}\n\nOverwrite the existing configuration file \
                         with the default values?",
                        path.display(),
                        err,
                    ),
                };
                File {
                    contents: T::default(),
                    path: should_overwrite.then(|| path.to_path_buf()),
                }
            }
        }
    }
}

#[allow(dead_code)]
pub fn scale_to_fit(aspect_ratio: f32, frame_size: [f32; 2]) -> ([f32; 2], [f32; 2]) {
    let width = (frame_size[1] * aspect_ratio).min(frame_size[0]);
    let height = width / aspect_ratio;
    (
        mul2s(sub2(frame_size, [width, height]), 0.5),
        [width, height],
    )
}

pub fn scale_to_fit_rotated(
    orig_size: [f32; 2],
    integer_scale: bool,
    rot: f32,
    frame_size: [f32; 2],
) -> ([f32; 2], [[f32; 2]; 4]) {
    let half_frame_size = mul2s(frame_size, 0.5);
    let (sin, cos) = rot.sin_cos();
    let mut scale = f32::INFINITY;
    let rotate_and_get_scale = |[x, y]: [f32; 2]| {
        let rot_x = x * cos - y * sin;
        let rot_y = x * sin + y * cos;
        scale = scale
            .min(half_frame_size[0] / rot_x.abs())
            .min(half_frame_size[1] / rot_y.abs());
        [rot_x, rot_y]
    };
    let half_size = mul2s(orig_size, 0.5);
    let rotated_rel_points = [
        [-half_size[0], -half_size[1]],
        [half_size[0], -half_size[1]],
        half_size,
        [-half_size[0], half_size[1]],
    ]
    .map(rotate_and_get_scale);
    if integer_scale && scale > 1.0 {
        scale = scale.floor();
    }
    (
        half_frame_size,
        rotated_rel_points.map(|point| add2(mul2s(point, scale), half_frame_size)),
    )
}

pub fn add_y_spacing(ui: &Ui, spacing: f32) {
    let mut cursor_pos = ui.cursor_screen_pos();
    cursor_pos[1] += spacing;
    ui.set_cursor_screen_pos(cursor_pos);
}

#[allow(clippy::too_many_arguments)]
pub fn heading_options_custom(
    ui: &Ui,
    inner_indent: f32,
    line_inner_margin: f32,
    line_outer_margin_start: f32,
    line_outer_margin_end: f32,
    line_thickness: f32,
    width: f32,
    height: f32,
    inner_height: f32,
    remove_item_spacing: bool,
    draw: impl FnOnce(),
) {
    let half_line_thickness = line_thickness * 0.5;

    let height = height.max(inner_height);

    let mut cursor_pos = ui.cursor_screen_pos();
    if remove_item_spacing {
        cursor_pos[1] -= style!(ui, item_spacing)[1];
    }
    let mut end_pos = [cursor_pos[0], cursor_pos[1] + height];
    if !remove_item_spacing {
        end_pos[1] += style!(ui, item_spacing)[1];
    }

    let line_outer_bounds = sub2s(
        [
            cursor_pos[0] + line_outer_margin_start,
            cursor_pos[0] + width - line_outer_margin_end,
        ],
        half_line_thickness,
    );
    let separator_color = ui.style_color(StyleColor::Separator);

    let mid_y = cursor_pos[1] + height * 0.5;
    let inner_start_x = cursor_pos[0] + inner_indent;
    let inner_start_y = mid_y - inner_height * 0.5;
    let line_y = mid_y - half_line_thickness;

    ui.set_cursor_screen_pos([inner_start_x, inner_start_y]);
    draw();
    ui.same_line_with_spacing(0.0, 0.0);
    let inner_end_x = ui.cursor_screen_pos()[0];

    let draw_list = ui.get_window_draw_list();
    draw_list
        .add_line(
            [line_outer_bounds[0], line_y],
            [
                inner_start_x - line_inner_margin - half_line_thickness,
                line_y,
            ],
            separator_color,
        )
        .thickness(line_thickness)
        .build();
    draw_list
        .add_line(
            [line_outer_bounds[1], line_y],
            [
                inner_end_x + line_inner_margin - half_line_thickness,
                line_y,
            ],
            separator_color,
        )
        .thickness(line_thickness)
        .build();
    ui.set_cursor_screen_pos(end_pos);
}

#[allow(clippy::too_many_arguments)]
pub fn heading_options(
    ui: &Ui,
    text: &str,
    text_indent: f32,
    line_inner_margin: f32,
    line_outer_margin_start: f32,
    line_outer_margin_end: f32,
    line_thickness: f32,
    width: f32,
    height: f32,
    remove_item_spacing: bool,
) {
    heading_options_custom(
        ui,
        text_indent,
        line_inner_margin,
        line_outer_margin_start,
        line_outer_margin_end,
        line_thickness,
        width,
        height,
        ui.text_line_height(),
        remove_item_spacing,
        || ui.text(text),
    );
}

pub fn table_row_heading(
    ui: &Ui,
    text: &str,
    text_indent: f32,
    line_inner_margin: f32,
    line_outer_margin: f32,
    line_thickness: f32,
    spacing: f32,
) {
    ui.table_next_row();
    ui.table_set_column_index(ui.table_column_count() - 1);
    let end_x = ui.cursor_screen_pos()[0] + ui.content_region_avail()[0];
    ui.table_set_column_index(0);

    let mut cursor_pos = ui.cursor_screen_pos();
    cursor_pos[1] += spacing;
    ui.set_cursor_screen_pos(cursor_pos);
    let width = end_x - cursor_pos[0];

    heading_options(
        ui,
        text,
        text_indent,
        line_inner_margin,
        line_outer_margin,
        line_outer_margin,
        line_thickness,
        width,
        0.0,
        false,
    );
}

pub fn heading(ui: &Ui, text: &str, text_indent: f32, line_inner_margin: f32, line_thickness: f32) {
    heading_options(
        ui,
        text,
        text_indent,
        line_inner_margin,
        0.0,
        0.0,
        line_thickness,
        ui.content_region_avail()[0],
        0.0,
        false,
    );
}

pub fn combo_value<T: PartialEq + Clone, L: for<'a> Fn(&'a T) -> Cow<'a, str>>(
    ui: &Ui,
    label: impl AsRef<str>,
    current_item: &mut T,
    items: &[T],
    label_fn: L,
) -> bool {
    let mut i = items
        .iter()
        .position(|item| item == current_item)
        .unwrap_or(usize::MAX);
    if ui.combo(label, &mut i, items, label_fn) {
        *current_item = items[i].clone();
        true
    } else {
        false
    }
}

#[inline]
pub fn add2<T: Add<Output = T>>([a0, a1]: [T; 2], [b0, b1]: [T; 2]) -> [T; 2] {
    [a0 + b0, a1 + b1]
}

#[inline]
pub fn sub2<T: Sub<Output = T>>([a0, a1]: [T; 2], [b0, b1]: [T; 2]) -> [T; 2] {
    [a0 - b0, a1 - b1]
}

#[inline]
pub fn sub2s<T: Sub<Output = T> + Copy>([a0, a1]: [T; 2], b: T) -> [T; 2] {
    [a0 - b, a1 - b]
}

#[inline]
pub fn mul2s<T: Mul<Output = T> + Copy>([a0, a1]: [T; 2], b: T) -> [T; 2] {
    [a0 * b, a1 * b]
}
