mod saves;

use super::{
    audio,
    utils::{config_base, data_base},
};
#[cfg(feature = "xq-audio")]
use dust_core::audio::InterpMethod as AudioXqInterpMethod;
use dust_core::{
    cpu::{arm7, arm9},
    spi::firmware,
    utils::{zeroed_box, BoxedByteSlice, Bytes},
    Model,
};
use saves::{save_path, SavePathConfig};
use serde::{Deserialize, Serialize};
use std::{
    fmt, fs,
    io::{self, Read},
    iter,
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
    pub model: ModelConfig,
    pub limit_framerate: bool,
    pub screen_rotation: i16,
    pub sync_to_audio: bool,
    #[cfg(feature = "xq-audio")]
    pub audio_xq_sample_rate_shift: u8,
    #[cfg(feature = "xq-audio")]
    pub audio_xq_interp_method: AudioXqInterpMethod,
    pub audio_interp_method: audio::InterpMethod,
    pub pause_on_launch: bool,
    pub autosave_interval_ms: f32,
    pub rtc_time_offset_seconds: i64,

    pub save_dir_path: PathBuf,

    pub audio_volume: f32,
    pub audio_sample_chunk_size: u32,
    pub fullscreen_render: bool,
    pub screen_integer_scale: bool,
    pub game_db_path: Option<PathBuf>,
    pub logging_kind: LoggingKind,
    pub imgui_log_history_capacity: usize,
    pub window_size: (u32, u32),
    pub imgui_config_path: Option<PathBuf>,
    pub hide_macos_title_bar: bool,
}

impl Default for Global {
    fn default() -> Self {
        let config_base = config_base();
        let data_base = data_base();
        Global {
            sys_dir_path: None,
            sys_paths: Default::default(),
            skip_firmware: true,
            model: ModelConfig::Auto,
            limit_framerate: true,
            screen_rotation: 0,
            sync_to_audio: true,
            audio_interp_method: audio::InterpMethod::Nearest,
            #[cfg(feature = "xq-audio")]
            audio_xq_sample_rate_shift: 0,
            #[cfg(feature = "xq-audio")]
            audio_xq_interp_method: AudioXqInterpMethod::Nearest,
            pause_on_launch: false,
            autosave_interval_ms: 1000.0,
            rtc_time_offset_seconds: 0,

            save_dir_path: data_base.join("saves"),

            audio_volume: 1.0,
            audio_sample_chunk_size: 512,
            fullscreen_render: true,
            screen_integer_scale: false,
            game_db_path: Some(data_base.join("game_db.json")),
            logging_kind: LoggingKind::Imgui,
            imgui_log_history_capacity: 1024 * 1024,
            window_size: (1300, 800),
            imgui_config_path: Some(config_base.join("imgui.ini")),
            hide_macos_title_bar: true,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Game {
    pub sys_dir_path: Option<PathBuf>,
    pub sys_paths: SysPaths,
    pub skip_firmware: Option<bool>,
    pub model: Option<ModelConfig>,
    pub limit_framerate: Option<bool>,
    pub screen_rotation: Option<i16>,
    pub sync_to_audio: Option<bool>,
    pub audio_interp_method: Option<audio::InterpMethod>,
    #[cfg(feature = "xq-audio")]
    pub audio_xq_sample_rate_shift: Option<u8>,
    #[cfg(feature = "xq-audio")]
    pub audio_xq_interp_method: Option<AudioXqInterpMethod>,
    pub pause_on_launch: Option<bool>,
    pub autosave_interval_ms: Option<f32>,
    pub rtc_time_offset_seconds: Option<i64>,

    pub save_path: Option<SavePathConfig>,
}

impl Default for Game {
    fn default() -> Self {
        Game {
            sys_dir_path: None,
            sys_paths: Default::default(),
            skip_firmware: None,
            model: None,
            limit_framerate: None,
            screen_rotation: None,
            sync_to_audio: None,
            audio_interp_method: None,
            #[cfg(feature = "xq-audio")]
            audio_xq_sample_rate_shift: None,
            #[cfg(feature = "xq-audio")]
            audio_xq_interp_method: None,
            pause_on_launch: None,
            autosave_interval_ms: None,
            rtc_time_offset_seconds: None,

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
pub struct RuntimeModifiable<T> {
    pub value: T,
    pub origin: SettingOrigin,
}

impl<T> RuntimeModifiable<T> {
    pub fn global(value: T) -> Self {
        RuntimeModifiable {
            value,
            origin: SettingOrigin::Global,
        }
    }
}

pub struct SysFiles {
    pub arm7_bios: Box<Bytes<{ arm7::BIOS_SIZE }>>,
    pub arm9_bios: Box<Bytes<{ arm9::BIOS_SIZE }>>,
    pub firmware: BoxedByteSlice,
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
                        firmware::VerificationRegion::User1 => "User 1",
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
    UnknownModel(firmware::ModelDetectionError),
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
            LaunchConfigError::UnknownModel(_) => {
                write!(
                    f,
                    concat!(
                        "Couldn't detect DS model from the provided firmware, please specify one ",
                        "directly",
                    )
                )
            }
        }
    }
}

pub struct CommonLaunchConfig {
    pub sys_files: SysFiles,
    pub skip_firmware: bool,
    pub model: Model,
    pub limit_framerate: RuntimeModifiable<bool>,
    pub screen_rotation: RuntimeModifiable<i16>,
    pub sync_to_audio: RuntimeModifiable<bool>,
    pub audio_interp_method: RuntimeModifiable<audio::InterpMethod>,
    pub audio_sample_chunk_size: u32,
    #[cfg(feature = "xq-audio")]
    pub audio_xq_sample_rate_shift: RuntimeModifiable<u8>,
    #[cfg(feature = "xq-audio")]
    pub audio_xq_interp_method: RuntimeModifiable<AudioXqInterpMethod>,
    pub pause_on_launch: bool,
    pub autosave_interval_ms: RuntimeModifiable<f32>,
    pub rtc_time_offset_seconds: RuntimeModifiable<i64>,
}

pub struct GameLaunchConfig {
    pub common: CommonLaunchConfig,
    pub cur_save_path: Option<PathBuf>,
}

fn read_sys_files(paths: SysPaths, errors: &mut Vec<LaunchConfigError>) -> Option<SysFiles> {
    macro_rules! open_file {
        ($field: ident, $file: ident, |$file_ident: ident| $f: expr) => {
            match paths.$field {
                Some(path) => {
                    let result: Result<_, io::Error> = try {
                        let mut $file_ident = fs::File::open(&path)?;
                        $f
                    };
                    result.unwrap_or_else(|err| {
                        errors.push(LaunchConfigError::SysFileError(SystemFile::$file, err));
                        None
                    })
                }
                None => {
                    errors.push(LaunchConfigError::MissingSysPath(SystemFile::$file));
                    None
                }
            }
        };
    }
    let (arm7_bios, arm9_bios, firmware) = (
        open_file!(arm7_bios, Arm7Bios, |file| {
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
        }),
        open_file!(arm9_bios, Arm9Bios, |file| {
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
        }),
        open_file!(firmware, Firmware, |file| {
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
        arm7_bios: arm7_bios.unwrap(),
        arm9_bios: arm9_bios.unwrap(),
        firmware: firmware.unwrap(),
    })
}

fn common_launch_config(
    global_config: &Global,
    game_config: Option<&Game>,
) -> Result<(CommonLaunchConfig, Vec<LaunchConfigWarning>), Vec<LaunchConfigError>> {
    macro_rules! plain_setting {
        ($field: ident) => {
            game_config
                .and_then(|config| config.$field)
                .unwrap_or(global_config.$field)
        };
    }

    macro_rules! runtime_modifiable {
        ($field: ident) => {
            if let Some(value) = game_config.and_then(|c| c.$field) {
                RuntimeModifiable {
                    value,
                    origin: SettingOrigin::Game,
                }
            } else {
                RuntimeModifiable {
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

    let sys_files = read_sys_files(
        SysPaths {
            arm7_bios: sys_path!(arm7_bios, "biosnds7.bin"),
            arm9_bios: sys_path!(arm9_bios, "biosnds9.bin"),
            firmware: sys_path!(firmware, "firmware.bin"),
        },
        &mut errors,
    );

    let mut model = None;
    if let Some(firmware) = sys_files.as_ref().map(|files| &files.firmware) {
        model = match plain_setting!(model) {
            ModelConfig::Auto => match firmware::detect_model(firmware.as_byte_slice()) {
                Ok(model) => Some(model),
                Err(error) => {
                    errors.push(LaunchConfigError::UnknownModel(error));
                    None
                }
            },
            ModelConfig::Ds => Some(Model::Ds),
            ModelConfig::Lite => Some(Model::Lite),
            ModelConfig::Ique => Some(Model::Ique),
            ModelConfig::IqueLite => Some(Model::IqueLite),
            ModelConfig::Dsi => Some(Model::Dsi),
        };
        if let Some(model) = model {
            if let Err(error) = firmware::verify(firmware.as_byte_slice(), model) {
                warnings.push(LaunchConfigWarning::InvalidFirmware(error));
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let skip_firmware = plain_setting!(skip_firmware);
    let limit_framerate = runtime_modifiable!(limit_framerate);
    let screen_rotation = runtime_modifiable!(screen_rotation);
    let sync_to_audio = runtime_modifiable!(sync_to_audio);
    let audio_interp_method = runtime_modifiable!(audio_interp_method);
    #[cfg(feature = "xq-audio")]
    let audio_xq_sample_rate_shift = runtime_modifiable!(audio_xq_sample_rate_shift);
    #[cfg(feature = "xq-audio")]
    let audio_xq_interp_method = runtime_modifiable!(audio_xq_interp_method);
    let pause_on_launch = plain_setting!(pause_on_launch);
    let autosave_interval_ms = runtime_modifiable!(autosave_interval_ms);
    let rtc_time_offset_seconds = runtime_modifiable!(rtc_time_offset_seconds);

    Ok((
        CommonLaunchConfig {
            sys_files: sys_files.unwrap(),
            skip_firmware,
            model: model.unwrap(),
            limit_framerate,
            screen_rotation,
            sync_to_audio,
            audio_interp_method,
            audio_sample_chunk_size: global_config.audio_sample_chunk_size,
            #[cfg(feature = "xq-audio")]
            audio_xq_sample_rate_shift,
            #[cfg(feature = "xq-audio")]
            audio_xq_interp_method,
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
    common_launch_config(global_config, None)
}

pub fn game_launch_config(
    global_config: &Global,
    game_config: &Game,
    game_title: &str,
) -> Result<(GameLaunchConfig, Vec<LaunchConfigWarning>), Vec<LaunchConfigError>> {
    let (common, warnings) = common_launch_config(global_config, Some(game_config))?;

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
