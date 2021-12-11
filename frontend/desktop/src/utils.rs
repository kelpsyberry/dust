use std::{
    env,
    lazy::SyncLazy,
    path::{Path, PathBuf},
};

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
