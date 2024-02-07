macro_rules! modify_configs_mask {
    (
        $ui: expr, $(width $width: expr,)?
        $(icon_tooltip $icon: literal, $tooltip: literal,)?
        $(label $label: expr,)?
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
                $game = $ui
                    .menu_item_config("Game overrides")
                    .enabled(game_enabled)
                    .build();
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
        $(label $label: expr,)?
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
                if $ui.menu_item("Game overrides") {
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

#[cfg(feature = "logging")]
use crate::config::LoggingKind;
#[cfg(target_os = "macos")]
use crate::config::TitleBarMode;
use crate::{
    audio,
    config::{self, saves, ModelConfig, Renderer2dKind, Renderer3dKind, Setting as _},
    ui::{
        utils::{
            add2, combo_value, heading_options, heading_spacing, sub2, sub2s, table_row_heading,
        },
        Config, EmuState,
    },
    utils::HomePathBuf,
};
#[cfg(feature = "xq-audio")]
use dust_core::audio::ChannelInterpMethod as AudioChannelInterpMethod;
use imgui::{StyleColor, StyleVar, TableColumnFlags, TableColumnSetup, TableFlags, Ui};
use input_map::Editor as InputMapEditor;
use rfd::FileDialog;
use setting::Setting;
use std::borrow::Cow;
#[cfg(feature = "xq-audio")]
use std::num::NonZeroU32;

macro_rules! home_path {
    (nonoverridable $id: ident) => {
        setting::HomePath::new(
            |config| config!(config, &$id),
            |config, value| set_config!(config, $id, value),
        )
    };
}

macro_rules! opt_home_path {
    (nonoverridable $id: ident, $placeholder: expr, $is_dir: expr) => {
        setting::OptHomePath::new(
            |config| config!(config, &$id).as_ref(),
            |config, value| set_config!(config, $id, value),
            $placeholder,
            $is_dir,
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
    (nonoverridable $id: ident, $step: expr, $display_format: expr) => {
        setting::Scalar::new(
            |config| config!(config, $id),
            |config, value| set_config!(config, $id, value),
            $step,
            $display_format,
        )
    };
    (overridable $id: ident, $step: expr, $display_format: expr) => {
        (
            setting::Scalar::new(
                |config| *config.$id.inner().global(),
                |config, value| config.$id.update(|inner| inner.set_global(value)),
                $step,
                $display_format,
            ),
            setting::Scalar::new(
                |config| config.$id.inner().game().unwrap(),
                |config, value| config.$id.update(|inner| inner.set_game(Some(value))),
                $step,
                $display_format,
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
    ($id: ident, $inner: ident$(, $($args: tt)*)?) => {
        setting::NonOverridable::new(
            $inner!(nonoverridable $id$(, $($args)*)*),
            |config| config.$id.set_default(),
        )
    };
}

macro_rules! overridable {
    ($id: ident, $inner: ident$(, $($args: tt)*)?) => {
        setting::Overridable::new(
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
    (
        $field: ident,
        $placeholder: expr,
        $is_dir: expr
    ) => {
        setting::Overridable::new(
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
                    $placeholder,
                    $is_dir,
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
                    $placeholder,
                    $is_dir,
                ),
            ),
            |config| config.sys_paths.inner().game().$field.is_some(),
            |config, enabled| {
                let value = if enabled {
                    Some(config.sys_paths.inner().global().$field.clone())
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
            imgui_config_path: nonoverridable!(imgui_config_path, opt_home_path, "", false),
            game_db_path: nonoverridable!(game_db_path, opt_home_path, "", false),
            sys_dir_path: sys_path!(dir, "", true),
            arm7_bios_path: sys_path!(arm7_bios, "$sys_dir_path/biosnds7.bin", false),
            arm9_bios_path: sys_path!(arm9_bios, "$sys_dir_path/biosnds9.bin", false),
            firmware_path: sys_path!(firmware, "$sys_dir_path/firmware.bin", false),
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
            full_window_screen: overridable!(full_window_screen, bool),
            screen_integer_scale: nonoverridable!(screen_integer_scale, bool),
            screen_rot: overridable!(screen_rot, slider, 0, 359, "%d°"),
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
            volume: overridable!(audio_volume, slider, 0.0, 100.0, "%.02f%%", 100.0),
            sample_chunk_size: overridable!(audio_sample_chunk_size, scalar, Some(128), "%d"),
            #[cfg(feature = "xq-audio")]
            custom_sample_rate: overridable!(
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
            input_enabled: overridable!(audio_input_enabled, bool),
            input_interp_method: overridable!(
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
            save_interval_ms: overridable!(save_interval_ms, scalar, Some(100.0), "%.02f ms"),
            reset_on_save_slot_switch: nonoverridable!(reset_on_save_slot_switch, bool),
            include_save_in_savestates: overridable!(include_save_in_savestates, bool),
            save_dir_path: nonoverridable!(save_dir_path, home_path),
            savestate_dir_path: nonoverridable!(savestate_dir_path, home_path),
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
                framerate_ratio_limit,
                bool_and_value_slider,
                12.5,
                800.0,
                "%.02f%%",
                100.0
            ),
            paused_framerate_limit: overridable!(
                paused_framerate_limit,
                slider,
                1.0,
                480.0,
                "%.02f FPS"
            ),
            sync_to_audio: overridable!(sync_to_audio, bool),
            pause_on_launch: overridable!(pause_on_launch, bool),
            skip_firmware: overridable!(skip_firmware, bool),
            prefer_hle_bios: overridable!(prefer_hle_bios, bool),
            model: overridable!(
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
                ds_slot_rom_in_memory_max_size,
                scalar,
                Some(1024 * 1024),
                "%d B"
            ),
            rtc_time_offset_seconds: overridable!(rtc_time_offset_seconds, scalar, Some(1), "%d s"),
            renderer_2d_kind: overridable!(
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
                resolution_scale_shift,
                string_format_slider,
                0,
                3,
                |value| format!("{}x", 1 << value)
            ),
        }
    }
}

#[cfg(any(feature = "logging", feature = "gdb-server"))]
struct DebugSettings {
    #[cfg(feature = "logging")]
    logging_kind: setting::NonOverridable<setting::Combo<LoggingKind>>,
    #[cfg(feature = "logging")]
    imgui_log_history_capacity: setting::Overridable<setting::Scalar<u32>>,
    #[cfg(feature = "gdb-server")]
    gdb_server_addr: setting::NonOverridable<setting::SocketAddr>,
}

#[cfg(any(feature = "logging", feature = "gdb-server"))]
impl DebugSettings {
    fn new() -> Self {
        DebugSettings {
            #[cfg(feature = "logging")]
            logging_kind: nonoverridable!(
                logging_kind,
                combo,
                &[LoggingKind::Imgui, LoggingKind::Term],
                |logging_kind| match logging_kind {
                    LoggingKind::Imgui => "ImGui",
                    LoggingKind::Term => "Terminal",
                }
                .into()
            ),
            #[cfg(feature = "logging")]
            imgui_log_history_capacity: overridable!(
                imgui_log_history_capacity,
                scalar,
                Some(1024),
                "%d"
            ),
            #[cfg(feature = "gdb-server")]
            gdb_server_addr: nonoverridable!(gdb_server_addr, socket_addr),
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
            enabled: overridable!(discord_presence_enabled, bool),
        }
    }
}

struct Settings {
    paths: PathsSettings,
    ui: UiSettings,
    audio: AudioSettings,
    saves: SavesSettings,
    emulation: EmulationSettings,
    #[cfg(any(feature = "logging", feature = "gdb-server"))]
    debug: DebugSettings,
    #[cfg(feature = "discord-presence")]
    discord_presence: DiscordPresenceSettings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Global,
    Game,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Paths,
    Ui,
    Audio,
    Saves,
    Emulation,
    Input,
    #[cfg(any(feature = "logging", feature = "gdb-server"))]
    Debug,
    #[cfg(feature = "discord-presence")]
    DiscordPresence,
}

struct SettingsData {
    game_loaded: bool,
    help_buttons_enabled: bool,
    cur_tab: Tab,
    cur_help_item: Option<(String, String)>,
    next_help_item: Option<(String, String)>,
}

impl SettingsData {
    fn set_help_item(&mut self, label: &str, help: &str) {
        if let Some((label_, _)) = &self.cur_help_item {
            if label_ == label {
                return;
            }
        }
        self.next_help_item = Some((label.to_string(), help.to_string()));
    }

    fn cur_help_item_or_default(&self) -> (&str, &str) {
        match &self.cur_help_item {
            Some((help_path, help_message)) => (help_path.as_str(), help_message.as_str()),
            None => (
                "No item selected",
                if self.help_buttons_enabled {
                    "Click on the \u{f059} button next to a setting to display its help here."
                } else {
                    "Hover over a setting with the cursor to display its help here."
                },
            ),
        }
    }
}

impl Settings {
    fn new() -> Self {
        Settings {
            paths: PathsSettings::new(),
            ui: UiSettings::new(),
            audio: AudioSettings::new(),
            saves: SavesSettings::new(),
            emulation: EmulationSettings::new(),
            #[cfg(any(feature = "logging", feature = "gdb-server"))]
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
    data: SettingsData,
}

const BORDER_WIDTH: f32 = 1.0;

impl Editor {
    pub fn new() -> Self {
        Editor {
            settings: Settings::new(),
            cur_section: Section::Paths,
            input_map_editor: None,
            data: SettingsData {
                game_loaded: false,
                help_buttons_enabled: false,
                cur_tab: Tab::Global,
                cur_help_item: None,
                next_help_item: None,
            },
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

    fn draw_game_saves_config(&mut self, ui: &Ui, config: &mut Config, emu_state: &EmuState) {
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

    fn draw_control_buttons(&mut self, ui: &Ui, config: &mut Config, emu_state: Option<&EmuState>) {
        let item_spacing = style!(ui, item_spacing);

        {
            let height = 2.0 * ui.frame_height() + item_spacing[1];
            let cell_padding = style!(ui, cell_padding);
            let cursor_pos = ui.cursor_screen_pos();
            let min = sub2(cursor_pos, cell_padding);
            let max = add2(
                add2(cursor_pos, [ui.content_region_avail()[0], height]),
                cell_padding,
            );
            ui.get_window_draw_list()
                .add_rect(min, max, [0.7, 0.7, 0.7, 0.2])
                .filled(true)
                .build();
        }

        let (top_button_width, bot_button_width) = {
            let min_button_width = 20.0 + style!(ui, frame_padding)[0] * 2.0;
            let avail_x = ui.content_region_avail()[0];
            (
                ((avail_x - item_spacing[0] * 2.0) / 3.0).max(min_button_width),
                ((avail_x - item_spacing[0]) / 2.0).max(min_button_width),
            )
        };

        modify_configs_mask!(
            ui,
            width top_button_width,
            icon_tooltip "\u{f1f8}", "Restore defaults",
            "restore_defaults",
            true,
            self.data.game_loaded,
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
            self.data.game_loaded && config.game_path.is_some(),
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
            self.data.game_loaded && config.game_path.is_some(),
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
            self.data.game_loaded,
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
            self.data.game_loaded,
            export_config!(serialize_global, "global_config.json"),
            export_config!(
                serialize_game,
                &format!("{}.json", emu_state.as_ref().unwrap().title)
            )
        );
    }

    fn draw_section_list(&mut self, ui: &Ui) {
        let cell_padding = style!(ui, cell_padding);

        {
            let cursor_pos = ui.cursor_screen_pos();
            let min = sub2(cursor_pos, cell_padding);
            let mut max = add2(cursor_pos, ui.content_region_avail());
            max[0] += cell_padding[0];
            ui.get_window_draw_list()
                .add_rect(min, max, [0.6, 0.6, 0.6, 0.2])
                .filled(true)
                .rounding(style!(ui, window_rounding))
                .round_top_right(false)
                .round_top_left(false)
                .round_bot_left(true)
                .round_bot_right(false)
                .build();
        }

        const LABELS_AND_SECTIONS: &[(&str, Section)] = &[
            ("\u{f07b} Paths", Section::Paths),
            ("\u{e163} UI", Section::Ui),
            ("\u{f026} Audio", Section::Audio),
            ("\u{f0c7} Saves", Section::Saves),
            ("\u{f2db} Emulation", Section::Emulation),
            ("\u{f11b} Input", Section::Input),
            #[cfg(any(feature = "logging", feature = "gdb-server"))]
            ("\u{f7d9} Debug", Section::Debug),
            #[cfg(feature = "discord-presence")]
            ("\u{f392} Discord presence", Section::DiscordPresence),
        ];

        ui.child_window("section_list")
            .size([
                {
                    let base_width = style!(ui, frame_padding)[0] * 2.0;
                    LABELS_AND_SECTIONS
                        .iter()
                        .map(|&(label, _)| ui.calc_text_size(label)[0] + base_width)
                        .fold(0.0, f32::max)
                        + style!(ui, scrollbar_size)
                },
                ui.content_region_avail()[1] - cell_padding[1],
            ])
            .build(|| {
                let frame_padding = style!(ui, frame_padding);
                let padding = [
                    frame_padding[0],
                    frame_padding[1] + style!(ui, item_spacing)[1] * 0.25,
                ];
                let double_padding_h = padding[0] * 2.0;
                let height = padding[1] * 2.0 + ui.text_line_height();

                for &(label, section) in LABELS_AND_SECTIONS {
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
                        self.data.cur_help_item = None;
                        self.data.next_help_item = None;
                        self.cur_section = section;
                    }

                    ui.set_cursor_screen_pos(add2(upper_left, padding));
                    ui.text(label);

                    ui.set_cursor_screen_pos([upper_left[0], upper_left[1] + height]);
                }
            });
    }

    fn draw_section(
        &mut self,
        ui: &Ui,
        config: &mut Config,
        emu_state: Option<&mut EmuState>,
        remaining_height: f32,
        padding: [f32; 2],
        outer_cell_padding: [f32; 2],
    ) {
        let _window_padding = ui.push_style_var(StyleVar::WindowPadding(padding));
        ui.child_window("section")
            .size([
                0.0,
                ui.content_region_avail()[1] - remaining_height - outer_cell_padding[1],
            ])
            .always_use_window_padding(true)
            .build(|| {
                drop(_window_padding);
                self.data.game_loaded = emu_state.as_ref().map_or(false, |e| e.game_loaded);

                let inner_cell_padding = [
                    style!(ui, item_spacing)[0] * 0.5,
                    style!(ui, cell_padding)[1],
                ];

                macro_rules! draw {
                    (
                        $section: expr,
                        $section_struct: ident,
                        [$((
                            $(#[$subsection_attr: meta])*
                            $subsection: literal,
                            [$(
                                $(#[$field_attr: meta])*
                                ($field: ident, $label: literal, $help: expr,)
                            ),*]
                        )),*]
                    ) => {{
                        let _cell_padding =
                            ui.push_style_var(StyleVar::CellPadding(inner_cell_padding));
                        if let Some(_table) = ui.begin_table_with_flags(
                            $section,
                            3 + self.data.help_buttons_enabled as usize,
                            TableFlags::SIZING_STRETCH_SAME | TableFlags::NO_CLIP,
                        ) {
                            ui.table_setup_column_with(TableColumnSetup {
                                flags: TableColumnFlags::WIDTH_FIXED,
                                ..TableColumnSetup::new("")
                            });
                            ui.table_setup_column("");
                            ui.table_setup_column_with(TableColumnSetup {
                                flags: TableColumnFlags::WIDTH_FIXED,
                                ..TableColumnSetup::new("")
                            });
                            if self.data.help_buttons_enabled {
                                ui.table_setup_column_with(TableColumnSetup {
                                    flags: TableColumnFlags::WIDTH_FIXED,
                                    ..TableColumnSetup::new("")
                                });
                            }
                            drop(_cell_padding);
                            let mut spacing = 0.0;
                            $(
                                $(#[$subsection_attr])*
                                {
                                    table_row_heading(
                                        ui,
                                        $subsection,
                                        16.0,
                                        5.0,
                                        -inner_cell_padding[0],
                                        BORDER_WIDTH,
                                        spacing,
                                    );
                                    $(
                                        $(#[$field_attr])*
                                        {
                                            let _id = ui.push_id(stringify!($field));
                                            self.settings.$section_struct.$field.draw(
                                                concat!($label, ": "),
                                                concat!(
                                                    $section,
                                                    " > ",
                                                    $subsection,
                                                    " > ",
                                                    $label,
                                                ),
                                                $help,
                                                ui,
                                                &mut config.config,
                                                &mut self.data
                                            );
                                        }
                                    )*
                                    #[allow(unused_assignments)]
                                    {
                                        spacing = 8.0;
                                    }
                                }
                            )*
                        }
                    }};
                }

                match self.cur_section {
                    Section::Paths => {
                        // imgui_config_path
                        // game_db_path
                        // sys_paths

                        draw!(
                            "Paths",
                            paths,
                            [
                                (
                                    "General",
                                    [
                                        (
                                            imgui_config_path,
                                            "ImGui config",
                                            "The location where the INI configuration file for \
                                             ImGui will be written, used to remember the window \
                                             layout across launches.",
                                        ),
                                        (
                                            game_db_path,
                                            "Game database",
                                            "The location where the JSON game database is stored, \
                                             used to determine save types for games.",
                                        )
                                    ]
                                ),
                                (
                                    "System files",
                                    [
                                        (
                                            sys_dir_path,
                                            "System dir",
                                            "The location of the directory containing the system \
                                             files (biosnds7.bin, biosnds9.bin, firmware.bin); \
                                             can be overridden by the below settings.",
                                        ),
                                        (
                                            arm7_bios_path,
                                            "ARM7 BIOS",
                                            "The location where the ARM7 BIOS binary is stored; \
                                             will default to $sys_dir_path/biosnds7.bin if not \
                                             specified.",
                                        ),
                                        (
                                            arm9_bios_path,
                                            "ARM9 BIOS",
                                            "The location where the ARM9 BIOS binary is stored; \
                                             will default to $sys_dir_path/biosnds9.bin if not \
                                             specified.",
                                        ),
                                        (
                                            firmware_path,
                                            "Firmware",
                                            "The location where the firmware binary is stored; \
                                             will default to $sys_dir_path/firmware.bin if not \
                                             specified.",
                                        )
                                    ]
                                )
                            ]
                        );
                    }

                    Section::Ui => {
                        // title_bar_mode
                        // full_window_screen
                        // screen_integer_scale
                        // screen_rot

                        draw!(
                            "UI",
                            ui,
                            [(
                                "General",
                                [
                                    (
                                        title_bar_mode,
                                        "Title bar mode",
                                        "How to display the title bar:
- System: will use the system title bar and display the emulator's menu under it
- Mixed: will blend the emulator's menu with the transparent system title bar, used to display the \
title and FPS
- Imgui: will completely hide the system title bar and render the title and FPS as part of the \
menu",
                                    ),
                                    (
                                        full_window_screen,
                                        "Full-window screen",
                                        "Whether the screen should be fill the entire emulator \
                                         window background, instead of being rendered as its own \
                                         Imgui window.",
                                    ),
                                    (
                                        screen_integer_scale,
                                        "Limit screen size to integer scales",
                                        "Whether the screen should be shrunk down to limit its \
                                         displayed size to multiples of 256x384 (intended to \
                                         prevent uneven pixel scaling at lower resolutions).",
                                    ),
                                    (
                                        screen_rot,
                                        "Screen rotation",
                                        "The clockwise rotation to apply to the screen in degrees \
                                         (intended for games that require the physical system to \
                                         be rotated).",
                                    )
                                ]
                            )]
                        );
                    }

                    Section::Audio => {
                        // audio_volume
                        // audio_sample_chunk_size
                        // audio_custom_sample_rate
                        // audio_channel_interp_method
                        // audio_interp_method

                        draw!(
                            "Audio",
                            audio,
                            [
                                (
                                    "Output",
                                    [
                                        (
                                            volume,
                                            "Volume",
                                            "Volume to play the console's audio output at.",
                                        ),
                                        (
                                            sample_chunk_size,
                                            "Sample chunk size",
                                            "(Advanced) How many samples to produce in the \
                                             emulator's core before they're queued to be played \
                                             back.",
                                        )
                                    ]
                                ),
                                (
                                    #[cfg(feature = "xq-audio")]
                                    "Backend output interpolation",
                                    [
                                        (
                                            custom_sample_rate,
                                            "Custom sample rate",
                                            "A custom rate at which audio output samples should \
                                             be produced by the console for playback, in Hz.",
                                        ),
                                        (
                                            channel_interp_method,
                                            "Channel interpolation method",
                                            "The interpolation method to apply to the console's \
                                             individual audio channels to map their samples to \
                                             the console's (custom or default) sample rate:
- Nearest: Don't apply any interpolation
- Cubic: Apply cubic interpolation",
                                        )
                                    ]
                                ),
                                (
                                    "Frontend Output Interpolation",
                                    [(
                                        output_interp_method,
                                        "Interpolation method",
                                        "The interpolation method to apply to the console's audio \
                                         output samples to map them to the sample rate of the \
                                         current audio output device:
    - Nearest: Don't apply any interpolation
    - Cubic: Apply cubic interpolation",
                                    )]
                                ),
                                (
                                    "Microphone Input",
                                    [
                                        (
                                            input_enabled,
                                            "Enabled",
                                            "Whether to enable audio input (may ask for \
                                             microphone permissions).",
                                        ),
                                        (
                                            input_interp_method,
                                            "Interpolation method",
                                            "The interpolation method to apply to the current \
                                             audio input device to map its samples to the \
                                             console's audio input sample rate:
- Nearest: Don't apply any interpolation
- Cubic: Apply cubic interpolation",
                                        )
                                    ]
                                )
                            ]
                        );
                    }

                    Section::Saves => {
                        // save_interval_ms
                        // reset_on_save_slot_switch
                        // include_save_in_savestates
                        // save_dir_path
                        // save_path_config

                        draw!(
                            "Saves",
                            saves,
                            [(
                                "General",
                                [
                                    (
                                        save_interval_ms,
                                        "Save interval",
                                        "The interval at which any new save file changes are \
                                         committed to the filesystem.",
                                    ),
                                    (
                                        reset_on_save_slot_switch,
                                        "Restart on save slot switch",
                                        "Whether to restart the emulator when switching save \
                                         slots (not doing so could lead to save file corruption).",
                                    ),
                                    (
                                        include_save_in_savestates,
                                        "Include save in savestates",
                                        "Whether to embed the current version of the save file in \
                                         savestates (not doing so could lead to save file \
                                         corruption due to inconsistencies when loading a \
                                         savestate).",
                                    ),
                                    (
                                        save_dir_path,
                                        "Save directory path",
                                        "The location of the directory where save files for games \
                                         will be stored (unless the specific game is customized \
                                         not to use the global save directory).",
                                    ),
                                    (
                                        savestate_dir_path,
                                        "Savestate directory path",
                                        "The location of the directory where created savestate \
                                         files for games will be stored.",
                                    )
                                ]
                            )]
                        );

                        heading_spacing(
                            ui,
                            &if self.data.game_loaded {
                                format!("Game saves - {}", emu_state.as_deref().unwrap().title)
                            } else {
                                "Game saves".to_string()
                            },
                            16.0,
                            5.0,
                            BORDER_WIDTH,
                            8.0,
                        );
                        if self.data.game_loaded {
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
                            "Emulation",
                            emulation,
                            [(
                                "General",
                                [
                                    (
                                        framerate_ratio_limit,
                                        "Framerate limit",
                                        "The framerate limit to apply to the emulator when \
                                         running, as a percentage of the console's native \
                                         framerate (~60 FPS). I.e., 200% will run emulation at \
                                         120 FPS, or 2x native speed.",
                                    ),
                                    (
                                        paused_framerate_limit,
                                        "Paused framerate limit",
                                        "The framerate limit to apply to the emulator when \
                                         paused, in FPS. This will affect components that reads \
                                         the emulator's state like debug views.",
                                    ),
                                    (
                                        sync_to_audio,
                                        "Sync to audio",
                                        "Whether to sync the emulator to the audio stream's \
                                         playback; this will always limit the emulator to at most \
                                         the console's native speed (or less if a lower framerate \
                                         limit is active).",
                                    ),
                                    (
                                        pause_on_launch,
                                        "Pause on launch",
                                        "Whether to pause the emulator immediately after starting \
                                         a game, requiring it to be resumed manually.",
                                    ),
                                    (
                                        skip_firmware,
                                        "Skip firmware",
                                        "Whether to skip the firmware game selection menu and \
                                         immediately boot the game (required for some homebrew \
                                         titles that don't get recognized by the firmware).
The firmware boot sequence will always be skipped if any system files are not provided.",
                                    ),
                                    (
                                        prefer_hle_bios,
                                        "Prefer HLE BIOS",
                                        "Whether to use the HLE BIOS implementation even if BIOS \
                                         files are provided.",
                                    ),
                                    (
                                        model,
                                        "Model",
                                        "What model of Nintendo DS to emulate (currently only DS \
                                         and DS Lite are functional).",
                                    ),
                                    (
                                        ds_slot_rom_in_memory_max_size,
                                        "DS slot ROM in-memory max size",
                                        "The maximum size that a DS Slot ROM file can have to get \
                                         directly loaded into memory, before falling back to \
                                         streaming from the filesystem.",
                                    ),
                                    (
                                        rtc_time_offset_seconds,
                                        "RTC time offset",
                                        "The offset to apply to the RTC time reported to the \
                                         console compared to the device's local time.",
                                    ),
                                    (
                                        renderer_2d_kind,
                                        "2D renderer kind",
                                        "Which 2D renderer to use:
- Software, sync: render everything synchronously on the emulation thread
- Software, async, per-scanline: render individual scanlines asynchronously on a worker thread
- EXPERIMENTAL: Hardware, async, per-scanline: render individual scanline components \
                                         asynchronously on a worker thread and apply blending, \
                                         layering and color effects using hardware acceleration \
                                         (required when using the hardware 3D renderer)",
                                    ),
                                    (
                                        renderer_3d_kind,
                                        "3D renderer kind",
                                        "Which 3D renderer to use:
- Software: render 3D content asynchronously on a worker thread in software
- EXPERIMENTAL: Hardware, async, per-scanline: render 3D content using hardware acceleration, at a \
                                         higher resolution if selected",
                                    ),
                                    (
                                        resolution_scale_shift,
                                        "3D HW resolution scale",
                                        "With the hardware 3D renderer enabled, the scale at \
                                         which 3D graphics should be rendered compared to the \
                                         native resolution.",
                                    )
                                ]
                            )]
                        );
                    }

                    Section::Input => {
                        self.input_map_editor
                            .get_or_insert_with(InputMapEditor::new)
                            .draw(ui, &mut config.config, &self.data);
                    }

                    #[cfg(any(feature = "logging", feature = "gdb-server"))]
                    Section::Debug => {
                        // logging_kind
                        // imgui_log_history_capacity
                        // gdb_server_addr

                        draw!(
                            "Debug",
                            debug,
                            [
                                (
                                    #[cfg(feature = "logging")]
                                    "Logging",
                                    [
                                        (
                                            logging_kind,
                                            "Kind",
                                            "Whether to show the collected logs inside the \
                                             terminal that launched the emulator or in an Imgui \
                                             window (accessed through Debug > Log)",
                                        ),
                                        (
                                            imgui_log_history_capacity,
                                            "ImGui log history capacity",
                                            "How many log messages to store in the Imgui log \
                                             window before clearing the oldest ones.",
                                        )
                                    ]
                                ),
                                (
                                    #[cfg(feature = "gdb-server")]
                                    "GDB server",
                                    [(
                                        gdb_server_addr,
                                        "GDB server address",
                                        "The address to expose the GDB server at once started.",
                                    )]
                                )
                            ]
                        );
                    }

                    #[cfg(feature = "discord-presence")]
                    Section::DiscordPresence => {
                        // discord_presence_enabled

                        draw!(
                            "Discord presence",
                            discord_presence,
                            [(
                                "General",
                                [(
                                    enabled,
                                    "Enabled",
                                    "Whether to enable Discord Rich Presence. If enabled, the \
                                     current game and its playtime will be shown in the Discord \
                                     status.",
                                )]
                            )]
                        );
                    }
                }

                if self.cur_section != Section::Input {
                    self.input_map_editor = None;
                }
            });
    }

    fn help_height(&self, ui: &Ui, padding: [f32; 2]) -> f32 {
        let (help_path, help_message) = self.data.cur_help_item_or_default();
        ui.calc_text_size_with_opts(help_path, false, ui.content_region_avail()[0])[1]
            + style!(ui, item_spacing)[1] * 5.0
            + ui.calc_text_size_with_opts(help_message, false, ui.content_region_avail()[0])[1]
            + 2.0 * padding[1]
    }

    fn draw_help(&self, ui: &Ui, height: f32, padding: [f32; 2], outer_cell_padding: [f32; 2]) {
        {
            let cursor_pos = ui.cursor_screen_pos();
            let min = [
                cursor_pos[0] - outer_cell_padding[0],
                cursor_pos[1] - outer_cell_padding[1].max(ui.text_line_height() * 0.5),
            ];
            let mut max = add2(cursor_pos, ui.content_region_avail());
            max[0] += outer_cell_padding[0];
            ui.get_window_draw_list()
                .add_rect(min, max, [0.5, 0.5, 0.5, 0.2])
                .filled(true)
                .rounding(style!(ui, window_rounding))
                .round_top_right(false)
                .round_top_left(false)
                .round_bot_left(false)
                .round_bot_right(true)
                .build();
        }

        let _window_padding = ui.push_style_var(StyleVar::WindowPadding(padding));
        ui.child_window("help")
            .size([0.0, height])
            .always_use_window_padding(true)
            .build(|| {
                drop(_window_padding);
                let (help_path, help_message) = self.data.cur_help_item_or_default();
                ui.dummy([0.0; 2]);
                ui.text_wrapped(help_path);
                ui.dummy([0.0; 2]);
                ui.text_wrapped(help_message);
                ui.dummy([0.0; 2]);
                ui.dummy([0.0; 2]);
            });
    }

    pub fn draw(
        &mut self,
        ui: &Ui,
        config: &mut Config,
        emu_state: Option<&mut EmuState>,
        opened: &mut bool,
    ) {
        self.data.game_loaded = emu_state.as_ref().map_or(false, |e| e.game_loaded);

        let _window_padding = ui.push_style_var(StyleVar::WindowPadding([0.0; 2]));
        ui.window("Configuration").opened(opened).build(|| {
            drop(_window_padding);
            let orig_cell_padding = style!(ui, cell_padding);
            let _cell_padding = ui.push_style_var(StyleVar::CellPadding([orig_cell_padding[0]; 2]));
            if let Some(_table) = ui.begin_table_with_flags(
                "##layout",
                2,
                TableFlags::BORDERS_INNER_V | TableFlags::PAD_OUTER_X,
            ) {
                let cell_padding = style!(ui, cell_padding);

                ui.table_setup_column_with(TableColumnSetup {
                    flags: TableColumnFlags::WIDTH_FIXED,
                    ..TableColumnSetup::new("")
                });
                ui.table_setup_column("");

                ui.table_next_row();
                ui.table_next_column();

                // ui.table_set_bg_color(TableBgTarget::CELL_BG, [0.5, 0.5, 0.5, 0.4]);

                self.draw_control_buttons(ui, config, emu_state.as_deref());

                let (separator_p1, separator_p2) = {
                    let mut cursor_pos = ui.cursor_screen_pos();
                    cursor_pos[1] -= style!(ui, item_spacing)[1];
                    let height = cell_padding[1] * 2.0 + BORDER_WIDTH;
                    let x = cursor_pos[0] - cell_padding[0];
                    let y = cursor_pos[1] + cell_padding[1];
                    cursor_pos[1] += height;
                    ui.set_cursor_screen_pos(cursor_pos);
                    (
                        sub2s([x, y], BORDER_WIDTH),
                        sub2s(
                            [x + ui.content_region_avail()[0] + cell_padding[0] * 2.0, y],
                            BORDER_WIDTH,
                        ),
                    )
                };

                self.draw_section_list(ui);

                ui.get_window_draw_list()
                    .add_line(
                        separator_p1,
                        separator_p2,
                        ui.style_color(StyleColor::Separator),
                    )
                    .thickness(BORDER_WIDTH)
                    .build();

                ui.table_next_column();

                let right_padding = [orig_cell_padding[0] * 2.0 - cell_padding[0], 0.0];

                let mut help_height = self.help_height(ui, right_padding);
                let help_header_height = ui.text_line_height().max(cell_padding[1] * 2.0);
                let help_plus_header_height = (help_height + help_header_height)
                    .min((ui.content_region_avail()[1] - style!(ui, cell_padding)[1]) * 0.3);
                help_height = help_plus_header_height - help_header_height;

                if let Some(_tab_bar) = ui.tab_bar("tab") {
                    if ui.tab_item("Global").is_some() {
                        self.data.cur_tab = Tab::Global;
                    }
                    ui.enabled(self.data.game_loaded, || {
                        if if let Some(title) = emu_state.as_deref().and_then(|emu_state| {
                            emu_state
                                .game_loaded
                                .then(|| format!("Game overrides - {}", emu_state.title))
                        }) {
                            ui.tab_item(&title)
                        } else {
                            ui.tab_item("Game overrides")
                        }
                        .is_some()
                        {
                            self.data.cur_tab = Tab::Game;
                        }
                    });
                }

                {
                    let _cell_padding = ui.push_style_var(StyleVar::CellPadding(orig_cell_padding));

                    self.draw_section(
                        ui,
                        config,
                        emu_state,
                        help_plus_header_height,
                        right_padding,
                        cell_padding,
                    );

                    heading_options(
                        ui,
                        "Help",
                        16.0,
                        5.0,
                        -cell_padding[0],
                        -cell_padding[0],
                        BORDER_WIDTH,
                        ui.content_region_avail()[0],
                        2.0 * cell_padding[1],
                        true,
                    );

                    self.draw_help(ui, help_height, right_padding, cell_padding);

                    if let Some(help_item) = self.data.next_help_item.take() {
                        self.data.cur_help_item = Some(help_item);
                    }
                }
            }
        });
    }
}
