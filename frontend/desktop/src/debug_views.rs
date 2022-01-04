#[macro_use]
#[allow(dead_code)]
mod common;

mod cpu_state;
use cpu_state::CpuState;
mod cpu_memory;
use cpu_memory::CpuMemory;
mod cpu_disasm;
use cpu_disasm::CpuDisasm;
mod palettes_2d;
use palettes_2d::Palettes2D;
mod bg_maps_2d;
use bg_maps_2d::BgMaps2d;
mod audio_channels;
use audio_channels::AudioChannels;

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
    fn handle_emu_state_changed<E: cpu::Engine>(
        prev: Option<&Self::EmuState>,
        new: Option<&Self::EmuState>,
        emu: &mut Emu<E>,
    );
    fn prepare_frame_data<'a, E: cpu::Engine, S: FrameDataSlot<'a, Self::FrameData>>(
        emu_state: &Self::EmuState,
        emu: &mut Emu<E>,
        frame_data: S,
    );

    fn clear_frame_data(&mut self);
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

            pub fn handle_message<E: cpu::Engine>(&mut self, emu: &mut Emu<E>, message: Message) {
                match message {
                    $(
                        Message::$s_toggle_updates_message_ident(enabled) => {
                            if let Some((state, view_enabled)) = &mut self.$s_view_ident {
                                *view_enabled = enabled;
                                if enabled {
                                    <$s_view_ty as View>::handle_emu_state_changed(
                                        None,
                                        Some(&state),
                                        emu,
                                    );
                                } else {
                                    <$s_view_ty as View>::handle_emu_state_changed(
                                        Some(&state),
                                        None,
                                        emu,
                                    );
                                }
                            }
                        }
                        Message::$s_update_emu_state_message_ident(new_state) => {
                            match self.$s_view_ident.as_ref() {
                                Some((prev_state, true)) => {
                                    <$s_view_ty as View>::handle_emu_state_changed(
                                        Some(prev_state),
                                        new_state.as_ref().map(|s| &s.0),
                                        emu,
                                    );
                                }
                                None => {
                                    <$s_view_ty as View>::handle_emu_state_changed(
                                        None,
                                        new_state.as_ref().map(|s| &s.0),
                                        emu,
                                    );
                                }
                                _ => {}
                            }
                            self.$s_view_ident = new_state;
                        }
                    )*
                    $(
                        Message::$i_toggle_updates_message_ident(key, enabled) => {
                            if let Some((state, view_enabled)) = self.$i_view_ident.get_mut(&key) {
                                *view_enabled = enabled;
                                if enabled {
                                    <$i_view_ty as View>::handle_emu_state_changed(
                                        None,
                                        Some(state),
                                        emu,
                                    );
                                } else {
                                    <$i_view_ty as View>::handle_emu_state_changed(
                                        Some(state),
                                        None,
                                        emu,
                                    );
                                }
                            }
                            for (other_key, (state, view_enabled)) in &mut self.$i_view_ident {
                                if *other_key == key || !*view_enabled {
                                    continue;
                                }
                                <$i_view_ty as View>::handle_emu_state_changed(
                                    Some(state),
                                    Some(state),
                                    emu,
                                );
                            }
                        }
                        Message::$i_update_emu_state_message_ident(key, new_state) => {
                            if let Some(new_state) = new_state {
                                match self.$i_view_ident.get(&key) {
                                    Some((prev_state, true)) => {
                                        <$i_view_ty as View>::handle_emu_state_changed(
                                            Some(prev_state),
                                            Some(&new_state.0),
                                            emu,
                                        );
                                    }
                                    None => {
                                        <$i_view_ty as View>::handle_emu_state_changed(
                                            None,
                                            Some(&new_state.0),
                                            emu,
                                        );
                                    }
                                    _ => {}
                                }
                                self.$i_view_ident.insert(key, new_state);
                            } else {
                                if let Some((prev_state, true)) = self.$i_view_ident.remove(&key) {
                                    <$i_view_ty as View>::handle_emu_state_changed(
                                        Some(&prev_state),
                                        None,
                                        emu,
                                    );
                                }
                            }
                            for (other_key, (state, view_enabled)) in &self.$i_view_ident {
                                if *other_key == key || !*view_enabled {
                                    continue;
                                }
                                <$i_view_ty as View>::handle_emu_state_changed(
                                    Some(state),
                                    Some(state),
                                    emu,
                                );
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

            pub fn clear(&mut self) {
                $(
                    self.$s_view_ident = None;
                )*
                $(
                    self.$i_view_ident.clear();
                )*
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

            pub fn clear_frame_data(&mut self) {
                $(
                    if let Some((view, _)) = &mut self.$s_view_ident {
                        view.clear_frame_data();
                    }
                )*
                $(
                    for (view, _) in self.$i_view_ident.values_mut() {
                        view.clear_frame_data();
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
                        *visible = false;
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
                        } else if was_visible != *visible {
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
                            *visible = false;
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
                            } else if was_visible != *visible {
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
    singleton arm7_state, CpuState<false>, ToggleArm7State, UpdateArm7State;
    singleton arm9_state, CpuState<true>, ToggleArm9State, UpdateArm9State;
    instanceable arm7_memory, CpuMemory<false>, ToggleArm7Memory, UpdateArm7Memory;
    instanceable arm9_memory, CpuMemory<true>, ToggleArm9Memory, UpdateArm9Memory;
    instanceable arm7_disasm, CpuDisasm<false>, ToggleArm7Disasm, UpdateArm7Disasm;
    instanceable arm9_disasm, CpuDisasm<true>, ToggleArm9Disasm, UpdateArm9Disasm;
    instanceable palettes_2d, Palettes2D, TogglePalettes2D, UpdatePalettes2D;
    instanceable bg_maps_2d, BgMaps2d, ToggleBgMaps2d, UpdateBgMaps2d;
    instanceable audio_channels, AudioChannels, ToggleAudioChannels, UpdateAudioChannels;
);
