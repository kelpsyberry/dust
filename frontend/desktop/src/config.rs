mod saves;

use super::{
    audio,
    utils::{config_base, data_base},
};
use dust_core::{
    audio::ChannelInterpMethod as AudioChannelInterpMethod,
    cpu::{arm7, arm9},
    spi::firmware,
    utils::{zeroed_box, BoxedByteSlice, Bytes},
    Model,
};
use saves::{save_path, SavePathConfig};
use serde::{Deserialize, Serialize};
#[cfg(feature = "xq-audio")]
use std::num::NonZeroU32;
use std::{
    fmt, fs,
    io::{self, Read},
    iter,
    net::SocketAddr,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoggingKind {
    Imgui,
    Term,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelConfig {
    Auto,
    Ds,
    Lite,
    Ique,
    IqueLite,
    Dsi,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SysPaths {
    pub arm7_bios: Option<PathBuf>,
    pub arm9_bios: Option<PathBuf>,
    pub firmware: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Global {
    pub sys_dir_path: Option<PathBuf>,
    pub sys_paths: SysPaths,
    pub skip_firmware: bool,
    pub pause_on_launch: bool,
    pub model: ModelConfig,
    pub limit_framerate: bool,
    pub screen_rotation: i16,
    pub sync_to_audio: bool,
    pub audio_volume: f32,
    pub audio_sample_chunk_size: u16,
    pub audio_interp_method: audio::InterpMethod,
    pub audio_custom_sample_rate: u32,
    pub audio_channel_interp_method: AudioChannelInterpMethod,
    pub autosave_interval_ms: f32,
    pub rtc_time_offset_seconds: i64,
    pub prefer_hle_bios: bool,

    pub save_dir_path: PathBuf,

    pub fullscreen_render: bool,
    pub screen_integer_scale: bool,
    pub game_db_path: Option<PathBuf>,
    pub logging_kind: LoggingKind,
    pub imgui_log_history_capacity: usize,
    pub window_size: (u32, u32),
    pub imgui_config_path: Option<PathBuf>,
    pub hide_macos_title_bar: bool,
    pub gdb_server_addr: SocketAddr,
}

impl Default for Global {
    fn default() -> Self {
        let config_base = config_base();
        let data_base = data_base();
        Global {
            sys_dir_path: None,
            sys_paths: Default::default(),
            skip_firmware: true,
            pause_on_launch: false,
            model: ModelConfig::Auto,
            limit_framerate: true,
            screen_rotation: 0,
            sync_to_audio: true,
            audio_volume: 1.0,
            audio_sample_chunk_size: 512,
            audio_interp_method: audio::InterpMethod::Nearest,
            audio_custom_sample_rate: 0,
            audio_channel_interp_method: AudioChannelInterpMethod::Nearest,
            autosave_interval_ms: 1000.0,
            rtc_time_offset_seconds: 0,
            prefer_hle_bios: false,

            save_dir_path: data_base.join("saves"),

            fullscreen_render: true,
            screen_integer_scale: false,
            game_db_path: Some(data_base.join("game_db.json")),
            logging_kind: LoggingKind::Imgui,
            imgui_log_history_capacity: 1024 * 1024,
            window_size: (1300, 800),
            imgui_config_path: Some(config_base.join("imgui.ini")),
            hide_macos_title_bar: true,
            gdb_server_addr: ([127_u8, 0, 0, 1], 12345_u16).into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Game {
    pub sys_dir_path: Option<PathBuf>,
    pub sys_paths: SysPaths,
    pub skip_firmware: Option<bool>,
    pub pause_on_launch: Option<bool>,
    pub model: Option<ModelConfig>,
    pub limit_framerate: Option<bool>,
    pub screen_rotation: Option<i16>,
    pub sync_to_audio: Option<bool>,
    pub audio_volume: Option<f32>,
    pub audio_sample_chunk_size: Option<u16>,
    pub audio_interp_method: Option<audio::InterpMethod>,
    pub audio_custom_sample_rate: Option<u32>,
    pub audio_channel_interp_method: Option<AudioChannelInterpMethod>,
    pub autosave_interval_ms: Option<f32>,
    pub rtc_time_offset_seconds: Option<i64>,
    pub prefer_hle_bios: Option<bool>,

    pub save_path: Option<SavePathConfig>,
}

impl Default for Game {
    fn default() -> Self {
        Game {
            sys_dir_path: None,
            sys_paths: Default::default(),
            skip_firmware: None,
            pause_on_launch: None,
            model: None,
            limit_framerate: None,
            screen_rotation: None,
            sync_to_audio: None,
            audio_volume: None,
            audio_sample_chunk_size: None,
            audio_interp_method: None,
            audio_custom_sample_rate: None,
            audio_channel_interp_method: None,
            autosave_interval_ms: None,
            rtc_time_offset_seconds: None,
            prefer_hle_bios: None,

            save_path: Some(SavePathConfig::GlobalSingle),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Json(serde_json::Error),
}

#[derive(Clone, Debug, Default)]
pub struct Config<T> {
    pub contents: T,
    pub dirty: bool,
    pub path: Option<PathBuf>,
}

impl<T> Config<T> {
    pub fn read_from_file(path: PathBuf) -> Result<Option<Self>, Error>
    where
        T: for<'de> Deserialize<'de>,
    {
        let content = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) => {
                if err.kind() == io::ErrorKind::NotFound {
                    return Ok(None);
                } else {
                    return Err(Error::Io(err));
                }
            }
        };
        match serde_json::from_str(&content) {
            Ok(result) => Ok(Some(Config {
                contents: result,
                // `serde` might have added some default values, so save at least once just in case
                dirty: true,
                path: Some(path),
            })),
            Err(err) => Err(Error::Json(err)),
        }
    }

    pub fn read_from_file_or_show_dialog(path: &Path, config_name: &str) -> Self
    where
        T: Default + for<'de> Deserialize<'de>,
    {
        let path_str = path.to_str().unwrap_or(config_name);
        let (config, save) = match Self::read_from_file(path.to_path_buf()) {
            Ok(config) => (config, true),
            Err(err) => (
                None,
                match err {
                    Error::Io(err) => {
                        config_error!(
                            concat!(
                                "Couldn't read `{}`: {}\n\nThe default values will be used, new ",
                                "changes will not be saved.",
                            ),
                            path_str,
                            err,
                        );
                        false
                    }
                    Error::Json(err) => config_error!(
                        yes_no,
                        concat!(
                            "Couldn't parse `{}`: {}\n\nOverwrite the existing configuration file ",
                            "with the default values?",
                        ),
                        path_str,
                        err,
                    ),
                },
            ),
        };
        config.unwrap_or_else(|| {
            if save {
                Config {
                    contents: T::default(),
                    dirty: true,
                    path: Some(path.to_path_buf()),
                }
            } else {
                Config::default()
            }
        })
    }

    pub fn flush(&mut self) -> Result<(), Error>
    where
        T: Serialize,
    {
        if let Some(path) = &self.path {
            self.dirty = false;
            let content = serde_json::to_vec_pretty(&self.contents).map_err(Error::Json)?;
            fs::write(path, &content).map_err(Error::Io)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingOrigin {
    Global,
    Game,
}

#[derive(Clone, Copy, Debug)]
pub struct GameOverridable<T> {
    pub value: T,
    pub origin: SettingOrigin,
}

impl<T> GameOverridable<T> {
    pub fn global(value: T) -> Self {
        GameOverridable {
            value,
            origin: SettingOrigin::Global,
        }
    }

    #[allow(dead_code)]
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> GameOverridable<U> {
        GameOverridable {
            value: f(self.value),
            origin: self.origin,
        }
    }

    pub fn update(&mut self, new_value: GameOverridable<T>) -> bool
    where
        T: PartialEq,
    {
        let changed = new_value.value != self.value;
        *self = new_value;
        changed
    }

    pub fn update_value(&mut self, new_value: T) -> bool
    where
        T: PartialEq,
    {
        let changed = new_value != self.value;
        self.value = new_value;
        changed
    }
}

pub struct SysFiles {
    pub arm7_bios: Option<Box<Bytes<{ arm7::BIOS_SIZE }>>>,
    pub arm9_bios: Option<Box<Bytes<{ arm9::BIOS_SIZE }>>>,
    pub firmware: Option<BoxedByteSlice>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemFile {
    Arm7Bios,
    Arm9Bios,
    Firmware,
}

#[derive(Debug)]
pub enum LaunchConfigWarning {
    InvalidFirmware(firmware::VerificationError),
}

impl fmt::Display for LaunchConfigWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LaunchConfigWarning::InvalidFirmware(verification_error) => match verification_error {
                firmware::VerificationError::IncorrectSize(got) => {
                    write!(f, "Invalid firmware size ({} bytes)", got)
                }
                firmware::VerificationError::IncorrectCrc16 {
                    region,
                    expected,
                    calculated,
                } => write!(
                    f,
                    concat!(
                        "Incorrect CRC16 for firmware {} region: expected {:#06X}, calculated ",
                        "{:#06X}",
                    ),
                    match region {
                        firmware::VerificationRegion::Wifi => "Wi-Fi",
                        firmware::VerificationRegion::Ap1 => "Access Point 1",
                        firmware::VerificationRegion::Ap2 => "Access Point 2",
                        firmware::VerificationRegion::Ap3 => "Access Point 3",
                        firmware::VerificationRegion::User0 => "User 0",
                        firmware::VerificationRegion::User0IQue => "User 0 (iQue/DSi)",
                        firmware::VerificationRegion::User1 => "User 1",
                        firmware::VerificationRegion::User1IQue => "User 1 (iQue/DSi)",
                    },
                    expected,
                    calculated
                ),
            },
        }
    }
}

#[derive(Debug)]
pub enum LaunchConfigError {
    MissingSysPath(SystemFile),
    SysFileError(SystemFile, io::Error),
    InvalidSysFileLength {
        file: SystemFile,
        expected: usize,
        got: u64,
    },
}

impl fmt::Display for LaunchConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const SYS_FILE_NAMES: [&str; 3] = ["ARM7 BIOS", "ARM9 BIOS", "firmware"];

        match self {
            LaunchConfigError::MissingSysPath(file) => {
                write!(f, "Missing path for {}", SYS_FILE_NAMES[*file as usize])
            }
            LaunchConfigError::SysFileError(file, err) => {
                write!(
                    f,
                    "Error while reading {}: {}",
                    SYS_FILE_NAMES[*file as usize], err
                )
            }
            LaunchConfigError::InvalidSysFileLength {
                file,
                expected,
                got,
            } => {
                write!(
                    f,
                    "Invalid {} size: expected {} bytes, got {} bytes",
                    SYS_FILE_NAMES[*file as usize], expected, got
                )
            }
        }
    }
}

pub struct CommonLaunchConfig {
    pub sys_files: SysFiles,
    pub skip_firmware: bool,
    pub pause_on_launch: bool,
    pub model: Model,
    pub limit_framerate: GameOverridable<bool>,
    pub screen_rotation: GameOverridable<i16>,
    pub sync_to_audio: GameOverridable<bool>,
    pub audio_volume: GameOverridable<f32>,
    pub audio_sample_chunk_size: GameOverridable<u16>,
    pub audio_interp_method: GameOverridable<audio::InterpMethod>,
    #[cfg(feature = "xq-audio")]
    pub audio_custom_sample_rate: GameOverridable<Option<NonZeroU32>>,
    #[cfg(feature = "xq-audio")]
    pub audio_channel_interp_method: GameOverridable<AudioChannelInterpMethod>,
    pub autosave_interval_ms: GameOverridable<f32>,
    pub rtc_time_offset_seconds: GameOverridable<i64>,
}

pub struct GameLaunchConfig {
    pub common: CommonLaunchConfig,
    pub cur_save_path: Option<PathBuf>,
}

fn read_sys_files(
    paths: SysPaths,
    read_bios: bool,
    sys_files_required: bool,
    errors: &mut Vec<LaunchConfigError>,
) -> Option<SysFiles> {
    macro_rules! open_file {
        ($field: ident, $file: ident, $required: expr, |$file_ident: ident| $f: expr) => {
            match paths.$field {
                Some(path) => {
                    let result: Result<_, io::Error> = try {
                        let mut $file_ident = fs::File::open(&path)?;
                        $f
                    };
                    result.unwrap_or_else(|err| {
                        if $required {
                            errors.push(LaunchConfigError::SysFileError(SystemFile::$file, err));
                        }
                        None
                    })
                }
                None => {
                    if $required {
                        errors.push(LaunchConfigError::MissingSysPath(SystemFile::$file));
                    }
                    None
                }
            }
        };
    }
    let (arm7_bios, arm9_bios, firmware) = (
        if read_bios {
            open_file!(arm7_bios, Arm7Bios, sys_files_required, |file| {
                let len = file.metadata()?.len();
                if len == arm7::BIOS_SIZE as u64 {
                    let mut buf = zeroed_box::<Bytes<{ arm7::BIOS_SIZE }>>();
                    file.read_exact(&mut buf[..])?;
                    Some(buf)
                } else {
                    errors.push(LaunchConfigError::InvalidSysFileLength {
                        file: SystemFile::Arm7Bios,
                        expected: arm7::BIOS_SIZE,
                        got: len,
                    });
                    None
                }
            })
        } else {
            None
        },
        if read_bios {
            open_file!(arm9_bios, Arm9Bios, sys_files_required, |file| {
                let len = file.metadata()?.len();
                if len == arm9::BIOS_SIZE as u64 {
                    let mut buf = zeroed_box::<Bytes<{ arm9::BIOS_SIZE }>>();
                    file.read_exact(&mut buf[..])?;
                    Some(buf)
                } else {
                    errors.push(LaunchConfigError::InvalidSysFileLength {
                        file: SystemFile::Arm9Bios,
                        expected: arm9::BIOS_SIZE,
                        got: len,
                    });
                    None
                }
            })
        } else {
            None
        },
        open_file!(firmware, Firmware, sys_files_required, |file| {
            let len = file.metadata()?.len() as usize;
            let mut buf = BoxedByteSlice::new_zeroed(len);
            file.read_exact(&mut buf[..])?;
            Some(buf)
        }),
    );
    if !errors.is_empty() {
        return None;
    }
    Some(SysFiles {
        arm7_bios,
        arm9_bios,
        firmware,
    })
}

fn common_launch_config(
    global_config: &Global,
    sys_files_required: bool,
    game_config: Option<&Game>,
) -> Result<(CommonLaunchConfig, Vec<LaunchConfigWarning>), Vec<LaunchConfigError>> {
    macro_rules! plain_setting {
        ($field: ident) => {
            game_config
                .and_then(|config| config.$field)
                .unwrap_or(global_config.$field)
        };
    }

    macro_rules! game_overridable {
        ($field: ident) => {
            if let Some(value) = game_config.and_then(|c| c.$field) {
                GameOverridable {
                    value,
                    origin: SettingOrigin::Game,
                }
            } else {
                GameOverridable {
                    value: global_config.$field,
                    origin: SettingOrigin::Global,
                }
            }
        };
    }

    macro_rules! sys_path {
        ($field: ident, $path_in_sys_dir: expr) => {
            game_config
                .into_iter()
                .map(|config| (&config.sys_dir_path, &config.sys_paths))
                .chain(iter::once((
                    &global_config.sys_dir_path,
                    &global_config.sys_paths,
                )))
                .find_map(|(sys_dir_path, sys_paths)| {
                    sys_paths.$field.clone().or_else(|| {
                        sys_dir_path
                            .as_ref()
                            .map(|dir_path| dir_path.join($path_in_sys_dir))
                    })
                })
        };
    }

    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    let prefer_hle_bios = !sys_files_required && plain_setting!(prefer_hle_bios);

    let sys_files = read_sys_files(
        SysPaths {
            arm7_bios: sys_path!(arm7_bios, "biosnds7.bin"),
            arm9_bios: sys_path!(arm9_bios, "biosnds9.bin"),
            firmware: sys_path!(firmware, "firmware.bin"),
        },
        !prefer_hle_bios,
        sys_files_required,
        &mut errors,
    );

    let firmware = sys_files.as_ref().and_then(|files| files.firmware.as_ref());
    let model = match plain_setting!(model) {
        ModelConfig::Auto => firmware
            .and_then(|firmware| firmware::detect_model(firmware.as_byte_slice()).ok())
            .unwrap_or_default(),
        ModelConfig::Ds => Model::Ds,
        ModelConfig::Lite => Model::Lite,
        ModelConfig::Ique => Model::Ique,
        ModelConfig::IqueLite => Model::IqueLite,
        ModelConfig::Dsi => Model::Dsi,
    };
    if let Some(firmware) = firmware {
        if let Err(error) = firmware::verify(firmware.as_byte_slice(), model) {
            warnings.push(LaunchConfigWarning::InvalidFirmware(error));
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let skip_firmware = plain_setting!(skip_firmware);
    let pause_on_launch = plain_setting!(pause_on_launch);
    let limit_framerate = game_overridable!(limit_framerate);
    let screen_rotation = game_overridable!(screen_rotation);
    let sync_to_audio = game_overridable!(sync_to_audio);
    let audio_volume = game_overridable!(audio_volume);
    let audio_sample_chunk_size = game_overridable!(audio_sample_chunk_size);
    let audio_interp_method = game_overridable!(audio_interp_method);
    #[cfg(feature = "xq-audio")]
    let audio_custom_sample_rate = game_overridable!(audio_custom_sample_rate).map(NonZeroU32::new);
    #[cfg(feature = "xq-audio")]
    let audio_channel_interp_method = game_overridable!(audio_channel_interp_method);
    let autosave_interval_ms = game_overridable!(autosave_interval_ms);
    let rtc_time_offset_seconds = game_overridable!(rtc_time_offset_seconds);

    Ok((
        CommonLaunchConfig {
            sys_files: sys_files.unwrap(),
            skip_firmware,
            model,
            limit_framerate,
            screen_rotation,
            sync_to_audio,
            audio_volume,
            audio_sample_chunk_size,
            audio_interp_method,
            #[cfg(feature = "xq-audio")]
            audio_custom_sample_rate,
            #[cfg(feature = "xq-audio")]
            audio_channel_interp_method,
            pause_on_launch,
            autosave_interval_ms,
            rtc_time_offset_seconds,
        },
        warnings,
    ))
}

pub fn firmware_launch_config(
    global_config: &Global,
) -> Result<(CommonLaunchConfig, Vec<LaunchConfigWarning>), Vec<LaunchConfigError>> {
    common_launch_config(global_config, true, None)
}

pub fn game_launch_config(
    global_config: &Global,
    game_config: &Game,
    game_title: &str,
) -> Result<(GameLaunchConfig, Vec<LaunchConfigWarning>), Vec<LaunchConfigError>> {
    let (common, warnings) = common_launch_config(global_config, false, Some(game_config))?;

    let cur_save_path = save_path(
        &global_config.save_dir_path,
        &game_config.save_path,
        game_title,
    );

    Ok((
        GameLaunchConfig {
            common,
            cur_save_path,
        },
        warnings,
    ))
}
