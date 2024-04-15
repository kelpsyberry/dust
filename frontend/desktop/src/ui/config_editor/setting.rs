use super::{SettingsData, Tab};
use crate::{config::Config, ui::utils::combo_value, utils::HomePathBuf};
use imgui::{internal::DataTypeKind, ItemHoveredFlags, SliderFlags, Ui, WindowHoveredFlags};
use rfd::FileDialog;
use std::{
    borrow::Cow, net::SocketAddr as StdSocketAddr, num::NonZeroU32, string::String as StdString,
};

pub trait RawSetting {
    fn draw(&mut self, ui: &Ui, config: &mut Config, tooltip: &str, width: f32);
}

pub struct String {
    pub get: fn(&Config) -> &str,
    pub set: fn(&mut Config, &str),
    buffer: StdString,
}

impl String {
    pub const fn new(get: fn(&Config) -> &str, set: fn(&mut Config, &str)) -> Self {
        String {
            get,
            set,
            buffer: StdString::new(),
        }
    }
}

impl RawSetting for String {
    fn draw(&mut self, ui: &Ui, config: &mut Config, tooltip: &str, width: f32) {
        self.buffer.clear();
        self.buffer.push_str((self.get)(config));

        ui.set_next_item_width(width);
        if ui
            .input_text("", &mut self.buffer)
            .auto_select_all(true)
            .enter_returns_true(true)
            .build()
        {
            (self.set)(config, &self.buffer);
        }

        if !tooltip.is_empty()
            && ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED)
        {
            ui.tooltip_text(tooltip);
        }
    }
}

pub struct HomePath {
    pub get: fn(&Config) -> &HomePathBuf,
    pub set: fn(&mut Config, HomePathBuf),
    buffer: StdString,
}

impl HomePath {
    pub const fn new(get: fn(&Config) -> &HomePathBuf, set: fn(&mut Config, HomePathBuf)) -> Self {
        HomePath {
            get,
            set,
            buffer: StdString::new(),
        }
    }
}

impl RawSetting for HomePath {
    fn draw(&mut self, ui: &Ui, config: &mut Config, tooltip: &str, width: f32) {
        let path = (self.get)(config);
        self.buffer.clear();
        self.buffer.push_str(
            path.to_string()
                .unwrap_or_else(|| "<invalid UTF-8>".into())
                .as_ref(),
        );

        let mut new_value = None;

        ui.set_next_item_width(
            width
                - (ui.calc_text_size("\u{f08e}")[0]
                    + ui.calc_text_size("\u{f07c}")[0]
                    + style!(ui, frame_padding)[0] * 4.0
                    + style!(ui, item_spacing)[0] * 2.0),
        );
        if ui
            .input_text("", &mut self.buffer)
            .auto_select_all(true)
            .enter_returns_true(true)
            .build()
        {
            new_value = Some(HomePathBuf::from(self.buffer.as_str()));
        }

        if !tooltip.is_empty()
            && ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED)
        {
            ui.tooltip_text(tooltip);
        }

        ui.same_line();

        if ui.button("\u{f08e}") {
            let _ = opener::open(&path.0);
        }
        if ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
            ui.tooltip_text("Open");
        }

        ui.same_line();

        if ui.button("\u{f07c}") {
            if let Some(path) = FileDialog::new().pick_folder() {
                new_value = Some(HomePathBuf(path));
            }
        }
        if ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
            ui.tooltip_text("Browse...");
        }

        if let Some(new_value) = new_value {
            (self.set)(config, new_value);
        }
    }
}

pub struct OptHomePath {
    pub get: fn(&Config) -> Option<&HomePathBuf>,
    pub set: fn(&mut Config, Option<HomePathBuf>),
    buffer: StdString,
    placeholder: &'static str,
    is_dir: bool,
}

impl OptHomePath {
    pub const fn new(
        get: fn(&Config) -> Option<&HomePathBuf>,
        set: fn(&mut Config, Option<HomePathBuf>),
        placeholder: &'static str,
        is_dir: bool,
    ) -> Self {
        OptHomePath {
            get,
            set,
            buffer: StdString::new(),
            placeholder,
            is_dir,
        }
    }
}

impl RawSetting for OptHomePath {
    fn draw(&mut self, ui: &Ui, config: &mut Config, tooltip: &str, width: f32) {
        self.buffer.clear();
        let path = (self.get)(config);
        if let Some(path) = path {
            self.buffer.push_str(
                path.to_string()
                    .unwrap_or_else(|| "<invalid UTF-8>".into())
                    .as_ref(),
            );
        }

        let mut new_value = None;

        ui.set_next_item_width(
            width
                - (ui.calc_text_size("\u{f08e}")[0]
                    + ui.calc_text_size("\u{f07c}")[0]
                    + style!(ui, frame_padding)[0] * 4.0
                    + style!(ui, item_spacing)[0] * 2.0),
        );
        if ui
            .input_text("", &mut self.buffer)
            .auto_select_all(true)
            .enter_returns_true(true)
            .hint(self.placeholder)
            .build()
        {
            new_value =
                Some((!self.buffer.is_empty()).then(|| HomePathBuf::from(self.buffer.as_str())));
        }

        if !tooltip.is_empty()
            && ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED)
        {
            ui.tooltip_text(tooltip);
        }

        ui.same_line();

        ui.enabled(path.is_some(), || {
            if ui.button("\u{f08e}") {
                let path = &path.unwrap().0;
                let _ = opener::open(if self.is_dir {
                    path
                } else {
                    path.parent().unwrap_or(path)
                });
            }
            if ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
                ui.tooltip_text(if self.is_dir {
                    "Open folder"
                } else {
                    "Open containing folder"
                });
            }
        });

        ui.same_line();

        if ui.button("\u{f07c}") {
            let path = if self.is_dir {
                FileDialog::new().pick_folder()
            } else {
                FileDialog::new().pick_file()
            };
            if let Some(path) = path {
                new_value = Some(Some(HomePathBuf(path)));
            }
        }
        if ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
            ui.tooltip_text("Browse...");
        }

        if let Some(new_value) = new_value {
            (self.set)(config, new_value);
        }
    }
}

pub struct SocketAddr {
    pub get: fn(&Config) -> StdSocketAddr,
    pub set: fn(&mut Config, StdSocketAddr),
    buffer: StdString,
}

impl SocketAddr {
    pub const fn new(
        get: fn(&Config) -> StdSocketAddr,
        set: fn(&mut Config, StdSocketAddr),
    ) -> Self {
        SocketAddr {
            get,
            set,
            buffer: StdString::new(),
        }
    }
}

impl RawSetting for SocketAddr {
    fn draw(&mut self, ui: &Ui, config: &mut Config, tooltip: &str, width: f32) {
        let mut addr = (self.get)(config);

        self.buffer.clear();
        self.buffer.push_str(&addr.ip().to_string());

        let total_width = width - style!(ui, item_spacing)[0];

        let mut updated = false;
        let mut hovered = false;

        ui.set_next_item_width(total_width * (2.0 / 3.0));
        if ui
            .input_text("##ip", &mut self.buffer)
            .auto_select_all(true)
            .enter_returns_true(true)
            .build()
        {
            if let Ok(ip_addr) = self.buffer.parse() {
                addr.set_ip(ip_addr);
                updated = true;
            }
        }
        hovered |= ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED);

        let mut port = addr.port();
        ui.same_line();
        ui.set_next_item_width(total_width / 3.0);
        if ui.input_scalar("##port", &mut port).step(1).build() {
            addr.set_port(port);
            updated = true;
        }
        hovered |= ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED);

        if updated {
            (self.set)(config, addr);
        }

        if !tooltip.is_empty() && hovered {
            ui.tooltip_text(tooltip);
        }
    }
}

pub struct Scalar<T: DataTypeKind + PartialOrd> {
    pub get: fn(&Config) -> T,
    pub set: fn(&mut Config, T),
    pub step: Option<T>,
    pub max: Option<T>,
    pub display_format: &'static str,
}

impl<T: DataTypeKind + PartialOrd> Scalar<T> {
    pub const fn new(
        get: fn(&Config) -> T,
        set: fn(&mut Config, T),
        step: Option<T>,
        max: Option<T>,
        display_format: &'static str,
    ) -> Self {
        Scalar {
            get,
            set,
            step,
            max,
            display_format,
        }
    }
}

impl<T: DataTypeKind + PartialOrd> RawSetting for Scalar<T> {
    fn draw(&mut self, ui: &Ui, config: &mut Config, tooltip: &str, width: f32) {
        let mut value = (self.get)(config);

        ui.set_next_item_width(width);
        let mut input = ui
            .input_scalar("", &mut value)
            .display_format(self.display_format);
        if let Some(step) = self.step {
            input = input.step(step);
        }
        if input.build() {
            if let Some(max) = self.max {
                if value > max {
                    value = max;
                }
            }

            (self.set)(config, value);
        }

        if !tooltip.is_empty()
            && ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED)
        {
            ui.tooltip_text(tooltip);
        }
    }
}

pub struct OptNonZeroU32Slider {
    pub get: fn(&Config) -> Option<NonZeroU32>,
    pub set: fn(&mut Config, Option<NonZeroU32>),
    pub default: NonZeroU32,
    pub min: u32,
    pub max: u32,
    pub display_format: &'static str,
}

impl OptNonZeroU32Slider {
    pub const fn new(
        get: fn(&Config) -> Option<NonZeroU32>,
        set: fn(&mut Config, Option<NonZeroU32>),
        default: NonZeroU32,
        min: u32,
        max: u32,
        display_format: &'static str,
    ) -> Self {
        OptNonZeroU32Slider {
            get,
            set,
            default,
            min,
            max,
            display_format,
        }
    }
}

impl RawSetting for OptNonZeroU32Slider {
    fn draw(&mut self, ui: &Ui, config: &mut Config, tooltip: &str, width: f32) {
        let mut value = (self.get)(config);

        let mut updated = false;
        let mut hovered = false;

        let checkbox_width = ui.frame_height();
        let input_width = width - checkbox_width - style!(ui, item_spacing)[0];

        if ui.checkbox("##active", &mut value.is_some()) {
            value = if value.is_some() {
                None
            } else {
                Some(self.default)
            };
            updated = true;
        }
        hovered |= ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED);

        if let Some(value_) = value {
            let mut raw_value = value_.get();
            ui.same_line();
            ui.set_next_item_width(input_width);
            if ui
                .slider_config("##value", self.min, self.max)
                .display_format(self.display_format)
                .build(&mut raw_value)
            {
                value = Some(NonZeroU32::new(raw_value).unwrap());
                updated = true;
            }
            hovered |= ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED);
        } else if width > ui.frame_height() {
            ui.same_line_with_spacing(0.0, 0.0);
            ui.dummy([width - ui.frame_height(), 0.0]);
        }

        if updated {
            (self.set)(config, value);
        }

        if !tooltip.is_empty() && hovered {
            ui.tooltip_text(tooltip);
        }
    }
}

pub struct BoolAndValueSlider<T: DataTypeKind> {
    pub get: fn(&Config) -> (bool, T),
    pub set: fn(&mut Config, (bool, T)),
    pub min: T,
    pub max: T,
    pub display_format: &'static str,
}

impl<T: DataTypeKind> BoolAndValueSlider<T> {
    pub const fn new(
        get: fn(&Config) -> (bool, T),
        set: fn(&mut Config, (bool, T)),
        min: T,
        max: T,
        display_format: &'static str,
    ) -> Self {
        BoolAndValueSlider {
            get,
            set,
            min,
            max,
            display_format,
        }
    }
}

impl<T: DataTypeKind> RawSetting for BoolAndValueSlider<T> {
    fn draw(&mut self, ui: &Ui, config: &mut Config, tooltip: &str, width: f32) {
        let (mut is_active, mut value) = (self.get)(config);

        let mut updated = false;
        let mut hovered = false;

        let checkbox_width = ui.frame_height();
        let input_width = width - checkbox_width - style!(ui, item_spacing)[0];

        if ui.checkbox("##active", &mut is_active) {
            updated = true;
        }
        hovered |= ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED);

        if is_active {
            ui.same_line();
            ui.set_next_item_width(input_width);
            if ui
                .slider_config("##value", self.min, self.max)
                .display_format(self.display_format)
                .build(&mut value)
            {
                updated = true;
            }
            hovered |= ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED);
        } else if width > ui.frame_height() {
            ui.same_line_with_spacing(0.0, 0.0);
            ui.dummy([width - ui.frame_height(), 0.0]);
        }

        if updated {
            (self.set)(config, (is_active, value));
        }

        if !tooltip.is_empty() && hovered {
            ui.tooltip_text(tooltip);
        }
    }
}

pub struct Slider<T: DataTypeKind> {
    pub get: fn(&Config) -> T,
    pub set: fn(&mut Config, T),
    pub min: T,
    pub max: T,
    pub display_format: &'static str,
}

impl<T: DataTypeKind> Slider<T> {
    pub const fn new(
        get: fn(&Config) -> T,
        set: fn(&mut Config, T),
        min: T,
        max: T,
        display_format: &'static str,
    ) -> Self {
        Slider {
            get,
            set,
            min,
            max,
            display_format,
        }
    }
}

impl<T: DataTypeKind> RawSetting for Slider<T> {
    fn draw(&mut self, ui: &Ui, config: &mut Config, tooltip: &str, width: f32) {
        let mut value = (self.get)(config);

        ui.set_next_item_width(width);
        if ui
            .slider_config("", self.min, self.max)
            .display_format(self.display_format)
            .flags(SliderFlags::ALWAYS_CLAMP)
            .build(&mut value)
        {
            (self.set)(config, value);
        }

        if !tooltip.is_empty()
            && ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED)
        {
            ui.tooltip_text(tooltip);
        }
    }
}

pub struct StringFormatSlider<T: DataTypeKind> {
    pub get: fn(&Config) -> T,
    pub set: fn(&mut Config, T),
    pub min: T,
    pub max: T,
    pub display_format: fn(T) -> StdString,
}

impl<T: DataTypeKind> StringFormatSlider<T> {
    pub const fn new(
        get: fn(&Config) -> T,
        set: fn(&mut Config, T),
        min: T,
        max: T,
        display_format: fn(T) -> StdString,
    ) -> Self {
        StringFormatSlider {
            get,
            set,
            min,
            max,
            display_format,
        }
    }
}

impl<T: DataTypeKind> RawSetting for StringFormatSlider<T> {
    fn draw(&mut self, ui: &Ui, config: &mut Config, tooltip: &str, width: f32) {
        let mut value = (self.get)(config);

        ui.set_next_item_width(width);
        if ui
            .slider_config("", self.min, self.max)
            .display_format(&(self.display_format)(value))
            .flags(SliderFlags::ALWAYS_CLAMP)
            .build(&mut value)
        {
            (self.set)(config, value);
        }

        if !tooltip.is_empty()
            && ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED)
        {
            ui.tooltip_text(tooltip);
        }
    }
}

pub struct Bool {
    pub get: fn(&Config) -> bool,
    pub set: fn(&mut Config, bool),
}

impl Bool {
    pub const fn new(get: fn(&Config) -> bool, set: fn(&mut Config, bool)) -> Self {
        Bool { get, set }
    }
}

impl RawSetting for Bool {
    fn draw(&mut self, ui: &Ui, config: &mut Config, tooltip: &str, width: f32) {
        let mut value = (self.get)(config);

        if ui.checkbox("", &mut value) {
            (self.set)(config, value);
        }
        let hovered = ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED);
        if width > ui.frame_height() {
            ui.same_line_with_spacing(0.0, 0.0);
            ui.dummy([width - ui.frame_height(), 0.0]);
        }

        if !tooltip.is_empty() && hovered {
            ui.tooltip_text(tooltip);
        }
    }
}

pub struct Combo<T: Clone + PartialEq + 'static> {
    pub get: fn(&Config) -> T,
    pub set: fn(&mut Config, T),
    pub items: &'static [T],
    pub label: for<'a> fn(&'a T) -> Cow<'a, str>,
}

impl<T: Clone + PartialEq + 'static> Combo<T> {
    pub const fn new(
        get: fn(&Config) -> T,
        set: fn(&mut Config, T),
        items: &'static [T],
        label: for<'a> fn(&'a T) -> Cow<'a, str>,
    ) -> Self {
        Combo {
            get,
            set,
            items,
            label,
        }
    }
}

impl<T: Clone + PartialEq + 'static> RawSetting for Combo<T> {
    fn draw(&mut self, ui: &Ui, config: &mut Config, tooltip: &str, width: f32) {
        let mut value = (self.get)(config);

        ui.set_next_item_width(width);
        if combo_value(ui, "", &mut value, self.items, self.label) {
            (self.set)(config, value);
        }

        if !tooltip.is_empty()
            && ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED)
        {
            ui.tooltip_text(tooltip);
        }
    }
}

fn is_row_hovered(ui: &Ui) -> bool {
    use imgui::sys::*;

    let rect_hovered = unsafe {
        let table = &*igGetCurrentTable();
        let mut row_rect = ImRect {
            Min: ImVec2 {
                x: table.WorkRect.Min.x,
                y: table.RowPosY1,
            },
            Max: ImVec2 {
                x: table.WorkRect.Max.x,
                y: table.RowPosY2,
            },
        };
        ImRect_ClipWith(&mut row_rect, table.BgClipRect);
        igIsMouseHoveringRect(row_rect.Min, row_rect.Max, true)
    };

    rect_hovered
        && ui.is_window_hovered_with_flags(WindowHoveredFlags::ALLOW_WHEN_BLOCKED_BY_ACTIVE_ITEM)
}

pub(super) trait Setting {
    fn draw(
        &mut self,
        label_colon: &str,
        label: &str,
        help: &str,
        ui: &Ui,
        config: &mut Config,
        data: &mut SettingsData,
    );
}

pub struct NonOverridable<S: RawSetting> {
    pub inner: S,
    pub reset: fn(&mut Config),
}

impl<S: RawSetting> NonOverridable<S> {
    pub fn new(inner: S, reset: fn(&mut Config)) -> Self {
        NonOverridable { inner, reset }
    }
}

impl<S: RawSetting> Setting for NonOverridable<S> {
    fn draw(
        &mut self,
        label_colon: &str,
        label: &str,
        help: &str,
        ui: &Ui,
        config: &mut Config,
        data: &mut SettingsData,
    ) {
        ui.table_next_row();

        ui.table_next_column();
        ui.align_text_to_frame_padding();
        ui.text(label_colon);

        ui.table_next_column();
        ui.enabled(data.cur_tab == Tab::Global, || {
            self.inner.draw(
                ui,
                config,
                if data.cur_tab == Tab::Global {
                    ""
                } else {
                    "This setting can't be overridden"
                },
                ui.content_region_avail()[0],
            );

            ui.table_next_column();
            if ui.button("\u{f1f8}") {
                (self.reset)(config);
            }
            if ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
                ui.tooltip_text("Set default");
            }
        });

        if data.help_buttons_enabled {
            ui.table_next_column();
            if ui.button("\u{f059}") {
                data.set_help_item(label, help);
            }
            if ui.is_item_hovered() {
                ui.tooltip_text("Help");
            }
        } else if is_row_hovered(ui) {
            data.set_help_item(label, help);
        }
    }
}

pub struct Overridable<S: RawSetting> {
    pub global: S,
    pub game: S,
    pub game_override_enabled: fn(&Config) -> bool,
    pub set_game_override_enabled: fn(&mut Config, enabled: bool),
    pub reset_global: fn(&mut Config),
    pub reset_game: fn(&mut Config),
}

impl<S: RawSetting> Overridable<S> {
    pub fn new(
        (global, game): (S, S),
        game_override_enabled: fn(&Config) -> bool,
        set_game_override_enabled: fn(&mut Config, enabled: bool),
        reset_global: fn(&mut Config),
        reset_game: fn(&mut Config),
    ) -> Self {
        Overridable {
            global,
            game,
            game_override_enabled,
            set_game_override_enabled,
            reset_global,
            reset_game,
        }
    }
}

impl<T: RawSetting> Setting for Overridable<T> {
    fn draw(
        &mut self,
        label_colon: &str,
        label: &str,
        help: &str,
        ui: &Ui,
        config: &mut Config,
        data: &mut SettingsData,
    ) {
        ui.table_next_row();

        ui.table_next_column();
        ui.align_text_to_frame_padding();
        ui.text(label_colon);

        ui.table_next_column();

        let tab_is_global = data.cur_tab == Tab::Global;
        let game_override_enabled = (self.game_override_enabled)(config);
        if tab_is_global {
            let _id = ui.push_id("global");
            self.global.draw(
                ui,
                config,
                if game_override_enabled {
                    "NOTE: Overridden for the current game"
                } else {
                    ""
                },
                ui.content_region_avail()[0],
            );
        } else {
            let button_width = ui.calc_text_size("\u{f055}")[0]
                .max(ui.calc_text_size("\u{f056}")[0])
                + style!(ui, frame_padding)[0] * 2.0;
            let width = ui.content_region_avail()[0] - (button_width + style!(ui, item_spacing)[0]);
            if game_override_enabled {
                let _id = ui.push_id("game");
                self.game.draw(ui, config, "", width);
            } else {
                ui.enabled(false, || {
                    let _id = ui.push_id("global");
                    self.global.draw(ui, config, "Global setting", width);
                });
            }

            ui.same_line();
            let (label, tooltip) = if game_override_enabled {
                ("\u{f056}", "Remove game override")
            } else {
                ("\u{f055}", "Add game override")
            };
            if ui.button_with_size(label, [button_width, 0.0]) {
                (self.set_game_override_enabled)(config, !game_override_enabled);
            }
            if ui.is_item_hovered() {
                ui.tooltip_text(tooltip);
            }
        }

        ui.table_next_column();
        ui.enabled(tab_is_global || game_override_enabled, || {
            if ui.button("\u{f1f8}") {
                match data.cur_tab {
                    Tab::Global => (self.reset_global)(config),
                    Tab::Game => (self.reset_game)(config),
                }
            }
            if ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
                ui.tooltip_text("Set default");
            }
        });

        if data.help_buttons_enabled {
            ui.table_next_column();
            if ui.button("\u{f059}") {
                data.set_help_item(label, help);
            }
            if ui.is_item_hovered() {
                ui.tooltip_text("Help");
            }
        } else if is_row_hovered(ui) {
            data.set_help_item(label, help);
        }
    }
}
