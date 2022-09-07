use crate::config::{self, File};
use imgui::{StyleColor, Ui};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, path::Path};

macro_rules! format_list {
    ($list: expr) => {
        $list.into_iter().fold(String::new(), |mut acc, v| {
            #[allow(unused_imports)]
            use std::fmt::Write;
            let _ = write!(acc, "\n- {v}");
            acc
        })
    };
}

macro_rules! location_str {
    ($path: expr) => {
        if let Some(path_str) = $path.to_str() {
            format!(" at `{path_str}`")
        } else {
            String::new()
        }
    };
}

macro_rules! warning {
    (yes_no, $title: expr, $($desc: tt)*) => {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title($title)
            .set_description(&format!($($desc)*))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
    };
    ($title: expr, $($desc: tt)*) => {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title($title)
            .set_description(&format!($($desc)*))
            .set_buttons(rfd::MessageButtons::Ok)
            .show()
    };
}

macro_rules! error {
    (yes_no, $title: expr, $($desc: tt)*) => {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title($title)
            .set_description(&format!($($desc)*))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
    };
    ($title: expr, $($desc: tt)*) => {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title($title)
            .set_description(&format!($($desc)*))
            .set_buttons(rfd::MessageButtons::Ok)
            .show()
    };
}

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
                let path_str = match path.to_str() {
                    Some(path_str) => format!("`{path_str}`"),
                    None => filename.to_string(),
                };
                let save = match err {
                    config::FileError::Io(err) => {
                        config_error!(
                            "Couldn't read `{}`: {}\n\nThe default values will be used, new \
                             changes will not be saved.",
                            path_str,
                            err,
                        );
                        false
                    }
                    config::FileError::Json(err) => config_error!(
                        yes_no,
                        "Couldn't parse `{}`: {}\n\nOverwrite the existing configuration file \
                         with the default values?",
                        path_str,
                        err,
                    ),
                };
                File {
                    contents: T::default(),
                    path: save.then(|| path.to_path_buf()),
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
        [
            (frame_size[0] - width) * 0.5,
            (frame_size[1] - height) * 0.5,
        ],
        [width, height],
    )
}

pub fn scale_to_fit_rotated(
    mut orig_size: [f32; 2],
    integer_scale: bool,
    rot: f32,
    frame_size: [f32; 2],
) -> ([f32; 2], [[f32; 2]; 4]) {
    let half_size = frame_size.map(|v| v * 0.5);
    let (sin, cos) = rot.sin_cos();
    let mut scale = f32::INFINITY;
    let rotate_and_get_scale = |[x, y]: [f32; 2]| {
        let rot_x = x * cos - y * sin;
        let rot_y = x * sin + y * cos;
        scale = scale
            .min(half_size[0] / rot_x.abs())
            .min(half_size[1] / rot_y.abs());
        [rot_x, rot_y]
    };
    orig_size[0] *= 0.5;
    orig_size[1] *= 0.5;
    let rotated_rel_points = [
        [-orig_size[0], -orig_size[1]],
        [orig_size[0], -orig_size[1]],
        orig_size,
        [-orig_size[0], orig_size[1]],
    ]
    .map(rotate_and_get_scale);
    if integer_scale && scale > 1.0 {
        scale = scale.floor();
    }
    (
        half_size,
        rotated_rel_points.map(|point| {
            [
                point[0] * scale + half_size[0],
                point[1] * scale + half_size[1],
            ]
        }),
    )
}

pub fn heading(ui: &Ui, text: &str, text_indent: f32, margin: f32) {
    let start_x = ui.cursor_screen_pos()[0];
    let window_x_bounds = [start_x, start_x + ui.content_region_avail()[0]];
    let separator_color = ui.style_color(StyleColor::Separator);

    let mut text_start_pos = ui.cursor_screen_pos();
    text_start_pos[0] += text_indent;
    ui.set_cursor_screen_pos(text_start_pos);
    ui.text(text);

    text_start_pos[1] += ui.text_line_height() * 0.5;
    let text_end_x = text_start_pos[0] + ui.calc_text_size(text)[0];

    let draw_list = ui.get_window_draw_list();
    draw_list
        .add_line(
            [window_x_bounds[0], text_start_pos[1]],
            [text_start_pos[0] - margin, text_start_pos[1]],
            separator_color,
        )
        .build();
    draw_list
        .add_line(
            [window_x_bounds[1], text_start_pos[1]],
            [text_end_x + margin, text_start_pos[1]],
            separator_color,
        )
        .build();
}

pub fn combo_value<T: PartialEq + Clone, L: for<'a> Fn(&'a T) -> Cow<'a, str>>(
    ui: &Ui,
    label: impl AsRef<str>,
    current_item: &mut T,
    items: &[T],
    label_fn: L,
) -> bool {
    let mut i = items.iter().position(|i| i == current_item).unwrap();
    if ui.combo(label, &mut i, items, label_fn) {
        *current_item = items[i].clone();
        true
    } else {
        false
    }
}
