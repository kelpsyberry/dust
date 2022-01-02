mod arm7_state;
use arm7_state::Arm7State;
mod arm7_memory;
use arm7_memory::Arm7Memory;
mod cpu_disasm;
use cpu_disasm::CpuDisasm;
mod arm9_state;
use arm9_state::Arm9State;
mod arm9_memory;
use arm9_memory::Arm9Memory;
mod palettes_2d;
use palettes_2d::Palettes2D;
mod bg_maps_2d;
use bg_maps_2d::BgMaps2d;
#[allow(dead_code)]
mod common;

use super::ui::window::Window;
use dust_core::{cpu, emu::Emu};
use fxhash::FxHashMap;
use std::collections::hash_map::Entry;

pub type ViewKey = u32;

pub trait FrameDataSlot<'a, T> {
    fn insert(self, value: T);
    fn get_or_insert_with(self, f: impl FnOnce() -> T) -> &'a mut T;
}

impl<'a, T> FrameDataSlot<'a, T> for Entry<'a, ViewKey, T> {
    fn insert(self, value: T) {
        match self {
            Entry::Occupied(mut entry) => {
                entry.insert(value);
            }
            Entry::Vacant(entry) => {
                entry.insert(value);
            }
        }
    }
    fn get_or_insert_with(self, f: impl FnOnce() -> T) -> &'a mut T {
        self.or_insert_with(f)
    }
}

impl<'a, T> FrameDataSlot<'a, T> for &'a mut Option<T> {
    fn insert(self, value: T) {
        *self = Some(value);
    }
    fn get_or_insert_with(self, f: impl FnOnce() -> T) -> &'a mut T {
        Option::get_or_insert_with(self, f)
    }
}

pub trait View {
    const NAME: &'static str;

    type FrameData;
    type EmuState: Clone;

    fn new(window: &mut Window) -> Self;
    fn destroy(self, window: &mut Window);

    fn emu_state(&self) -> Self::EmuState;
    fn prepare_frame_data<'a, E: cpu::Engine, S: FrameDataSlot<'a, Self::FrameData>>(
        emu_state: &Self::EmuState,
        emu: &mut Emu<E>,
        frame_data: S,
    );

    fn update_from_frame_data(&mut self, frame_data: &Self::FrameData, window: &mut Window);
    fn customize_window<'ui, 'a, T: AsRef<str>>(
        &mut self,
        ui: &imgui::Ui,
        window: imgui::Window<'ui, 'a, T>,
    ) -> imgui::Window<'ui, 'a, T>;
    fn render(
        &mut self,
        ui: &imgui::Ui,
        window: &mut Window,
        emu_running: bool,
    ) -> Option<Self::EmuState>;
}

macro_rules! declare_structs {
    (
        $(
            singleton
            $s_view_ident: ident,
            $s_view_ty: ty,
            $s_toggle_updates_message_ident: ident,
            $s_update_emu_state_message_ident: ident
        );*$(;)?
        $(
            instanceable
            $i_view_ident: ident,
            $i_view_ty: ty,
            $i_toggle_updates_message_ident: ident,
            $i_update_emu_state_message_ident: ident
        );*$(;)?
    ) => {
        pub enum Message {
            $(
                $s_toggle_updates_message_ident(bool),
                $s_update_emu_state_message_ident(Option<(<$s_view_ty as View>::EmuState, bool)>),
            )*
            $(
                $i_toggle_updates_message_ident(ViewKey, bool),
                $i_update_emu_state_message_ident(
                    ViewKey,
                    Option<(<$i_view_ty as View>::EmuState, bool)>,
                ),
            )*
        }

        #[derive(Clone)]
        pub struct EmuState {
            $(
                $s_view_ident: Option<(<$s_view_ty as View>::EmuState, bool)>,
            )*
            $(
                $i_view_ident: FxHashMap<ViewKey, (<$i_view_ty as View>::EmuState, bool)>,
            )*
        }

        impl EmuState {
            pub fn new() -> Self {
                EmuState {
                    $(
                        $s_view_ident: None,
                    )*
                    $(
                        $i_view_ident: FxHashMap::default(),
                    )*
                }
            }

            pub fn handle_message(&mut self, message: Message) {
                match message {
                    $(
                        Message::$s_toggle_updates_message_ident(enabled) => {
                            if let Some((_, view_enabled)) = &mut self.$s_view_ident {
                                *view_enabled = enabled;
                            }
                        }
                        Message::$s_update_emu_state_message_ident(emu_state) => {
                            self.$s_view_ident = emu_state;
                        }
                    )*
                    $(
                        Message::$i_toggle_updates_message_ident(key, enabled) => {
                            if let Some((_, view_enabled)) = self.$i_view_ident.get_mut(&key) {
                                *view_enabled = enabled;
                            }
                        }
                        Message::$i_update_emu_state_message_ident(key, emu_state) => {
                            if let Some(emu_state) = emu_state {
                                self.$i_view_ident.insert(key, emu_state);
                            } else {
                                self.$i_view_ident.remove(&key);
                            }
                        }
                    )*
                }
            }

            pub fn prepare_frame_data<E: cpu::Engine>(
                &mut self,
                emu: &mut Emu<E>,
                frame_data: &mut FrameData,
            ) {
                $(
                    if let Some((emu_state, visible)) = &self.$s_view_ident {
                        if *visible {
                            <$s_view_ty>::prepare_frame_data(
                                emu_state,
                                emu,
                                &mut frame_data.$s_view_ident,
                            );
                        }
                    } else {
                        frame_data.$s_view_ident = None;
                    }
                )*
                $(
                    frame_data.$i_view_ident.retain(|key, _| self.$i_view_ident.contains_key(key));
                    for (key, (emu_state, visible)) in &self.$i_view_ident {
                        if !*visible {
                            continue;
                        }
                        <$i_view_ty>::prepare_frame_data(
                            emu_state,
                            emu,
                            frame_data.$i_view_ident.entry(*key),
                        );
                    }
                )*
            }
        }

        pub struct FrameData {
            $(
                $s_view_ident: Option<<$s_view_ty as View>::FrameData>,
            )*
            $(
                $i_view_ident: FxHashMap<ViewKey, <$i_view_ty as View>::FrameData>,
            )*
        }

        impl FrameData {
            #[inline]
            #[must_use]
            pub fn new() -> Self {
                FrameData {
                    $(
                        $s_view_ident: None,
                    )*
                    $(
                        $i_view_ident: FxHashMap::default(),
                    )*
                }
            }
        }

        pub struct UiState {
            messages: Vec<Message>,
            $(
                $s_view_ident: Option<($s_view_ty, bool)>,
            )*
            $(
                $i_view_ident: FxHashMap<ViewKey, ($i_view_ty, bool)>,
            )*
        }

        impl UiState {
            #[inline]
            #[must_use]
            pub fn new() -> Self {
                UiState {
                    messages: Vec::new(),
                    $(
                        $s_view_ident: None,
                    )*
                    $(
                        $i_view_ident: FxHashMap::default(),
                    )*
                }
            }

            pub fn update_from_frame_data(&mut self, frame_data: &FrameData, window: &mut Window) {
                $(
                    if let Some((view, visible)) = &mut self.$s_view_ident {
                        if *visible {
                            if let Some(frame_data) = &frame_data.$s_view_ident {
                                view.update_from_frame_data(frame_data, window);
                            }
                        }
                    }
                )*
                $(
                    for (key, (view, visible)) in &mut self.$i_view_ident {
                        if !*visible {
                            continue;
                        }
                        if let Some(frame_data) = frame_data.$i_view_ident.get(key) {
                            view.update_from_frame_data(frame_data, window);
                        }
                    }
                )*
            }

            pub fn reload_emu_state(&mut self) {
                $(
                    if let Some((view, visible)) = &self.$s_view_ident {
                        let emu_state = view.emu_state();
                        self.messages.push(Message::$s_update_emu_state_message_ident(
                            Some((emu_state, *visible)),
                        ));
                    }
                )*
                $(
                    for (key, (view, visible)) in &self.$i_view_ident {
                        let emu_state = view.emu_state();
                        self.messages.push(Message::$i_update_emu_state_message_ident(
                            *key,
                            Some((emu_state, *visible)),
                        ));
                    }
                )*
            }

            pub fn render_menu(&mut self, ui: &imgui::Ui, window: &mut Window) {
                $(
                    if ui.menu_item_config(<$s_view_ty>::NAME)
                        .selected(self.$s_view_ident.is_some())
                        .build() {
                        if let Some(view) = self.$s_view_ident.take() {
                            self.messages.push(Message::$s_update_emu_state_message_ident(
                                None,
                            ));
                            view.0.destroy(window);
                        } else {
                            let view = <$s_view_ty>::new(window);
                            let emu_state = view.emu_state();
                            self.$s_view_ident = Some((view, true));
                            self.messages.push(Message::$s_update_emu_state_message_ident(
                                Some((emu_state, true)),
                            ));
                        }
                    }
                )*
                ui.separator();
                $(
                    if ui.menu_item(<$i_view_ty>::NAME) {
                        let mut key = 1;
                        while self.$i_view_ident.contains_key(&key) {
                            key += 1;
                        }
                        let view = <$i_view_ty>::new(window);
                        let emu_state = view.emu_state();
                        self.$i_view_ident.insert(key, (view, true));
                        self.messages.push(Message::$i_update_emu_state_message_ident(
                            key,
                            Some((emu_state, true)),
                        ));
                    }
                )*
            }

            pub fn render<'a>(
                &'a mut self,
                ui: &imgui::Ui,
                window: &mut Window,
                emu_running: bool,
            ) -> impl Iterator<Item = Message> + 'a {
                $(
                    if let Some((view, visible)) = &mut self.$s_view_ident {
                        let mut opened = true;
                        let was_visible = *visible;
                        let mut new_emu_state = None;
                        view.customize_window(
                            ui,
                            ui.window(<$s_view_ty>::NAME).opened(&mut opened)
                        ).build(|| {
                            *visible = true;
                            new_emu_state = view.render(ui, window, emu_running);
                        });
                        if let Some(new_emu_state) = new_emu_state {
                            self.messages.push(Message::$s_update_emu_state_message_ident(
                                Some((new_emu_state, true))
                            ));
                        } else if !opened {
                            self.messages.push(Message::$s_update_emu_state_message_ident(
                                None,
                            ));
                            self.$s_view_ident.take().unwrap().0.destroy(window);
                        } else if was_visible != !*visible {
                            self.messages.push(Message::$s_toggle_updates_message_ident(
                                *visible,
                            ));
                        }
                    }
                )*
                $(
                    let closed_views: Vec<_> = self.$i_view_ident.drain_filter(
                        |key, (view, visible)| {
                            let mut opened = true;
                            let was_visible = *visible;
                            let mut new_emu_state = None;
                            view.customize_window(
                                ui,
                                ui.window(&format!("{} {}", <$i_view_ty>::NAME, *key))
                                    .opened(&mut opened),
                            ).build(|| {
                                *visible = true;
                                new_emu_state = view.render(ui, window, emu_running);
                            });
                            if let Some(new_emu_state) = new_emu_state {
                                self.messages.push(Message::$i_update_emu_state_message_ident(
                                    *key,
                                    Some((new_emu_state, true))
                                ));
                            } else if !opened {
                                self.messages.push(Message::$i_update_emu_state_message_ident(
                                    *key,
                                    None,
                                ));
                                return true;
                            } else if was_visible != !*visible {
                                self.messages.push(Message::$i_toggle_updates_message_ident(
                                    *key,
                                    *visible,
                                ));
                            }
                            false
                        }
                    ).map(|(_, (view, _))| view).collect();
                    for view in closed_views {
                        view.destroy(window);
                    }
                )*
                self.messages.drain(..)
            }
        }
    };
}

declare_structs!(
    singleton arm7_state, Arm7State, ToggleArm7State, UpdateArm7State;
    singleton arm9_state, Arm9State, ToggleArm9State, UpdateArm9State;
    instanceable arm7_memory, Arm7Memory, ToggleArm7Memory, UpdateArm7Memory;
    instanceable arm7_disasm, CpuDisasm<false>, ToggleArm7Disasm, UpdateArm7Disasm;
    instanceable arm9_memory, Arm9Memory, ToggleArm9Memory, UpdateArm9Memory;
    instanceable arm9_disasm, CpuDisasm<true>, ToggleArm9Disasm, UpdateArm9Disasm;
    instanceable palettes_2d, Palettes2D, TogglePalettes2D, UpdatePalettes2D;
    instanceable bg_maps_2d, BgMaps2d, ToggleBgMaps2d, UpdateBgMaps2d;
);
