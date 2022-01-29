use std::{
    env,
    lazy::SyncLazy,
    path::{Path, PathBuf},
};

macro_rules! format_list {
    ($list: expr) => {
        $list.into_iter().fold(String::new(), |mut acc, v| {
            use core::fmt::Write;
            let _ = write!(acc, "\n- {}", v);
            acc
        })
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

macro_rules! config_error {
    (yes_no, $($desc: tt)*) => {
        error!(yes_no, "Configuration error", $($desc)*)
    };
    ($($desc: tt)*) => {
        error!("Configuration error", $($desc)*)
    };
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

static CONFIG_BASE: SyncLazy<PathBuf> = SyncLazy::new(|| match env::var_os("XDG_CONFIG_HOME") {
    Some(config_dir) => Path::new(&config_dir).join("dust"),
    None => home::home_dir()
        .map(|home| home.join(".config/dust"))
        .unwrap_or_else(|| PathBuf::from("/.config/dust")),
});

static DATA_BASE: SyncLazy<PathBuf> = SyncLazy::new(|| match env::var_os("XDG_DATA_HOME") {
    Some(data_home) => Path::new(&data_home).join("dust"),
    None => home::home_dir()
        .map(|home| home.join(".local/share/dust"))
        .unwrap_or_else(|| PathBuf::from("/.local/share/dust")),
});

pub fn config_base<'a>() -> &'a Path {
    &*CONFIG_BASE
}

pub fn data_base<'a>() -> &'a Path {
    &*DATA_BASE
}
