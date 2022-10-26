macro_rules! modify_configs_mask {
    (
        $ui: expr, $(width $width: expr,)?
        $(icon_tooltip $icon: literal, $tooltip: literal,)?
        $(label $label: literal,)?
        $id: literal,
        $global_enabled: expr, $game_enabled: expr,
        |$global: ident, $game: ident| $process: expr
    ) => {
        let global_enabled = $global_enabled;
        let game_enabled = $game_enabled;
        let show_popup = game_enabled || !global_enabled;

        let pressed = $ui.button_with_size(
            $($icon,)*
            $(if show_popup {
                concat!($label, "...")
            } else {
                $label
            },)*
            [0.0 $(+ $width)*, 0.0],
        );

        let mut $global = false;
        let mut $game = false;

        if show_popup {
            $(
                if $ui.is_item_hovered() {
                    $ui.tooltip_text(concat!($tooltip, "..."));
                }
            )*
            if pressed {
                $ui.open_popup($id);
            }
            $ui.popup($id, || {
                $global = $ui
                    .menu_item_config("Global")
                    .enabled(global_enabled)
                    .build();
                $game = $ui.menu_item_config("Game").enabled(game_enabled).build();
                let all = $ui.menu_item("All");
                $global |= all;
                $game |= (all && game_enabled);
            });
        } else {
            $(
                if $ui.is_item_hovered() {
                    $ui.tooltip_text($tooltip);
                }
            )*
            $global = pressed;
        }

        $process
    };
}

macro_rules! modify_configs {
    (
        $ui: expr, $(width $width: expr,)?
        $(icon_tooltip $icon: literal, $tooltip: literal,)?
        $(label $label: literal,)?
        $id: literal,
        $game_enabled: expr, $process_global: expr, $process_game: expr
    ) => {
        let game_enabled = $game_enabled;

        let pressed = $ui.button_with_size(
            $($icon,)*
            $(if game_enabled {
                concat!($label, "...")
            } else {
                $label
            },)*
            [0.0 $(+ $width)*, 0.0],
        );

        let mut global = false;

        if game_enabled {
            $(
                if $ui.is_item_hovered() {
                    $ui.tooltip_text(concat!($tooltip, "..."));
                }
            )*
            if pressed {
                $ui.open_popup($id);
            }
            $ui.popup($id, || {
                global = $ui.menu_item("Global");
                if $ui.menu_item("Game") {
                    $process_game
                }
            })
        } else {
            $(
                if $ui.is_item_hovered() {
                    $ui.tooltip_text($tooltip);
                }
            )*
            global = pressed;
        }

        if global {
            $process_global
        }
    };
}

mod input_map;
#[allow(dead_code)]
mod setting;

use super::{utils::heading, Config, EmuState};
#[cfg(feature = "log")]
use crate::config::LoggingKind;
use crate::{
    audio,
    config::{
        self, saves, ModelConfig, Renderer2dKind, Renderer3dKind, Setting as _, TitleBarMode,
    },
    ui::utils::combo_value,
    utils::HomePathBuf,
};
#[cfg(feature = "xq-audio")]
use dust_core::audio::ChannelInterpMethod as AudioChannelInterpMethod;
use imgui::{StyleColor, TableColumnFlags, TableColumnSetup, TableFlags, Ui};
use input_map::Editor as InputMapEditor;
use rfd::FileDialog;
use setting::Setting;
use std::borrow::Cow;
#[cfg(feature = "xq-audio")]
use std::num::NonZeroU32;

struct SettingsData {
    game_loaded: bool,
}

macro_rules! home_path {
    (nonoverridable $id: ident) => {
        setting::HomePath::new(
            |config| config!(config, &$id),
            |config, value| set_config!(config, $id, value),
        )
    };
}

macro_rules! opt_home_path {
    (nonoverridable $id: ident) => {
        setting::OptHomePath::new(
            |config| config!(config, &$id).as_ref(),
            |config, value| set_config!(config, $id, value),
        )
    };
}

#[allow(unused_macros)]
macro_rules! socket_addr {
    (nonoverridable $id: ident) => {
        setting::SocketAddr::new(
            |config| config!(config, $id),
            |config, value| set_config!(config, $id, value),
        )
    };
}

macro_rules! scalar {
    (nonoverridable $id: ident, $step: expr) => {
        setting::Scalar::new(
            |config| config!(config, $id),
            |config, value| set_config!(config, $id, value),
            $step,
        )
    };
    (overridable $id: ident, $step: expr) => {
        (
            setting::Scalar::new(
                |config| *config.$id.inner().global(),
                |config, value| config.$id.update(|inner| inner.set_global(value)),
                $step,
            ),
            setting::Scalar::new(
                |config| config.$id.inner().game().unwrap(),
                |config, value| config.$id.update(|inner| inner.set_game(Some(value))),
                $step,
            ),
        )
    };
}

#[cfg(feature = "xq-audio")]
macro_rules! opt_nonzero_u32_slider {
    (overridable $id: ident, $default: expr, $min: expr, $max: expr, $display_format: expr) => {
        (
            setting::OptNonZeroU32Slider::new(
                |config| NonZeroU32::new(*config.$id.inner().global()),
                |config, value| {
                    config
                        .$id
                        .update(|inner| inner.set_global(value.map_or(0, |v| v.get())))
                },
                $default,
                $min,
                $max,
                $display_format,
            ),
            setting::OptNonZeroU32Slider::new(
                |config| NonZeroU32::new(config.$id.inner().game().unwrap()),
                |config, value| {
                    config
                        .$id
                        .update(|inner| inner.set_game(Some(value.map_or(0, |v| v.get()))))
                },
                $default,
                $min,
                $max,
                $display_format,
            ),
        )
    };
}

macro_rules! bool_and_value_slider {
    (overridable $id: ident, $min: expr, $max: expr, $display_format: expr$(, $scale: expr)?) => {
        (
            setting::BoolAndValueSlider::new(
                |config| {
                    let (active, value) = *config.$id.inner().global();
                    (active, value$(* $scale)*)
                },
                |config, (active, value)| config.$id.update(
                    |inner| inner.set_global((active, value$(/ $scale)*)),
                ),
                $min,
                $max,
                $display_format,
            ),
            setting::BoolAndValueSlider::new(
                |config| {
                    let (active, value) = config.$id.inner().game().unwrap();
                    (active, value$(* $scale)*)
                },
                |config, (active, value)| config.$id.update(
                    |inner| inner.set_game(Some((active, value$(/ $scale)*))),
                ),
                $min,
                $max,
                $display_format,
            ),
        )
    };
}

macro_rules! slider {
    (
        nonoverridable
        $id: ident, $min: expr, $max: expr, $display_format: expr$(, $scale: expr)?
    ) => {
        setting::Slider::new(
            |config| config!(config, $id) $(* $scale)*,
            |config, value| set_config!(config, $id, value $(/ $scale)*),
            $min,
            $max,
            $display_format,
        )
    };
    (
        overridable
        $id: ident, $min: expr, $max: expr, $display_format: expr$(, $scale: expr)?
    ) => {
        (
            setting::Slider::new(
                |config| *config.$id.inner().global() $(* $scale)*,
                |config, value| config.$id.update(|inner| inner.set_global(value $(/ $scale)*)),
                $min,
                $max,
                $display_format,
            ),
            setting::Slider::new(
                |config| config.$id.inner().game().unwrap() $(* $scale)*,
                |config, value| config.$id.update(|inner| inner.set_game(Some(value $(/ $scale)*))),
                $min,
                $max,
                $display_format,
            ),
        )
    };
}

macro_rules! string_format_slider {
    (
        nonoverridable
        $id: ident, $min: expr, $max: expr, $display_format: expr$(, $scale: expr)?
    ) => {
        setting::StringFormatSlider::new(
            |config| config!(config, $id) $(* $scale)*,
            |config, value| set_config!(config, $id, value $(/ $scale)*),
            $min,
            $max,
            $display_format,
        )
    };
    (
        overridable
        $id: ident, $min: expr, $max: expr, $display_format: expr$(, $scale: expr)?
    ) => {
        (
            setting::StringFormatSlider::new(
                |config| *config.$id.inner().global() $(* $scale)*,
                |config, value| config.$id.update(|inner| inner.set_global(value $(/ $scale)*)),
                $min,
                $max,
                $display_format,
            ),
            setting::StringFormatSlider::new(
                |config| config.$id.inner().game().unwrap() $(* $scale)*,
                |config, value| config.$id.update(|inner| inner.set_game(Some(value $(/ $scale)*))),
                $min,
                $max,
                $display_format,
            ),
        )
    };
}

macro_rules! bool {
    (nonoverridable $id: ident) => {
        setting::Bool::new(
            |config| config!(config, $id),
            |config, value| set_config!(config, $id, value),
        )
    };
    (overridable $id: ident) => {
        (
            setting::Bool::new(
                |config| *config.$id.inner().global(),
                |config, value| config.$id.update(|inner| inner.set_global(value)),
            ),
            setting::Bool::new(
                |config| config.$id.inner().game().unwrap(),
                |config, value| config.$id.update(|inner| inner.set_game(Some(value))),
            ),
        )
    };
}

macro_rules! combo {
    (nonoverridable $id: ident, $items: expr, $label: expr) => {
        setting::Combo::new(
            |config| config!(config, $id),
            |config, value| set_config!(config, $id, value),
            $items,
            $label,
        )
    };
    (overridable $id: ident, $items: expr, $label: expr) => {
        (
            setting::Combo::new(
                |config| *config.$id.inner().global(),
                |config, value| config.$id.update(|inner| inner.set_global(value)),
                $items,
                $label,
            ),
            setting::Combo::new(
                |config| config.$id.inner().game().unwrap(),
                |config, value| config.$id.update(|inner| inner.set_game(Some(value))),
                $items,
                $label,
            ),
        )
    };
}

macro_rules! nonoverridable {
    ($label: literal, $id: ident, $inner: ident$(, $($args: tt)*)?) => {
        setting::NonOverridable::new(
            concat!($label, ": "),
            $inner!(nonoverridable $id$(, $($args)*)*),
            |config| config.$id.set_default(),
        )
    };
}

macro_rules! overridable {
    ($label: literal, $id: ident, $inner: ident$(, $($args: tt)*)?) => {
        setting::Overridable::new(
            concat!($label, ": "),
            $inner!(overridable $id$(, $($args)*)*),
            |config| config.$id.inner().game().is_some(),
            |config, enabled| {
                let value = if enabled {
                    Some(config.$id.inner().global().clone())
                } else {
                    None
                };
                config.$id.update(|inner| inner.set_game(value));
            },
            |config| config.$id.update(|inner| inner.set_default_global()),
            |config| config.$id.update(|inner| inner.set_default_game()),
        )
    };
}

macro_rules! sys_path {
    ($label: literal, $field: ident) => {
        setting::Overridable::new(
            concat!($label, ": "),
            (
                setting::OptHomePath::new(
                    |config| config.sys_paths.inner().global().$field.as_ref(),
                    |config, value| {
                        config.sys_paths.update(|inner| {
                            inner.update_global(|global| {
                                global.$field = value;
                            })
                        });
                    },
                ),
                setting::OptHomePath::new(
                    |config| {
                        config
                            .sys_paths
                            .inner()
                            .game()
                            .$field
                            .as_ref()
                            .unwrap()
                            .as_ref()
                    },
                    |config, value| {
                        config.sys_paths.update(|inner| {
                            inner.update_game(|game| {
                                game.$field = Some(value);
                            })
                        });
                    },
                ),
            ),
            |config| config.sys_paths.inner().game().$field.is_some(),
            |config, enabled| {
                let value = if enabled {
                    Some(Some(
                        config
                            .sys_paths
                            .inner()
                            .global()
                            .$field
                            .clone()
                            .unwrap_or_else(|| HomePathBuf::from("...")),
                    ))
                } else {
                    None
                };
                config
                    .sys_paths
                    .update(|inner| inner.update_game(|game| game.$field = value));
            },
            |config| {
                let value = config.sys_paths.inner().default_global().$field.clone();
                config
                    .sys_paths
                    .update(|inner| inner.update_global(|global| global.$field = value));
            },
            |config| {
                let value = config.sys_paths.inner().default_game().$field.clone();
                config
                    .sys_paths
                    .update(|inner| inner.update_game(|game| game.$field = value));
            },
        )
    };
}

struct PathsSettings {
    imgui_config_path: setting::NonOverridable<setting::OptHomePath>,
    game_db_path: setting::NonOverridable<setting::OptHomePath>,
    sys_dir_path: setting::Overridable<setting::OptHomePath>,
    arm7_bios_path: setting::Overridable<setting::OptHomePath>,
    arm9_bios_path: setting::Overridable<setting::OptHomePath>,
    firmware_path: setting::Overridable<setting::OptHomePath>,
}

impl PathsSettings {
    fn new() -> Self {
        PathsSettings {
            imgui_config_path: nonoverridable!(
                "ImGui config path",
                imgui_config_path,
                opt_home_path
            ),
            game_db_path: nonoverridable!("Game database path", game_db_path, opt_home_path),
            sys_dir_path: sys_path!("System dir path", dir),
            arm7_bios_path: sys_path!("ARM7 BIOS path", arm7_bios),
            arm9_bios_path: sys_path!("ARM9 BIOS path", arm9_bios),
            firmware_path: sys_path!("Firmware path", firmware),
        }
    }
}

struct UiSettings {
    #[cfg(target_os = "macos")]
    title_bar_mode: setting::NonOverridable<setting::Combo<TitleBarMode>>,
    full_window_screen: setting::Overridable<setting::Bool>,
    screen_integer_scale: setting::NonOverridable<setting::Bool>,
    screen_rot: setting::Overridable<setting::Slider<u16>>,
}

impl UiSettings {
    fn new() -> Self {
        UiSettings {
            #[cfg(target_os = "macos")]
            title_bar_mode: nonoverridable!(
                "Title bar mode",
                title_bar_mode,
                combo,
                &[
                    TitleBarMode::System,
                    TitleBarMode::Mixed,
                    TitleBarMode::Imgui,
                ],
                |title_bar_mode| {
                    match title_bar_mode {
                        TitleBarMode::System => "System",
                        TitleBarMode::Mixed => "Mixed",
                        TitleBarMode::Imgui => "Imgui",
                    }
                    .into()
                }
            ),
            full_window_screen: overridable!("Full-window screen", full_window_screen, bool),
            screen_integer_scale: nonoverridable!(
                "Limit screen size to integer scales",
                screen_integer_scale,
                bool
            ),
            screen_rot: overridable!("Screen rotation", screen_rot, slider, 0, 359, "%d°"),
        }
    }
}

struct AudioSettings {
    volume: setting::Overridable<setting::Slider<f32>>,
    sample_chunk_size: setting::Overridable<setting::Scalar<u16>>,
    #[cfg(feature = "xq-audio")]
    custom_sample_rate: setting::Overridable<setting::OptNonZeroU32Slider>,
    #[cfg(feature = "xq-audio")]
    channel_interp_method: setting::Overridable<setting::Combo<AudioChannelInterpMethod>>,
    output_interp_method: setting::Overridable<setting::Combo<audio::InterpMethod>>,
    input_enabled: setting::Overridable<setting::Bool>,
    input_interp_method: setting::Overridable<setting::Combo<audio::InterpMethod>>,
}

impl AudioSettings {
    fn new() -> Self {
        AudioSettings {
            volume: overridable!("Volume", audio_volume, slider, 0.0, 100.0, "%.02f%%", 100.0),
            sample_chunk_size: overridable!(
                "Sample chunk size",
                audio_sample_chunk_size,
                scalar,
                Some(128)
            ),
            #[cfg(feature = "xq-audio")]
            custom_sample_rate: overridable!(
                "Custom sample rate",
                audio_custom_sample_rate,
                opt_nonzero_u32_slider,
                NonZeroU32::new(
                    (audio::output::DEFAULT_INPUT_SAMPLE_RATE as f64
                        * audio::SAMPLE_RATE_ADJUSTMENT_RATIO)
                        .round() as u32
                )
                .unwrap(),
                1 << 14,
                1 << 18,
                "%d Hz"
            ),
            #[cfg(feature = "xq-audio")]
            channel_interp_method: overridable!(
                "Channel interpolation method",
                audio_channel_interp_method,
                combo,
                &[
                    AudioChannelInterpMethod::Nearest,
                    AudioChannelInterpMethod::Cubic,
                ],
                |interp_method| {
                    match interp_method {
                        AudioChannelInterpMethod::Nearest => "Nearest",
                        AudioChannelInterpMethod::Cubic => "Cubic",
                    }
                    .into()
                }
            ),
            output_interp_method: overridable!(
                "Interpolation method",
                audio_output_interp_method,
                combo,
                &[audio::InterpMethod::Nearest, audio::InterpMethod::Cubic],
                |interp_method| {
                    match interp_method {
                        audio::InterpMethod::Nearest => "Nearest",
                        audio::InterpMethod::Cubic => "Cubic",
                    }
                    .into()
                }
            ),
            input_enabled: overridable!("Enabled", audio_input_enabled, bool),
            input_interp_method: overridable!(
                "Interpolation method",
                audio_input_interp_method,
                combo,
                &[audio::InterpMethod::Nearest, audio::InterpMethod::Cubic],
                |interp_method| {
                    match interp_method {
                        audio::InterpMethod::Nearest => "Nearest",
                        audio::InterpMethod::Cubic => "Cubic",
                    }
                    .into()
                }
            ),
        }
    }
}

struct SavesSettings {
    save_interval_ms: setting::Overridable<setting::Scalar<f32>>,
    reset_on_save_slot_switch: setting::NonOverridable<setting::Bool>,
    include_save_in_savestates: setting::Overridable<setting::Bool>,
    save_dir_path: setting::NonOverridable<setting::HomePath>,
    savestate_dir_path: setting::NonOverridable<setting::HomePath>,
}

impl SavesSettings {
    fn new() -> Self {
        SavesSettings {
            save_interval_ms: overridable!(
                "Save interval ms",
                save_interval_ms,
                scalar,
                Some(100.0)
            ),
            reset_on_save_slot_switch: nonoverridable!(
                "Restart on save slot switch",
                reset_on_save_slot_switch,
                bool
            ),
            include_save_in_savestates: overridable!(
                "Include save in states",
                include_save_in_savestates,
                bool
            ),
            save_dir_path: nonoverridable!("Save directory path", save_dir_path, home_path),
            savestate_dir_path: nonoverridable!(
                "Saved state directory path",
                savestate_dir_path,
                home_path
            ),
        }
    }
}

struct EmulationSettings {
    framerate_ratio_limit: setting::Overridable<setting::BoolAndValueSlider<f32>>,
    paused_framerate_limit: setting::Overridable<setting::Slider<f32>>,
    sync_to_audio: setting::Overridable<setting::Bool>,
    pause_on_launch: setting::Overridable<setting::Bool>,
    skip_firmware: setting::Overridable<setting::Bool>,
    prefer_hle_bios: setting::Overridable<setting::Bool>,
    model: setting::Overridable<setting::Combo<ModelConfig>>,
    ds_slot_rom_in_memory_max_size: setting::Overridable<setting::Scalar<u32>>,
    rtc_time_offset_seconds: setting::Overridable<setting::Scalar<i64>>,
    renderer_2d_kind: setting::Overridable<setting::Combo<Renderer2dKind>>,
    renderer_3d_kind: setting::Overridable<setting::Combo<Renderer3dKind>>,
    resolution_scale_shift: setting::Overridable<setting::StringFormatSlider<u8>>,
}

impl EmulationSettings {
    fn new() -> Self {
        EmulationSettings {
            framerate_ratio_limit: overridable!(
                "Framerate limit",
                framerate_ratio_limit,
                bool_and_value_slider,
                12.5,
                800.0,
                "%.02f%%",
                100.0
            ),
            paused_framerate_limit: overridable!(
                "Paused framerate limit",
                paused_framerate_limit,
                slider,
                1.0,
                480.0,
                "%.02f FPS"
            ),
            sync_to_audio: overridable!("Sync to audio", sync_to_audio, bool),
            pause_on_launch: overridable!("Pause on launch", pause_on_launch, bool),
            skip_firmware: overridable!("Skip firmware", skip_firmware, bool),
            prefer_hle_bios: overridable!("Prefer HLE BIOS", prefer_hle_bios, bool),
            model: overridable!(
                "Model",
                model,
                combo,
                &[
                    ModelConfig::Auto,
                    ModelConfig::Ds,
                    ModelConfig::Lite,
                    ModelConfig::Ique,
                    ModelConfig::IqueLite,
                    ModelConfig::Dsi,
                ],
                |model| match model {
                    ModelConfig::Auto => "Auto",
                    ModelConfig::Ds => "DS",
                    ModelConfig::Lite => "DS Lite",
                    ModelConfig::Ique => "IQue DS",
                    ModelConfig::IqueLite => "IQue DS Lite",
                    ModelConfig::Dsi => "DSi",
                }
                .into()
            ),
            ds_slot_rom_in_memory_max_size: overridable!(
                "DS slot ROM in-memory max size",
                ds_slot_rom_in_memory_max_size,
                scalar,
                Some(1024 * 1024)
            ),
            rtc_time_offset_seconds: overridable!(
                "RTC time offset seconds",
                rtc_time_offset_seconds,
                scalar,
                Some(1)
            ),
            renderer_2d_kind: overridable!(
                "2D renderer kind",
                renderer_2d_kind,
                combo,
                &[
                    Renderer2dKind::SoftSync,
                    Renderer2dKind::SoftLockstepScanlines,
                    Renderer2dKind::WgpuLockstepScanlines
                ],
                |kind| match kind {
                    Renderer2dKind::SoftSync => "Software, sync",
                    Renderer2dKind::SoftLockstepScanlines => "Software, async, per-scanline",
                    Renderer2dKind::WgpuLockstepScanlines =>
                        "EXPERIMENTAL: Hardware, async, per-scanline",
                }
                .into()
            ),
            renderer_3d_kind: overridable!(
                "3D renderer kind",
                renderer_3d_kind,
                combo,
                &[Renderer3dKind::Soft, Renderer3dKind::Wgpu],
                |kind| match kind {
                    Renderer3dKind::Soft => "Software",
                    Renderer3dKind::Wgpu => "EXPERIMENTAL: Hardware",
                }
                .into()
            ),
            resolution_scale_shift: overridable!(
                "Resolution scale",
                resolution_scale_shift,
                string_format_slider,
                0,
                3,
                |value| format!("{}x", 1 << value)
            ),
        }
    }
}

#[cfg(any(feature = "log", feature = "gdb-server"))]
struct DebugSettings {
    #[cfg(feature = "log")]
    logging_kind: setting::NonOverridable<setting::Combo<LoggingKind>>,
    #[cfg(feature = "log")]
    imgui_log_history_capacity: setting::Overridable<setting::Scalar<u32>>,
    #[cfg(feature = "gdb-server")]
    gdb_server_addr: setting::NonOverridable<setting::SocketAddr>,
}

#[cfg(any(feature = "log", feature = "gdb-server"))]
impl DebugSettings {
    fn new() -> Self {
        DebugSettings {
            #[cfg(feature = "log")]
            logging_kind: nonoverridable!(
                "Kind",
                logging_kind,
                combo,
                &[LoggingKind::Imgui, LoggingKind::Term],
                |logging_kind| match logging_kind {
                    LoggingKind::Imgui => "ImGui",
                    LoggingKind::Term => "Terminal",
                }
                .into()
            ),
            #[cfg(feature = "log")]
            imgui_log_history_capacity: overridable!(
                "ImGui log history capacity",
                imgui_log_history_capacity,
                scalar,
                Some(1024)
            ),
            #[cfg(feature = "gdb-server")]
            gdb_server_addr: nonoverridable!("GDB server address", gdb_server_addr, socket_addr),
        }
    }
}

#[cfg(feature = "discord-presence")]
struct DiscordPresenceSettings {
    enabled: setting::Overridable<setting::Bool>,
}

#[cfg(feature = "discord-presence")]
impl DiscordPresenceSettings {
    fn new() -> Self {
        DiscordPresenceSettings {
            enabled: overridable!("Enabled", discord_presence_enabled, bool),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Paths,
    Ui,
    Audio,
    Saves,
    Emulation,
    Input,
    #[cfg(any(feature = "log", feature = "gdb-server"))]
    Debug,
    #[cfg(feature = "discord-presence")]
    DiscordPresence,
}

struct Settings {
    paths: PathsSettings,
    ui: UiSettings,
    audio: AudioSettings,
    saves: SavesSettings,
    emulation: EmulationSettings,
    #[cfg(any(feature = "log", feature = "gdb-server"))]
    debug: DebugSettings,
    #[cfg(feature = "discord-presence")]
    discord_presence: DiscordPresenceSettings,
}

impl Settings {
    fn new() -> Self {
        Settings {
            paths: PathsSettings::new(),
            ui: UiSettings::new(),
            audio: AudioSettings::new(),
            saves: SavesSettings::new(),
            emulation: EmulationSettings::new(),
            #[cfg(any(feature = "log", feature = "gdb-server"))]
            debug: DebugSettings::new(),
            #[cfg(feature = "discord-presence")]
            discord_presence: DiscordPresenceSettings::new(),
        }
    }
}

pub(super) struct Editor {
    settings: Settings,
    cur_section: Section,
    input_map_editor: Option<InputMapEditor>,
}

impl Editor {
    pub fn new() -> Self {
        Editor {
            settings: Settings::new(),
            cur_section: Section::Paths,
            input_map_editor: None,
        }
    }

    pub fn process_event(&mut self, event: &winit::event::Event<()>, config: &mut Config) {
        if let Some(input_map_editor) = &mut self.input_map_editor {
            input_map_editor.process_event(event, &mut config.config);
        }
    }

    pub fn emu_stopped(&mut self) {
        if let Some(input_map_editor) = &mut self.input_map_editor {
            input_map_editor.emu_stopped();
        }
    }

    fn draw_game_saves_config(&mut self, ui: &Ui, config: &mut Config, emu_state: &mut EmuState) {
        ui.text("Save path: ");

        ui.same_line();

        let mut enabled = config!(config.config, &save_path_config).is_some();
        if ui.checkbox("##enabled", &mut enabled) {
            set_config!(
                config.config,
                save_path_config,
                if enabled {
                    Some(Default::default())
                } else {
                    None
                }
            );
        }

        if enabled {
            ui.same_line();

            let mut path_config =
                Cow::Borrowed(config!(config.config, &save_path_config).as_ref().unwrap());
            let mut updated = false;

            let combo_width = (ui.content_region_avail()[0] - style!(ui, item_spacing)[0]) * 0.5;

            let mut location_kind = path_config.location.kind();
            ui.set_next_item_width(combo_width);
            if combo_value(
                ui,
                "##location",
                &mut location_kind,
                &[saves::LocationKind::Global, saves::LocationKind::Custom],
                |location_kind| {
                    match location_kind {
                        saves::LocationKind::Global => "Global",
                        saves::LocationKind::Custom => "Custom",
                    }
                    .into()
                },
            ) {
                path_config.to_mut().change_location(
                    location_kind,
                    &config!(config.config, &save_dir_path).0,
                    &emu_state.title,
                );
                updated = true;
            }

            let mut slots_kind = path_config.slots.kind();
            ui.same_line();
            ui.set_next_item_width(combo_width);
            if combo_value(
                ui,
                "##slots",
                &mut slots_kind,
                &[saves::SlotsKind::Single, saves::SlotsKind::Multiple],
                |slots_kind| {
                    match slots_kind {
                        saves::SlotsKind::Single => "Single-slot",
                        saves::SlotsKind::Multiple => "Multi-slot",
                    }
                    .into()
                },
            ) {
                path_config.to_mut().change_slots(
                    slots_kind,
                    &config!(config.config, &save_dir_path).0,
                    &emu_state.title,
                );
                updated = true;
            }

            if let saves::Location::Custom(location) = &path_config.location {
                let mut location = Cow::Borrowed(location);

                let mut location_updated = false;

                let base_width =
                    (ui.content_region_avail()[0] - style!(ui, item_spacing)[0] * 2.0) / 10.0;

                ui.set_next_item_width(base_width * 5.0);
                let mut base_dir_str = location
                    .base_dir
                    .to_string()
                    .map_or_else(|| "<invalid UTF-8>".to_string(), |v| v.to_string());
                if ui
                    .input_text("##base_dir", &mut base_dir_str)
                    .auto_select_all(true)
                    .enter_returns_true(true)
                    .build()
                {
                    location.to_mut().base_dir = HomePathBuf(base_dir_str.into());
                    location_updated = true;
                }

                macro_rules! os_string {
                    ($id: ident) => {{
                        let mut str = location
                            .$id
                            .as_ref()
                            .map(|v| v.to_str().unwrap_or("<invalid UTF-8>"))
                            .unwrap_or("")
                            .to_string();
                        if ui
                            .input_text(concat!("##", stringify!($id)), &mut str)
                            .auto_select_all(true)
                            .enter_returns_true(true)
                            .build()
                        {
                            location.to_mut().$id = (!str.is_empty()).then(|| str.into());
                            location_updated = true;
                        }
                    }};
                }

                ui.same_line();
                ui.set_next_item_width(base_width * 4.0);
                os_string!(base_name);

                ui.same_line();
                ui.set_next_item_width(base_width);
                os_string!(extension);

                if location_updated {
                    path_config.to_mut().location = saves::Location::Custom(location.into_owned());
                    updated = true;
                }
            }

            if updated {
                set_config!(
                    config.config,
                    save_path_config,
                    Some(path_config.into_owned())
                );
            }
        }
    }

    pub fn draw(
        &mut self,
        ui: &Ui,
        config: &mut Config,
        emu_state: Option<&mut EmuState>,
        opened: &mut bool,
    ) {
        let game_loaded = emu_state.as_ref().map_or(false, |e| e.game_loaded);

        ui.window("Configuration").opened(opened).build(|| {
            if let Some(_table) = ui.begin_table_with_flags("##all", 2, TableFlags::BORDERS_INNER_V)
            {
                ui.table_setup_column_with(TableColumnSetup {
                    flags: TableColumnFlags::WIDTH_FIXED,
                    ..TableColumnSetup::new("")
                });
                ui.table_setup_column("");

                ui.table_next_row();
                ui.table_next_column();

                let (top_button_width, bot_button_width) = {
                    let min_button_width = 20.0 + style!(ui, frame_padding)[0] * 2.0;
                    let item_spacing_x = style!(ui, item_spacing)[0];
                    let avail_x = ui.content_region_avail()[0];
                    (
                        ((avail_x - item_spacing_x * 2.0) / 3.0).max(min_button_width),
                        ((avail_x - style!(ui, item_spacing)[0]) / 2.0).max(min_button_width),
                    )
                };

                modify_configs_mask!(
                    ui,
                    width top_button_width,
                    icon_tooltip "\u{f1f8}", "Restore defaults",
                    "restore_defaults",
                    true,
                    game_loaded,
                    |global, game| {
                        if global {
                            config.config.deserialize_global(&config::Global::default());
                        }
                        if game {
                            config.config.deserialize_game(&config::Game::default());
                        }
                    }
                );
                ui.same_line();
                modify_configs_mask!(
                    ui,
                    width top_button_width,
                    icon_tooltip "\u{f2f9}", "Reload",
                    "reload",
                    config.global_path.is_some(),
                    game_loaded && config.game_path.is_some(),
                    |global, game| {
                        if global {
                            if let Ok(config::File { contents, .. }) =
                                config::File::read(config.global_path.as_ref().unwrap(), false)
                            {
                                config.config.deserialize_global(&contents);
                            }
                        }
                        if game {
                            if let Ok(config::File { contents, .. }) =
                                config::File::read(config.game_path.as_ref().unwrap(), false)
                            {
                                config.config.deserialize_game(&contents);
                            }
                        }
                    }
                );
                ui.same_line();
                modify_configs_mask!(
                    ui,
                    width top_button_width,
                    icon_tooltip "\u{f0c7}", "Save",
                    "save",
                    config.global_path.is_some(),
                    game_loaded && config.game_path.is_some(),
                    |global, game| {
                        if global {
                            let _ = config::File {
                                contents: config.config.serialize_global(),
                                path: Some(config.global_path.as_ref().unwrap().clone()),
                            }
                            .write();
                        }
                        if game {
                            let _ = config::File {
                                contents: config.config.serialize_game(),
                                path: Some(config.game_path.as_ref().unwrap().clone()),
                            }
                            .write();
                        }
                    }
                );

                macro_rules! import_config {
                    ($deserialize: ident) => {
                        if let Some(config_file) = FileDialog::new()
                            .add_filter("JSON configuration file", &["json"])
                            .pick_file()
                            .and_then(|path| config::File::read(&path, false).ok())
                        {
                            config.config.$deserialize(&config_file.contents);
                        }
                    };
                }

                modify_configs!(
                    ui,
                    width bot_button_width,
                    icon_tooltip "\u{f56f}", "Import ",
                    "import",
                    game_loaded,
                    import_config!(deserialize_global),
                    import_config!(deserialize_game)
                );

                macro_rules! export_config {
                    ($serialize: ident, $default_file_name: expr) => {
                        if let Some(path) = FileDialog::new()
                            .add_filter("JSON configuration file", &["json"])
                            .set_file_name($default_file_name)
                            .save_file()
                        {
                            let _ = config::File {
                                contents: config.config.$serialize(),
                                path: Some(path),
                            }
                            .write();
                        }
                    };
                }

                ui.same_line();
                modify_configs!(
                    ui,
                    width bot_button_width,
                    icon_tooltip "\u{f56e}", "Export ",
                    "export",
                    game_loaded,
                    export_config!(serialize_global, "global_config.json"),
                    export_config!(
                        serialize_game,
                        &format!("{}.json", emu_state.as_ref().unwrap().title)
                    )
                );

                {
                    let item_spacing = style!(ui, item_spacing)[1];
                    let cursor_pos = ui.cursor_screen_pos();
                    let y = cursor_pos[1] + item_spacing;
                    ui.get_window_draw_list()
                        .add_line(
                            [cursor_pos[0], y],
                            [
                                cursor_pos[0]
                                    + ui.content_region_avail()[0]
                                    + style!(ui, cell_padding)[0],
                                y,
                            ],
                            ui.style_color(StyleColor::Separator),
                        )
                        .build();
                    ui.set_cursor_screen_pos([cursor_pos[0], cursor_pos[1] + item_spacing * 2.0]);
                }

                let labels_and_sections = [
                    ("\u{f07b} Paths", Section::Paths),
                    ("\u{e163} UI", Section::Ui),
                    ("\u{f026} Audio", Section::Audio),
                    ("\u{f0c7} Saves", Section::Saves),
                    ("\u{f2db} Emulation", Section::Emulation),
                    ("\u{f11b} Input", Section::Input),
                    #[cfg(any(feature = "log", feature = "gdb-server"))]
                    ("\u{f7d9} Debug", Section::Debug),
                    #[cfg(feature = "discord-presence")]
                    ("\u{f392} Discord presence", Section::DiscordPresence),
                ];

                ui.child_window("section_list")
                    .size([
                        {
                            let base_width = style!(ui, frame_padding)[0] * 2.0;
                            labels_and_sections
                                .iter()
                                .map(|(label, _)| ui.calc_text_size(label)[0] + base_width)
                                .fold(0.0, f32::max)
                                + style!(ui, scrollbar_size)
                        },
                        ui.content_region_avail()[1] - style!(ui, cell_padding)[1],
                    ])
                    .build(|| {
                        let frame_padding = style!(ui, frame_padding);
                        let padding = [
                            frame_padding[0],
                            frame_padding[1] + style!(ui, item_spacing)[1] * 0.25,
                        ];
                        let double_padding_h = padding[0] * 2.0;
                        let height = padding[1] * 2.0 + ui.text_line_height();

                        for (label, section) in labels_and_sections {
                            let upper_left = ui.cursor_screen_pos();

                            let width = ui.content_region_avail()[0]
                                .max(ui.calc_text_size(label)[0] + double_padding_h);

                            if self.cur_section == section {
                                ui.get_window_draw_list()
                                    .add_rect(
                                        upper_left,
                                        [upper_left[0] + width, upper_left[1] + height],
                                        ui.style_color(StyleColor::ButtonActive),
                                    )
                                    .filled(true)
                                    .build();
                            }

                            if ui.invisible_button(label, [width, height]) {
                                self.cur_section = section;
                            }

                            ui.set_cursor_screen_pos([
                                upper_left[0] + padding[0],
                                upper_left[1] + padding[1],
                            ]);
                            ui.text(label);

                            ui.set_cursor_screen_pos([upper_left[0], upper_left[1] + height]);
                        }
                    });

                ui.table_next_column();

                ui.child_window("section")
                    .size([
                        0.0,
                        ui.content_region_avail()[1] - style!(ui, cell_padding)[1],
                    ])
                    .build(|| {
                        let data = SettingsData {
                            game_loaded: emu_state.as_ref().map_or(false, |e| e.game_loaded),
                        };

                        macro_rules! draw {
                            (
                                $id: literal,
                                $tab: ident,
                                [$($(#[$attr: meta])* $field: ident),*]
                            ) => {
                                if let Some(_table) = ui.begin_table_with_flags(
                                    $id,
                                    4,
                                    TableFlags::SIZING_STRETCH_SAME | TableFlags::NO_CLIP,
                                ) {
                                    ui.table_setup_column_with(TableColumnSetup {
                                        flags: TableColumnFlags::WIDTH_FIXED,
                                        ..TableColumnSetup::new("")
                                    });
                                    ui.table_setup_column("");
                                    ui.table_setup_column_with(TableColumnSetup {
                                        flags: if data.game_loaded {
                                            TableColumnFlags::empty()
                                        } else {
                                            TableColumnFlags::WIDTH_FIXED
                                        },
                                        ..TableColumnSetup::new("")
                                    });
                                    ui.table_setup_column_with(TableColumnSetup {
                                        flags: TableColumnFlags::WIDTH_FIXED,
                                        ..TableColumnSetup::new("")
                                    });
                                    $(
                                        $(#[$attr])*
                                        {
                                            let _id = ui.push_id(stringify!($field));
                                            self.settings.$tab.$field.draw(
                                                ui,
                                                &mut config.config,
                                                &data
                                            );
                                        }
                                    )*
                                }
                            }
                        }

                        match self.cur_section {
                            Section::Paths => {
                                // imgui_config_path
                                // game_db_path
                                // sys_paths

                                draw!("general", paths, [imgui_config_path, game_db_path]);

                                ui.dummy([0.0, 4.0]);
                                heading(ui, "System files", 16.0, 5.0);

                                draw!(
                                    "sys_files",
                                    paths,
                                    [sys_dir_path, arm7_bios_path, arm9_bios_path, firmware_path]
                                );
                            }

                            Section::Ui => {
                                // title_bar_mode
                                // full_window_screen
                                // screen_integer_scale
                                // screen_rot

                                draw!(
                                    "general",
                                    ui,
                                    [
                                        #[cfg(target_os = "macos")]
                                        title_bar_mode,
                                        full_window_screen,
                                        screen_integer_scale,
                                        screen_rot
                                    ]
                                );
                            }

                            Section::Audio => {
                                // audio_volume
                                // audio_sample_chunk_size
                                // audio_custom_sample_rate
                                // audio_channel_interp_method
                                // audio_interp_method

                                draw!("general", audio, [volume, sample_chunk_size]);

                                #[cfg(feature = "xq-audio")]
                                {
                                    ui.dummy([0.0, 4.0]);
                                    heading(ui, "Backend output interpolation", 16.0, 5.0);
                                    draw!(
                                        "backend_interp",
                                        audio,
                                        [custom_sample_rate, channel_interp_method]
                                    );
                                }

                                ui.dummy([0.0, 4.0]);
                                heading(ui, "Frontend output interpolation", 16.0, 5.0);
                                draw!("frontend_interp", audio, [output_interp_method]);

                                ui.dummy([0.0, 4.0]);
                                heading(ui, "Input", 16.0, 5.0);
                                draw!(
                                    "frontend_interp",
                                    audio,
                                    [input_enabled, input_interp_method]
                                );
                            }

                            Section::Saves => {
                                // save_interval_ms
                                // reset_on_save_slot_switch
                                // include_save_in_savestates
                                // save_dir_path
                                // save_path_config

                                draw!(
                                    "general",
                                    saves,
                                    [
                                        save_interval_ms,
                                        reset_on_save_slot_switch,
                                        include_save_in_savestates,
                                        save_dir_path,
                                        savestate_dir_path
                                    ]
                                );

                                ui.dummy([0.0, 4.0]);
                                heading(ui, "Game", 16.0, 5.0);

                                if data.game_loaded {
                                    self.draw_game_saves_config(ui, config, emu_state.unwrap());
                                } else {
                                    ui.text_disabled("Load a game to configure its save path");
                                }
                            }

                            Section::Emulation => {
                                // framerate_ratio_limit
                                // paused_framerate_limit
                                // sync_to_audio
                                // pause_on_launch
                                // skip_firmware
                                // prefer_hle_bios
                                // model
                                // ds_slot_rom_in_memory_max_size
                                // rtc_time_offset_seconds
                                // renderer_2d_kind
                                // renderer_3d_kind
                                // resolution_scale_shift

                                draw!(
                                    "general",
                                    emulation,
                                    [
                                        framerate_ratio_limit,
                                        paused_framerate_limit,
                                        sync_to_audio,
                                        pause_on_launch,
                                        skip_firmware,
                                        prefer_hle_bios,
                                        model,
                                        ds_slot_rom_in_memory_max_size,
                                        rtc_time_offset_seconds,
                                        renderer_2d_kind,
                                        renderer_3d_kind,
                                        resolution_scale_shift
                                    ]
                                );
                            }

                            Section::Input => {
                                self.input_map_editor
                                    .get_or_insert_with(InputMapEditor::new)
                                    .draw(ui, &mut config.config, &data);
                            }

                            #[cfg(any(feature = "log", feature = "gdb-server"))]
                            Section::Debug => {
                                // logging_kind
                                // imgui_log_history_capacity
                                // gdb_server_addr

                                #[cfg(feature = "log")]
                                {
                                    heading(ui, "Logging", 16.0, 5.0);
                                    draw!(
                                        "logging",
                                        debug,
                                        [logging_kind, imgui_log_history_capacity]
                                    );

                                    #[cfg(feature = "gdb-server")]
                                    ui.dummy([0.0, 4.0]);
                                }

                                #[cfg(feature = "gdb-server")]
                                {
                                    heading(ui, "GDB server", 16.0, 5.0);
                                    draw!("gdb_server", debug, [gdb_server_addr]);
                                }
                            }

                            #[cfg(feature = "discord-presence")]
                            Section::DiscordPresence => {
                                // discord_presence_enabled

                                draw!("general", discord_presence, [enabled]);
                            }
                        }

                        if self.cur_section != Section::Input {
                            self.input_map_editor = None;
                        }
                    });
            }
        });
    }
}
