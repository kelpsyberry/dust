pub mod saves;
#[allow(dead_code)]
mod setting;
pub use setting::{Resolvable, Setting};

use crate::{
    audio,
    utils::{config_base, data_base, HomePathBuf},
};
use dust_core::{
    audio::ChannelInterpMethod as AudioChannelInterpMethod,
    cpu::{arm7, arm9},
    spi::firmware,
    utils::{zeroed_box, BoxedByteSlice, Bytes},
    Model,
};
use serde::{Deserialize, Serialize};
use setting::{
    resolve_option, set_option, set_unreachable, NonOverridable, Overridable, OverridableTypes,
    SettingOrigin, Tracked, Untracked,
};
use std::{
    fmt, fs,
    io::{self, Read},
    net::SocketAddr,
    num::NonZeroU32,
    path::{Path, PathBuf},
};

macro_rules! def_config {
    (
        $global_config: ident, $game_config: ident, $config: ident,
        untracked {
            global {
                $($ug_ident: ident: $ug_inner_ty: ty = $ug_default: expr,)*
            }
            overridable {
                $(
                    $uo_ident: ident: $($uo_inner_ty: ty),* = $uo_global_default: expr,
                    $uo_game_default: expr, resolve $uo_resolve: path, set $uo_set: path,
                )*
            }
            game {
                $($uga_ident: ident: $uga_inner_ty: ty = $uga_default: expr,)*
            }
        }
        tracked {
            global {
                $($tg_ident: ident: $tg_inner_ty: ty = $tg_default: expr,)*
            }
            overridable {
                $(
                    $to_ident: ident: $($to_inner_ty: ty),* = $to_global_default: expr,
                    $to_game_default: expr, resolve $to_resolve: path, set $to_set: path,
                )*
            }
            game {
                $(
                    $tga_ident: ident: $tga_inner_ty: ty = $tga_default: expr,
                )*
            }
        }
        ui {
            $($ui_ident: ident: $ui_ty: ty = $ui_default: expr,)*
        }
    ) => {
        #[derive(Serialize, Deserialize)]
        #[serde(default, rename_all = "kebab-case")]
        pub struct $global_config {
            $(pub $ug_ident: $ug_inner_ty,)*
            $(pub $uo_ident: <Overridable<$($uo_inner_ty),*> as OverridableTypes>::Global,)*
            $(pub $tg_ident: $tg_inner_ty,)*
            $(pub $to_ident: <Overridable<$($to_inner_ty),*> as OverridableTypes>::Global,)*
            $(pub $ui_ident: $ui_ty,)*
        }

        impl Default for $global_config {
            fn default() -> Self {
                $global_config {
                    $($ug_ident: $ug_default,)*
                    $($uo_ident: $uo_global_default,)*
                    $($tg_ident: $tg_default,)*
                    $($to_ident: $to_global_default,)*
                    $($ui_ident: $ui_default,)*
                }
            }
        }

        #[derive(Serialize, Deserialize)]
        #[serde(default, rename_all = "kebab-case")]
        pub struct $game_config {
            $(pub $uo_ident: <Overridable<$($uo_inner_ty),*> as OverridableTypes>::Game,)*
            $(pub $uga_ident: $uga_inner_ty,)*
            $(pub $to_ident: <Overridable<$($to_inner_ty),*> as OverridableTypes>::Game,)*
            $(pub $tga_ident: $tga_inner_ty,)*
        }

        impl Default for $game_config {
            fn default() -> Self {
                $game_config {
                    $($uo_ident: $uo_game_default,)*
                    $($uga_ident: $uga_default,)*
                    $($to_ident: $to_game_default,)*
                    $($tga_ident: $tga_default,)*
                }
            }
        }

        pub struct $config {
            $(pub $ug_ident: Untracked<NonOverridable<$ug_inner_ty>>,)*
            $(pub $uo_ident: Untracked<Overridable<$($uo_inner_ty),*>>,)*
            $(pub $uga_ident: Untracked<NonOverridable<$uga_inner_ty>>,)*
            $(pub $tg_ident: Tracked<NonOverridable<$tg_inner_ty>>,)*
            $(pub $to_ident: Tracked<Overridable<$($to_inner_ty),*>>,)*
            $(pub $tga_ident: Tracked<NonOverridable<$tga_inner_ty>>,)*
            $(pub $ui_ident: $ui_ty,)*
        }

        impl $config {
            pub fn from_global(global: &$global_config) -> Self {
                $config {
                    $($ug_ident: Untracked::new(NonOverridable::new(
                        global.$ug_ident.clone(),
                        $ug_default,
                    )),)*
                    $($uo_ident: Untracked::new(Overridable::new(
                        global.$uo_ident.clone(),
                        $uo_global_default,
                        Default::default(),
                        $uo_game_default,
                        $uo_resolve,
                        $uo_set,
                    )),)*
                    $($uga_ident: Untracked::new(NonOverridable::new(
                        Default::default(),
                        $uga_default,
                    )),)*
                    $($tg_ident: Tracked::new(
                        NonOverridable::new(global.$tg_ident.clone(), $tg_default),
                    ),)*
                    $($to_ident: Tracked::new(Overridable::new(
                        global.$to_ident.clone(),
                        $to_global_default,
                        Default::default(),
                        $to_game_default,
                        $to_resolve,
                        $to_set,
                    )),)*
                    $($tga_ident: Tracked::new(
                        NonOverridable::new(Default::default(), $tga_default),
                    ),)*
                    $($ui_ident: global.$ui_ident.clone(),)*
                }
            }

            pub fn serialize_global(&self) -> $global_config {
                $global_config {
                    $($ug_ident: self.$ug_ident.get().clone(),)*
                    $($uo_ident: self.$uo_ident.inner().global().clone(),)*
                    $($tg_ident: self.$tg_ident.get().clone(),)*
                    $($to_ident: self.$to_ident.inner().global().clone(),)*
                    $($ui_ident: self.$ui_ident.clone(),)*
                }
            }

            pub fn deserialize_global(&mut self, global: &$global_config) {
                $(self.$ug_ident.set(global.$ug_ident.clone());)*
                $(self.$uo_ident.update(|value| value.set_global(global.$uo_ident.clone()));)*
                $(self.$tg_ident.set(global.$tg_ident.clone());)*
                $(self.$to_ident.update(|value| value.set_global(global.$to_ident.clone()));)*
                $(self.$ui_ident = global.$ui_ident.clone();)*
            }

            pub fn serialize_game(&self) -> $game_config {
                $game_config {
                    $($uo_ident: self.$uo_ident.inner().game().clone(),)*
                    $($uga_ident: self.$uga_ident.get().clone(),)*
                    $($to_ident: self.$to_ident.inner().game().clone(),)*
                    $($tga_ident: self.$tga_ident.get().clone(),)*
                }
            }

            pub fn deserialize_game(&mut self, game: &$game_config) {
                $(self.$uo_ident.update(|value| value.set_game(game.$uo_ident.clone()));)*
                $(self.$uga_ident.set(game.$uga_ident.clone());)*
                $(self.$to_ident.update(|value| value.set_game(game.$to_ident.clone()));)*
                $(self.$tga_ident.set(game.$tga_ident.clone());)*
            }

            pub fn unset_game(&mut self) {
                $(self.$uo_ident.update(|value| value.set_default_game());)*
                $(self.$uga_ident.set($lga_default);)*
                $(self.$to_ident.update(|value| value.set_default_game());)*
                $(self.$tga_ident.set($tga_default);)*
            }

            pub fn clear_updates(&mut self) {
                $(self.$tg_ident.clear_updates();)*
                $(self.$to_ident.clear_updates();)*
                $(self.$tga_ident.clear_updates();)*
            }
        }
    };
}

macro_rules! config_changed {
    ($config: expr, $($key: ident)|*) => {{
        let config = &$config;
        $(config.$key.changed())||*
    }};
}

macro_rules! config_changed_value {
    ($config: expr, $key: ident) => {{
        use $crate::config::Setting;
        let config = &$config;
        if config.$key.changed() {
            Some(*config.$key.get())
        } else {
            None
        }
    }};
}

macro_rules! config {
    ($config: expr, $key: ident) => {{
        use $crate::config::Setting;
        $config.$key.get().clone()
    }};
    ($config: expr, &$key: ident) => {{
        use $crate::config::Setting;
        $config.$key.get()
    }};
}

macro_rules! set_config {
    ($config: expr, $key: ident, $value: expr) => {{
        use $crate::config::Setting;
        $config.$key.set($value);
    }};
}

macro_rules! toggle_config {
    ($config: expr, $key: ident) => {{
        use $crate::config::Setting;
        let config = &mut $config;
        config.$key.set(!config.$key.get().clone());
    }};
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct SysPaths<T = Option<HomePathBuf>> {
    pub dir: T,
    pub arm7_bios: T,
    pub arm9_bios: T,
    pub firmware: T,
}

#[derive(Clone)]
pub struct ResolvedSysPaths {
    pub arm7_bios: Option<HomePathBuf>,
    pub arm9_bios: Option<HomePathBuf>,
    pub firmware: Option<HomePathBuf>,
}

impl ResolvedSysPaths {
    fn resolve(global: &SysPaths, game: &SysPaths) -> (Self, SettingOrigin) {
        macro_rules! path {
            ($field: ident, $path_in_sys_dir: expr) => {
                [game, global].into_iter().find_map(|paths| {
                    paths.$field.clone().or_else(|| {
                        paths
                            .dir
                            .as_ref()
                            .map(|dir_path| HomePathBuf(dir_path.0.join($path_in_sys_dir)))
                    })
                })
            };
        }

        (
            ResolvedSysPaths {
                arm7_bios: path!(arm7_bios, "biosnds7.bin"),
                arm9_bios: path!(arm9_bios, "biosnds9.bin"),
                firmware: path!(arm7_bios, "firmware.bin"),
            },
            SettingOrigin::Game,
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LoggingKind {
    Imgui,
    Term,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelConfig {
    Auto,
    Ds,
    Lite,
    Ique,
    IqueLite,
    Dsi,
}

fn resolve_opt_nonzero_u32(
    global: &u32,
    game: &Option<u32>,
) -> (Option<NonZeroU32>, SettingOrigin) {
    let (value, origin) = resolve_option(global, game);
    (NonZeroU32::new(value), origin)
}

fn set_opt_nonzero_u32(
    global: &mut u32,
    game: &mut Option<u32>,
    value: Option<NonZeroU32>,
    origin: SettingOrigin,
) {
    set_option(
        global,
        game,
        match value {
            Some(value) => value.get(),
            None => 0,
        },
        origin,
    );
}

// NOTE: All settings can be changed at runtime (although some changes can only be applied when the
//       emulator is restarted); the untracked ones simply don't need to run any update processing
//       code.

def_config! {
    Global, Game, Config,
    untracked {
        global {
            imgui_config_path: Option<HomePathBuf> = Some(
                HomePathBuf(config_base().join("imgui.ini"))
            ),
            screen_integer_scale: bool = false,
            reset_on_save_slot_switch: bool = true,
            gdb_server_addr: SocketAddr = ([127_u8, 0, 0, 1], 12345_u16).into(),
        }
        overridable {
            ds_slot_rom_in_memory_max_size: u32 = 32 * 1024 * 1024, None,
                resolve resolve_option, set set_option,
            screen_rot: u16 = 0, None,
                resolve resolve_option, set set_option,
            sys_paths: ResolvedSysPaths, SysPaths, SysPaths, SysPaths<Option<Option<HomePathBuf>>>
                = Default::default(), Default::default(),
                resolve ResolvedSysPaths::resolve, set set_unreachable,
            skip_firmware: bool = true, None,
                resolve resolve_option, set set_option,
            pause_on_launch: bool = false, None,
                resolve resolve_option, set set_option,
            model: ModelConfig = ModelConfig::Auto, None,
                resolve resolve_option, set set_option,
            prefer_hle_bios: bool = false, None,
                resolve resolve_option, set set_option,
        }
        game {}
    }
    tracked {
        global {
            hide_macos_title_bar: bool = true,
            game_db_path: Option<HomePathBuf> = Some(HomePathBuf(data_base().join("game_db.json"))),
            logging_kind: LoggingKind = LoggingKind::Imgui,
            save_dir_path: HomePathBuf = HomePathBuf(data_base().join("saves")),
        }
        overridable {
            fullscreen_render: bool = true, None,
                resolve resolve_option, set set_option,
            imgui_log_history_capacity: u32 = 1024 * 1024, None,
                resolve resolve_option, set set_option,
            discord_presence_enabled: bool = true, None,
                resolve resolve_option, set set_option,
            limit_framerate: bool = true, None,
                resolve resolve_option, set set_option,
            sync_to_audio: bool = true, None,
                resolve resolve_option, set set_option,
            audio_volume: f32 = 1.0, None,
                resolve resolve_option, set set_option,
            audio_sample_chunk_size: u16 = 512, None,
                resolve resolve_option, set set_option,
            audio_interp_method: audio::InterpMethod = audio::InterpMethod::Nearest, None,
                resolve resolve_option, set set_option,
            audio_custom_sample_rate: Option<NonZeroU32>, u32 = 0, None,
                resolve resolve_opt_nonzero_u32, set set_opt_nonzero_u32,
            audio_channel_interp_method: AudioChannelInterpMethod = AudioChannelInterpMethod::Nearest, None,
                resolve resolve_option, set set_option,
            save_interval_ms: f32 = 1000.0, None,
                resolve resolve_option, set set_option,
            rtc_time_offset_seconds: i64 = 0, None,
                resolve resolve_option, set set_option,
        }
        game {
            save_path_config: Option<saves::PathConfig> = Some(Default::default()),
        }
    }
    ui {
        window_size: (u32, u32) = (1300, 800),
    }
}

impl Config {
    pub fn save_path(&self, game_title: &str) -> Option<PathBuf> {
        config!(self, &save_path_config)
            .as_ref()
            .and_then(|config| config.path(&config!(self, &save_dir_path).0, game_title))
    }
}

#[derive(Default)]
pub struct File<T: Serialize + for<'de> Deserialize<'de>> {
    pub path: Option<PathBuf>,
    pub contents: T,
}

#[derive(Debug)]
pub enum FileError {
    Io(io::Error),
    Json(serde_json::Error),
}

impl<T: Default + Serialize + for<'de> Deserialize<'de>> File<T> {
    pub fn read(path: &Path, default_on_not_found: bool) -> Result<Self, FileError> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(err) => {
                if default_on_not_found && err.kind() == io::ErrorKind::NotFound {
                    return Ok(File {
                        path: Some(path.to_path_buf()),
                        contents: Default::default(),
                    });
                } else {
                    return Err(FileError::Io(err));
                }
            }
        };
        match serde_json::from_str(&content) {
            Ok(result) => Ok(File {
                path: Some(path.to_path_buf()),
                contents: result,
            }),
            Err(err) => Err(FileError::Json(err)),
        }
    }

    pub fn reload(&mut self) -> Result<(), FileError> {
        if let Some(path) = &self.path {
            self.contents = Self::read(path, false)?.contents;
        }
        Ok(())
    }

    pub fn write(&self) -> Result<(), FileError> {
        if let Some(path) = &self.path {
            fs::write(
                path,
                &serde_json::to_vec_pretty(&self.contents).map_err(FileError::Json)?,
            )
            .map_err(FileError::Io)
        } else {
            Ok(())
        }
    }
}

pub struct SysFiles {
    pub arm7_bios: Option<Box<Bytes<{ arm7::BIOS_SIZE }>>>,
    pub arm9_bios: Option<Box<Bytes<{ arm9::BIOS_SIZE }>>>,
    pub firmware: Option<BoxedByteSlice>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SystemFile {
    Arm7Bios,
    Arm9Bios,
    Firmware,
}

pub enum LaunchWarning {
    InvalidFirmware(firmware::VerificationError),
}

impl fmt::Display for LaunchWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LaunchWarning::InvalidFirmware(verification_error) => match verification_error {
                firmware::VerificationError::IncorrectSize(got) => {
                    write!(f, "Invalid firmware size ({got} bytes)")
                }
                firmware::VerificationError::IncorrectCrc16 {
                    region,
                    expected,
                    calculated,
                } => write!(
                    f,
                    "Incorrect CRC16 for firmware {} region: expected {:#06X}, calculated {:#06X}",
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

pub enum LaunchError {
    MissingSysPath(SystemFile),
    SysFileError(SystemFile, io::Error),
    InvalidSysFileLength {
        file: SystemFile,
        expected: usize,
        got: u64,
    },
}

impl fmt::Display for LaunchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const SYS_FILE_NAMES: [&str; 3] = ["ARM7 BIOS", "ARM9 BIOS", "firmware"];

        match self {
            LaunchError::MissingSysPath(file) => {
                write!(f, "Missing path for {}", SYS_FILE_NAMES[*file as usize])
            }
            LaunchError::SysFileError(file, err) => {
                write!(
                    f,
                    "Error while reading {}: {err}",
                    SYS_FILE_NAMES[*file as usize]
                )
            }
            LaunchError::InvalidSysFileLength {
                file,
                expected,
                got,
            } => {
                write!(
                    f,
                    "Invalid {} size: expected {expected} bytes, got {got} bytes",
                    SYS_FILE_NAMES[*file as usize]
                )
            }
        }
    }
}

pub struct Launch {
    pub sys_files: SysFiles,
    pub skip_firmware: bool,
    pub model: Model,
}

impl Launch {
    pub fn new(
        config: &Config,
        is_firmware: bool,
    ) -> Result<(Self, Vec<LaunchWarning>), Vec<LaunchError>> {
        let prefer_hle_bios = !is_firmware && *config.prefer_hle_bios.get();
        let skip_firmware = !is_firmware && (prefer_hle_bios || *config.skip_firmware.get());

        let mut warnings = Vec::new();
        let mut errors = Vec::new();

        macro_rules! open_file {
            ($path: expr, $file: ident, |$file_ident: ident| $f: expr) => {
                match $path {
                    Some(path) => (|| {
                        let mut $file_ident = fs::File::open(&path.0)?;
                        Ok($f)
                    })()
                    .unwrap_or_else(|err| {
                        if is_firmware {
                            errors.push(LaunchError::SysFileError(SystemFile::$file, err));
                        }
                        None
                    }),
                    None => {
                        if is_firmware {
                            errors.push(LaunchError::MissingSysPath(SystemFile::$file));
                        }
                        None
                    }
                }
            };
        }

        let (arm7_bios, arm9_bios, firmware) = (
            if !prefer_hle_bios {
                open_file!(&config.sys_paths.get().arm7_bios, Arm7Bios, |file| {
                    let len = file.metadata()?.len();
                    if len == arm7::BIOS_SIZE as u64 {
                        let mut buf = zeroed_box::<Bytes<{ arm7::BIOS_SIZE }>>();
                        file.read_exact(&mut buf[..])?;
                        Some(buf)
                    } else {
                        errors.push(LaunchError::InvalidSysFileLength {
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
            if !prefer_hle_bios {
                open_file!(&config.sys_paths.get().arm9_bios, Arm9Bios, |file| {
                    let len = file.metadata()?.len();
                    if len == arm9::BIOS_SIZE as u64 {
                        let mut buf = zeroed_box::<Bytes<{ arm9::BIOS_SIZE }>>();
                        file.read_exact(&mut buf[..])?;
                        Some(buf)
                    } else {
                        errors.push(LaunchError::InvalidSysFileLength {
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
            open_file!(&config.sys_paths.get().firmware, Firmware, |file| {
                let len = file.metadata()?.len() as usize;
                let mut buf = BoxedByteSlice::new_zeroed(len);
                file.read_exact(&mut buf[..])?;
                Some(buf)
            }),
        );

        let model = match config.model.get() {
            ModelConfig::Auto => firmware
                .as_ref()
                .and_then(|firmware| firmware::detect_model(firmware.as_byte_slice()).ok())
                .unwrap_or_default(),
            ModelConfig::Ds => Model::Ds,
            ModelConfig::Lite => Model::Lite,
            ModelConfig::Ique => Model::Ique,
            ModelConfig::IqueLite => Model::IqueLite,
            ModelConfig::Dsi => Model::Dsi,
        };
        if let Some(firmware) = &firmware {
            if let Err(error) = firmware::verify(firmware.as_byte_slice(), model) {
                warnings.push(LaunchWarning::InvalidFirmware(error));
            }
        }

        if !errors.is_empty() {
            return Err(errors);
        }

        Ok((
            Launch {
                sys_files: SysFiles {
                    arm7_bios,
                    arm9_bios,
                    firmware,
                },
                skip_firmware,
                model,
            },
            warnings,
        ))
    }
}
