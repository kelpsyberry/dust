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
use palettes_2d::Palettes2d;
mod bg_maps_2d;
use bg_maps_2d::BgMaps2d;
mod audio_channels;
use audio_channels::AudioChannels;

use super::ui::window::Window;
use ahash::AHashMap as HashMap;
use dust_core::{cpu, emu::Emu};
use std::collections::hash_map::Entry;

pub type ViewKey = u32;

pub trait FrameDataSlot<'a, T> {
    fn insert(self, value: T);
    fn leave_unchanged(self);
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

    fn leave_unchanged(self) {
        if let Entry::Occupied(entry) = self {
            entry.remove();
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

    fn leave_unchanged(self) {
        *self = None;
    }

    fn get_or_insert_with(self, f: impl FnOnce() -> T) -> &'a mut T {
        Option::get_or_insert_with(self, f)
    }
}

pub trait Messages<T: View> {
    fn push(&mut self, message: <T::EmuState as EmuState>::Message);
}

pub trait EmuState: Sized {
    type InitData: Send;
    type Message: Send;
    type FrameData: Send;

    fn new<E: cpu::Engine>(data: Self::InitData, visible: bool, emu: &mut Emu<E>) -> Self;
    fn destroy<E: cpu::Engine>(self, _emu: &mut Emu<E>) {}

    fn handle_visibility_changed<E: cpu::Engine>(&mut self, _visible: bool, _emu: &mut Emu<E>) {}
    fn handle_message<E: cpu::Engine>(&mut self, _message: Self::Message, _emu: &mut Emu<E>);

    fn prepare_frame_data<'a, E: cpu::Engine, S: FrameDataSlot<'a, Self::FrameData>>(
        &mut self,
        emu: &mut Emu<E>,
        frame_data: S,
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshType {
    Addition,
    Deletion,
    VisibilityChange,
    Message,
}

pub trait InstanceableEmuState: EmuState {
    const ADDITION_TRIGGERS_REFRESH: bool = false;
    const DELETION_TRIGGERS_REFRESH: bool = false;
    fn visibility_change_triggers_refresh(_visible: bool) -> bool {
        false
    }
    fn message_triggers_refresh(_message: &Self::Message) -> bool {
        false
    }

    fn refresh<E: cpu::Engine>(&mut self, _ty: RefreshType, _visible: bool, _emu: &mut Emu<E>) {}
    fn finish_preparing_frame_data<E: cpu::Engine>(_emu: &mut Emu<E>) {}
}

pub trait BaseView: Sized {
    const MENU_NAME: &'static str;

    fn new(window: &mut Window) -> Self;
    fn destroy(self, _window: &mut Window) {}
}

pub trait View: BaseView {
    type EmuState: EmuState;

    fn emu_state(&self) -> <Self::EmuState as EmuState>::InitData;
    fn update_from_frame_data(
        &mut self,
        frame_data: &<Self::EmuState as EmuState>::FrameData,
        window: &mut Window,
    );

    fn draw(&mut self, ui: &imgui::Ui, window: &mut Window, messages: impl Messages<Self>);
}

pub trait StaticView: BaseView {
    type Data: Send;

    fn fetch_data<E: cpu::Engine>(emu: &mut Emu<E>) -> Self::Data;
    fn update_from_data(&mut self, data: Self::Data, window: &mut Window);

    fn draw(&mut self, ui: &imgui::Ui, window: &mut Window);
}

pub trait SingletonView: BaseView {
    fn window<'ui>(
        &mut self,
        ui: &'ui imgui::Ui,
    ) -> imgui::Window<'ui, 'ui, impl AsRef<str> + 'static> {
        ui.window(Self::MENU_NAME)
    }
    fn window_stopped(ui: &'_ imgui::Ui) -> imgui::Window<'_, '_, impl AsRef<str> + 'static> {
        ui.window(Self::MENU_NAME)
    }
}

pub trait InstanceableView: BaseView {
    fn window<'ui>(
        &mut self,
        key: u32,
        ui: &'ui imgui::Ui,
    ) -> imgui::Window<'ui, 'ui, impl AsRef<str> + 'static> {
        ui.window(format!("{} {key}", Self::MENU_NAME))
    }
    fn window_stopped(
        key: u32,
        ui: &'_ imgui::Ui,
    ) -> imgui::Window<'_, '_, impl AsRef<str> + 'static> {
        ui.window(format!("{} {key}", Self::MENU_NAME))
    }
}

macro_rules! declare_structs {
    (
        [$((
            $s_view_ident: ident,
            $s_view_ty: ty,
            $s_init_message_ident: ident,
            $s_destroy_message_ident: ident,
            $s_visibility_changed_message_ident: ident,
            $s_message_ident: ident
        )),*],
        [$((
            $i_view_ident: ident,
            $i_view_ty: ty,
            $i_init_message_ident: ident,
            $i_destroy_message_ident: ident,
            $i_visibility_changed_message_ident: ident,
            $i_message_ident: ident
        )),*],
        [$((
            $sst_view_ident: ident,
            $sst_view_ty: ty,
            $sst_fetch_message_ident: ident,
            $sst_reply_message_ident: ident
        )),*]
    ) => {
        pub enum Message {
            $(
                $s_init_message_ident(<<$s_view_ty as View>::EmuState as EmuState>::InitData, bool),
                $s_destroy_message_ident,
                $s_visibility_changed_message_ident(bool),
                $s_message_ident(<<$s_view_ty as View>::EmuState as EmuState>::Message),
            )*
            $(
                $i_init_message_ident(
                    ViewKey,
                    <<$i_view_ty as View>::EmuState as EmuState>::InitData,
                    bool,
                ),
                $i_destroy_message_ident(ViewKey),
                $i_visibility_changed_message_ident(ViewKey, bool),
                $i_message_ident(ViewKey, <<$i_view_ty as View>::EmuState as EmuState>::Message),
            )*
            $(
                $sst_fetch_message_ident,
            )*
        }

        pub enum Notification {
            $(
                $sst_reply_message_ident(<$sst_view_ty as StaticView>::Data),
            )*
        }

        pub struct ViewsEmuState {
            $(
                $s_view_ident: Option<(<$s_view_ty as View>::EmuState, bool)>,
            )*
            $(
                $i_view_ident: HashMap<ViewKey, (<$i_view_ty as View>::EmuState, bool)>,
            )*
        }

        impl ViewsEmuState {
            pub fn new() -> Self {
                ViewsEmuState {
                    $(
                        $s_view_ident: None,
                    )*
                    $(
                        $i_view_ident: HashMap::default(),
                    )*
                }
            }

            pub fn handle_message<E: cpu::Engine>(
                &mut self,
                emu: &mut Emu<E>,
                message: Message,
            ) -> Option<Notification> {
                match message {
                    $(
                        Message::$s_init_message_ident(data, visible) => {
                            self.$s_view_ident = Some((
                                <<$s_view_ty as View>::EmuState as EmuState>::new(
                                    data,
                                    visible,
                                    emu,
                                ),
                                visible,
                            ));
                        }
                        Message::$s_destroy_message_ident => {
                            if let Some((state, _)) = self.$s_view_ident.take() {
                                state.destroy(emu);
                            }
                        }

                        Message::$s_visibility_changed_message_ident(new_visible) => {
                            if let Some((state, visible)) = &mut self.$s_view_ident {
                                *visible = new_visible;
                                state.handle_visibility_changed(new_visible, emu);
                            }
                        }
                        Message::$s_message_ident(message) => {
                            if let Some((state, _)) = &mut self.$s_view_ident {
                                state.handle_message(message, emu);
                            }
                        }
                    )*
                    $(
                        Message::$i_init_message_ident(key, data, visible) => {
                            self.$i_view_ident.insert(
                                key,
                                (
                                    <<$i_view_ty as View>::EmuState as EmuState>::new(
                                        data,
                                        visible,
                                        emu,
                                    ),
                                    visible,
                                ),
                            );
                            if <$i_view_ty as View>::EmuState::ADDITION_TRIGGERS_REFRESH {
                                for (other_key, (state, visible)) in &mut self.$i_view_ident {
                                    if *other_key != key {
                                        state.refresh(RefreshType::Addition, *visible, emu);
                                    }
                                }
                            }
                        }
                        Message::$i_destroy_message_ident(key) => {
                            if let Some((state, _)) = self.$i_view_ident.remove(&key) {
                                state.destroy(emu);
                            }
                            if <$i_view_ty as View>::EmuState::DELETION_TRIGGERS_REFRESH {
                                for (other_key, (state, visible)) in &mut self.$i_view_ident {
                                    if *other_key != key {
                                        state.refresh(RefreshType::Deletion, *visible, emu);
                                    }
                                }
                            }
                        }

                        Message::$i_visibility_changed_message_ident(key, new_visible) => {
                            if let Some((state, visible)) = &mut self.$i_view_ident.get_mut(&key) {
                                *visible = new_visible;
                                state.handle_visibility_changed(new_visible, emu);
                            }
                            if <$i_view_ty as View>::EmuState::visibility_change_triggers_refresh(
                                new_visible,
                            ) {
                                for (other_key, (state, visible)) in &mut self.$i_view_ident {
                                    if *other_key != key {
                                        state.refresh(RefreshType::VisibilityChange, *visible, emu);
                                    }
                                }
                            }
                        }
                        Message::$i_message_ident(key, message) => {
                            let triggers_refresh =
                                <$i_view_ty as View>::EmuState::message_triggers_refresh(
                                    &message,
                                );
                            if let Some((state, _)) = &mut self.$i_view_ident.get_mut(&key) {
                                state.handle_message(message, emu);
                            }
                            if triggers_refresh {
                                for (other_key, (state, visible)) in &mut self.$i_view_ident {
                                    if *other_key != key {
                                        state.refresh(RefreshType::Message, *visible, emu);
                                    }
                                }
                            }
                        }
                    )*
                    $(
                        Message::$sst_fetch_message_ident => {
                            return Some(Notification::$sst_reply_message_ident(
                                <$sst_view_ty as StaticView>::fetch_data(emu),
                            ));
                        }
                    )*
                }
                None
            }

            pub fn prepare_frame_data<E: cpu::Engine>(
                &mut self,
                emu: &mut Emu<E>,
                frame_data: &mut FrameData,
            ) {
                $(
                    if let Some((state, visible)) = &mut self.$s_view_ident {
                        let data = &mut frame_data.$s_view_ident;
                        if *visible {
                            state.prepare_frame_data(emu, data);
                        } else {
                            data.leave_unchanged();
                        }
                    } else {
                        frame_data.$s_view_ident = None;
                    }
                )*
                $(
                    frame_data.$i_view_ident.retain(|key, _| self.$i_view_ident.contains_key(key));
                    for (key, (state, visible)) in &mut self.$i_view_ident {
                        let data = frame_data.$i_view_ident.entry(*key);
                        if *visible {
                            state.prepare_frame_data(emu, data);
                        } else {
                            data.leave_unchanged();
                        }
                    }
                    <
                        <$i_view_ty as View>::EmuState as InstanceableEmuState
                    >::finish_preparing_frame_data(emu);
                )*
            }
        }

        pub struct FrameData {
            $(
                $s_view_ident: Option<<<$s_view_ty as View>::EmuState as EmuState>::FrameData>,
            )*
            $(
                $i_view_ident: HashMap<
                    ViewKey,
                    <<$i_view_ty as View>::EmuState as EmuState>::FrameData,
                >,
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
                        $i_view_ident: HashMap::default(),
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
                $s_view_ident: Option<(Option<$s_view_ty>, bool)>,
            )*
            $(
                $i_view_ident: HashMap<ViewKey, (Option<$i_view_ty>, bool)>,
            )*
            $(
                $sst_view_ident: Option<Option<$sst_view_ty>>,
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
                        $i_view_ident: HashMap::default(),
                    )*
                    $(
                        $sst_view_ident: None,
                    )*
                }
            }

            pub fn handle_notif(&mut self, notif: Notification, window: &mut Window) {
                match notif {
                    $(
                        Notification::$sst_reply_message_ident(data) => {
                            if let Some(Some(view)) = &mut self.$sst_view_ident {
                                view.update_from_data(data, window);
                            }
                        }
                    )*
                }
            }

            pub fn update_from_frame_data(&mut self, frame_data: &FrameData, window: &mut Window) {
                $(
                    if let Some((Some(view), true)) = &mut self.$s_view_ident {
                        if let Some(frame_data) = &frame_data.$s_view_ident {
                            view.update_from_frame_data(frame_data, window);
                        }
                    }
                )*
                $(
                    for (key, view) in &mut self.$i_view_ident {
                        let (Some(view), true) = view else { continue };
                        if let Some(frame_data) = frame_data.$i_view_ident.get(key) {
                            view.update_from_frame_data(frame_data, window);
                        }
                    }
                )*
            }

            pub fn emu_started(&mut self, window: &mut Window) {
                $(
                    if let Some((view @ None, visible)) = &mut self.$s_view_ident {
                        let view = view.insert(<$s_view_ty>::new(window));
                        let data = view.emu_state();
                        self.messages.push(Message::$s_init_message_ident(data, *visible));
                    }
                )*
                $(
                    for (key, view) in &mut self.$i_view_ident {
                        let (view @ None, visible) = view else { continue };
                        let view = view.insert(<$i_view_ty>::new(window));
                        let data = view.emu_state();
                        self.messages.push(Message::$i_init_message_ident(*key, data, *visible));
                    }
                )*
                $(
                    if let Some(view @ None) = &mut self.$sst_view_ident {
                        *view = Some(<$sst_view_ty>::new(window));
                        self.messages.push(Message::$sst_fetch_message_ident);
                    }
                )*
            }

            pub fn emu_stopped(&mut self, window: &mut Window) {
                $(
                    if let Some((view, _)) = &mut self.$s_view_ident {
                        if let Some(view) = view.take() {
                            view.destroy(window);
                            self.messages.push(Message::$s_destroy_message_ident);
                        };
                    }
                )*
                $(
                    for (key, (view, _)) in &mut self.$i_view_ident {
                        if let Some(view) = view.take() {
                            view.destroy(window);
                            self.messages.push(Message::$i_destroy_message_ident(*key));
                        }
                    }
                )*
                $(
                    if let Some(view) = &mut self.$sst_view_ident {
                        if let Some(view) = view.take() {
                            view.destroy(window);
                        };
                    }
                )*
            }

            pub fn draw_menu(&mut self, emu_running: bool, ui: &imgui::Ui, window: &mut Window) {
                $(
                    if ui.menu_item_config(<$sst_view_ty>::MENU_NAME)
                        .selected(self.$sst_view_ident.is_some())
                        .build()
                    {
                        if let Some(view) = self.$sst_view_ident.take() {
                            if let Some(view) = view {
                                view.destroy(window);
                            }
                        } else {
                            self.$sst_view_ident = Some(if emu_running {
                                self.messages.push(Message::$sst_fetch_message_ident);
                                Some(<$sst_view_ty>::new(window))
                            } else {
                                None
                            });
                        }
                    }
                )*
                ui.separator();
                $(
                    if ui.menu_item_config(<$s_view_ty>::MENU_NAME)
                        .selected(self.$s_view_ident.is_some())
                        .build()
                    {
                        if let Some((view, _)) = self.$s_view_ident.take() {
                            if let Some(view) = view {
                                view.destroy(window);
                                self.messages.push(Message::$s_destroy_message_ident);
                            }
                        } else {
                            self.$s_view_ident = Some((
                                if emu_running {
                                    let view = <$s_view_ty>::new(window);
                                    let data = view.emu_state();
                                    self.messages.push(Message::$s_init_message_ident(data, true));
                                    Some(view)
                                } else {
                                    None
                                },
                                true,
                            ));
                        }
                    }
                )*
                ui.separator();
                $(
                    if ui.menu_item(<$i_view_ty>::MENU_NAME) {
                        let mut key = 1;
                        while self.$i_view_ident.contains_key(&key) {
                            key += 1;
                        }
                        self.$i_view_ident.insert(
                            key,
                            (
                                if emu_running {
                                    let view = <$i_view_ty>::new(window);
                                    let data = view.emu_state();
                                    self.messages.push(
                                        Message::$i_init_message_ident(key, data, true),
                                    );
                                    Some(view)
                                } else {
                                    None
                                },
                                true,
                            ),
                        );
                    }
                )*
            }

            fn draw_unavailable_view(ui: &imgui::Ui) {
                ui.text("Start the emulator to see information.");
            }

            pub fn draw<'a>(
                &'a mut self,
                ui: &imgui::Ui,
                window: &mut Window,
            ) -> impl Iterator<Item = Message> + 'a {
                $(
                    if let Some((view, visible)) = &mut self.$s_view_ident {
                        let mut opened = true;
                        let was_visible = *visible;
                        *visible = false;
                        if let Some(view) = view {
                            let ui_window = view.window(ui).opened(&mut opened);
                            ui_window.build(|| {
                                *visible = true;
                                view.draw(ui, window, &mut self.messages);
                            });
                            if !opened {
                                let Some((Some(view), _)) = self.$s_view_ident.take() else {
                                    unreachable!();
                                };
                                view.destroy(window);
                                self.messages.push(Message::$s_destroy_message_ident);
                            } else if was_visible != *visible {
                                self.messages.push(Message::$s_visibility_changed_message_ident(
                                    *visible,
                                ));
                            }
                        } else {
                            <$s_view_ty>::window_stopped(ui)
                                .opened(&mut opened)
                                .build(|| {
                                    *visible = true;
                                    Self::draw_unavailable_view(ui);
                                });
                            if !opened {
                                self.$s_view_ident.take();
                            }
                        }
                    }
                )*
                $(
                    let closed_views = self.$i_view_ident.extract_if(
                        |key, (view, visible)| {
                            let mut opened = true;
                            let was_visible = *visible;
                            *visible = false;
                            if let Some(view) = view {
                                view.window(*key, ui).opened(&mut opened).build(|| {
                                    *visible = true;
                                    view.draw(ui, window, (&mut self.messages, *key));
                                });
                                if !opened {
                                    self.messages.push(Message::$i_destroy_message_ident(*key));
                                    return true;
                                } else if was_visible != *visible {
                                    self.messages.push(Message::$i_visibility_changed_message_ident(
                                        *key,
                                        *visible,
                                    ));
                                }
                                false
                            } else {
                                <$i_view_ty>::window_stopped(*key, ui)
                                    .opened(&mut opened)
                                    .build(|| {
                                        *visible = true;
                                        Self::draw_unavailable_view(ui);
                                    });
                                !opened
                            }
                        },
                    )
                        .filter_map(|(_, (view, _))| view)
                        .collect::<Vec<_>>();
                    for view in closed_views {
                        view.destroy(window);
                    }
                )*
                $(
                    if let Some(view) = &mut self.$sst_view_ident {
                        let mut opened = true;
                        if let Some(view) = view {
                            let ui_window = view.window(ui).opened(&mut opened);
                            ui_window.build(|| {
                                view.draw(ui, window);
                            });
                            if !opened {
                                let Some(Some(view)) = self.$sst_view_ident.take() else {
                                    unreachable!();
                                };
                                view.destroy(window);
                            }
                        } else {
                            <$sst_view_ty>::window_stopped(ui)
                                .opened(&mut opened)
                                .build(|| {
                                    Self::draw_unavailable_view(ui);
                                });
                            if !opened {
                                self.$sst_view_ident.take();
                            }
                        }
                    }
                )*
                self.messages.drain(..)
            }
        }

        $(
            impl Messages<$s_view_ty> for &mut Vec<Message> {
                fn push(&mut self, message: <<$s_view_ty as View>::EmuState as EmuState>::Message) {
                    Vec::push(self, Message::$s_message_ident(message));
                }
            }
        )*

        $(
            impl Messages<$i_view_ty> for (&mut Vec<Message>, ViewKey) {
                fn push(&mut self, message: <<$i_view_ty as View>::EmuState as EmuState>::Message) {
                    self.0.push(Message::$i_message_ident(self.1, message));
                }
            }
        )*
    };
}

declare_structs!(
    [
        (arm7_state, CpuState<false>, InitArm7State, DestroyArm7State, Arm7StateVisibility, Arm7StateCustom),
        (arm9_state, CpuState<true>, InitArm9State, DestroyArm9State, Arm9StateVisibility, Arm9StateCustom)
    ],
    [
        (arm7_memory, CpuMemory<false>, InitArm7Memory, DestroyArm7Memory, Arm7MemoryVisibility, Arm7MemoryCustom),
        (arm9_memory, CpuMemory<true>, InitArm9Memory, DestroyArm9Memory, Arm9MemoryVisibility, Arm9MemoryCustom),
        (arm7_disasm, CpuDisasm<false>, InitArm7Disasm, DestroyArm7Disasm, Arm7DisasmVisibility, Arm7DisasmCustom),
        (arm9_disasm, CpuDisasm<true>, InitArm9Disasm, DestroyArm9Disasm, Arm9DisasmVisibility, Arm9DisasmCustom),
        (palettes_2d, Palettes2d, InitPalettes2d, DestroyPalettes2d, Palettes2dVisibility, Palettes2dCustom),
        (bg_maps_2d, BgMaps2d, InitBgMaps2d, DestroyBgMaps2d, BgMaps2dVisibility, BgMaps2dCustom),
        (audio_channels, AudioChannels, InitAudioChannels, DestroyAudioChannels, AudioChannelsVisibility, AudioChannelsCustom)
    ],
    []
);
