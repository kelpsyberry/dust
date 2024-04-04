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
mod ds_rom_info;
use ds_rom_info::DsRomInfo;
mod fs;
use fs::Fs;

use super::ui::window::Window;
use ahash::AHashMap as HashMap;
use dust_core::{cpu, emu::Emu};
use std::collections::hash_map::Entry;

pub type ViewKey = u32;

pub trait BaseView: Sized {
    const MENU_NAME: &'static str;
}

// region: Frame views (synced with emulator visual updates, updating every frame)

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

pub trait FrameViewMessages<T: FrameView> {
    fn push(&mut self, message: <T::EmuState as FrameViewEmuState>::Message);
}

pub trait FrameViewEmuState: Sized {
    type InitData: Send;
    type Message: Send;
    type FrameData: Send;

    fn new<E: cpu::Engine>(data: Self::InitData, visible: bool, emu: &mut Emu<E>) -> Self;
    fn destroy<E: cpu::Engine>(self, _emu: &mut Emu<E>) {}

    fn handle_visibility_changed<E: cpu::Engine>(&mut self, _visible: bool, _emu: &mut Emu<E>) {}
    fn handle_message<E: cpu::Engine>(&mut self, message: Self::Message, emu: &mut Emu<E>);

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

pub trait InstanceableFrameViewEmuState: FrameViewEmuState {
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

pub trait FrameView: BaseView {
    type EmuState: FrameViewEmuState;

    fn new(window: &mut Window) -> Self;
    fn destroy(self, _window: &mut Window) {}

    fn emu_state(&self) -> <Self::EmuState as FrameViewEmuState>::InitData;
    fn update_from_frame_data(
        &mut self,
        frame_data: &<Self::EmuState as FrameViewEmuState>::FrameData,
        window: &mut Window,
    );

    fn draw(&mut self, ui: &imgui::Ui, window: &mut Window, messages: impl FrameViewMessages<Self>);
}

// endregion

// region: Message views (no per-frame updates, pass messages between the emulator and the UI)

pub trait MessageViewNotifications<T: MessageViewEmuState> {
    fn push(&mut self, notif: T::Notification);
}

pub trait MessageViewMessages<T: MessageView> {
    fn push(&mut self, message: <T::EmuState as MessageViewEmuState>::Message);
}

pub trait MessageViewEmuState: Sized {
    type InitData: Send;
    type Message: Send;
    type Notification: Send;

    fn new<E: cpu::Engine, N: MessageViewNotifications<Self>>(
        data: Self::InitData,
        visible: bool,
        emu: &mut Emu<E>,
        notifs: N,
    ) -> Self;
    fn destroy<E: cpu::Engine>(self, _emu: &mut Emu<E>) {}

    fn handle_visibility_changed<E: cpu::Engine>(&mut self, _visible: bool, _emu: &mut Emu<E>) {}
    fn handle_message<E: cpu::Engine, N: MessageViewNotifications<Self>>(
        &mut self,
        message: Self::Message,
        emu: &mut Emu<E>,
        notifs: N,
    );

    fn update<E: cpu::Engine, N: MessageViewNotifications<Self>>(
        &mut self,
        _emu: &mut Emu<E>,
        _notifs: N,
    ) {
    }
}

pub trait MessageView: BaseView {
    type EmuState: MessageViewEmuState;

    fn new(window: &mut Window) -> Self;
    fn destroy(self, _window: &mut Window) {}

    fn emu_state(&self) -> <Self::EmuState as MessageViewEmuState>::InitData;
    fn handle_notif(
        &mut self,
        notif: <Self::EmuState as MessageViewEmuState>::Notification,
        window: &mut Window,
    );

    fn draw(
        &mut self,
        ui: &imgui::Ui,
        window: &mut Window,
        messages: impl MessageViewMessages<Self>,
    );
}

// endregion

// region: Static views (request data once then display it)

pub trait StaticView: BaseView {
    type Data: Send;

    fn fetch_data<E: cpu::Engine>(emu: &mut Emu<E>) -> Self::Data;

    fn new(data: Self::Data, window: &mut Window) -> Self;
    fn destroy(self, _window: &mut Window) {}

    fn draw(&mut self, ui: &imgui::Ui, window: &mut Window);
}

// endregion

// region: View types, singleton and instanceable

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

// endregion

enum StaticViewState<T: StaticView> {
    Loading,
    Loaded(T),
}

pub trait Notifications {
    fn push(&mut self, notif: Notification);
}

impl<N: Notifications> Notifications for &mut N {
    fn push(&mut self, notif: Notification) {
        N::push(*self, notif);
    }
}

pub trait Messages {
    fn push(&mut self, notif: Message);
}

impl<N: Messages> Messages for &mut N {
    fn push(&mut self, notif: Message) {
        N::push(*self, notif);
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
            $ss_view_ident: ident,
            $ss_view_ty: ty,
            $ss_fetch_message_ident: ident,
            $ss_reply_message_ident: ident
        )),*],
        [$((
            $im_view_ident: ident,
            $im_view_ty: ty,
            $im_init_message_ident: ident,
            $im_destroy_message_ident: ident,
            $im_visibility_changed_message_ident: ident,
            $im_message_ident: ident,
            $im_notif_ident: ident
        )),*]
    ) => {
        #[allow(clippy::enum_variant_names)]
        pub enum Message {
            $(
                $s_init_message_ident(
                    <<$s_view_ty as FrameView>::EmuState as FrameViewEmuState>::InitData,
                    bool,
                ),
                $s_destroy_message_ident,
                $s_visibility_changed_message_ident(bool),
                $s_message_ident(
                    <<$s_view_ty as FrameView>::EmuState as FrameViewEmuState>::Message,
                ),
            )*
            $(
                $i_init_message_ident(
                    ViewKey,
                    <<$i_view_ty as FrameView>::EmuState as FrameViewEmuState>::InitData,
                    bool,
                ),
                $i_destroy_message_ident(ViewKey),
                $i_visibility_changed_message_ident(ViewKey, bool),
                $i_message_ident(
                    ViewKey,
                    <<$i_view_ty as FrameView>::EmuState as FrameViewEmuState>::Message,
                ),
            )*
            $(
                $ss_fetch_message_ident,
            )*
            $(
                $im_init_message_ident(
                    ViewKey,
                    <<$im_view_ty as MessageView>::EmuState as MessageViewEmuState>::InitData,
                    bool,
                ),
                $im_destroy_message_ident(ViewKey),
                $im_visibility_changed_message_ident(ViewKey, bool),
                $im_message_ident(
                    ViewKey,
                    <<$im_view_ty as MessageView>::EmuState as MessageViewEmuState>::Message,
                ),
            )*
        }

        pub enum Notification {
            $(
                $ss_reply_message_ident(<$ss_view_ty as StaticView>::Data),
            )*
            $(
                $im_notif_ident(
                    ViewKey,
                    <<$im_view_ty as MessageView>::EmuState as MessageViewEmuState>::Notification,
                ),
            )*
        }

        pub struct EmuState {
            $(
                $s_view_ident: Option<(<$s_view_ty as FrameView>::EmuState, bool)>,
            )*
            $(
                $i_view_ident: HashMap<ViewKey, (<$i_view_ty as FrameView>::EmuState, bool)>,
            )*
            $(
                $im_view_ident: HashMap<ViewKey, (<$im_view_ty as MessageView>::EmuState, bool)>,
            )*
        }

        impl EmuState {
            pub fn new() -> Self {
                EmuState {
                    $(
                        $s_view_ident: None,
                    )*
                    $(
                        $i_view_ident: HashMap::default(),
                    )*
                    $(
                        $im_view_ident: HashMap::default(),
                    )*
                }
            }

            pub fn handle_message<E: cpu::Engine>(
                &mut self,
                emu: &mut Emu<E>,
                message: Message,
                mut notifs: impl Notifications,
            ) {
                match message {
                    $(
                        Message::$s_init_message_ident(data, visible) => {
                            self.$s_view_ident = Some((
                                <<$s_view_ty as FrameView>::EmuState as FrameViewEmuState>::new(
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
                                    <<$i_view_ty as FrameView>::EmuState as FrameViewEmuState>::new(
                                        data,
                                        visible,
                                        emu,
                                    ),
                                    visible,
                                ),
                            );
                            if <$i_view_ty as FrameView>::EmuState::ADDITION_TRIGGERS_REFRESH {
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
                            if <$i_view_ty as FrameView>::EmuState::DELETION_TRIGGERS_REFRESH {
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
                            if <
                                $i_view_ty as FrameView
                            >::EmuState::visibility_change_triggers_refresh(
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
                                <$i_view_ty as FrameView>::EmuState::message_triggers_refresh(
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
                        Message::$ss_fetch_message_ident => {
                            notifs.push(Notification::$ss_reply_message_ident(
                                <$ss_view_ty as StaticView>::fetch_data(emu),
                            ));
                        }
                    )*
                    $(
                        Message::$im_init_message_ident(key, data, visible) => {
                            self.$im_view_ident.insert(
                                key,
                                (
                                    <
                                        <$im_view_ty as MessageView
                                    >::EmuState as MessageViewEmuState>::new(
                                        data,
                                        visible,
                                        emu,
                                        (notifs, key),
                                    ),
                                    visible,
                                ),
                            );
                        }
                        Message::$im_destroy_message_ident(key) => {
                            if let Some((state, _)) = self.$im_view_ident.remove(&key) {
                                state.destroy(emu);
                            }
                        }

                        Message::$im_visibility_changed_message_ident(key, new_visible) => {
                            if let Some((state, visible)) = &mut self.$im_view_ident.get_mut(&key) {
                                *visible = new_visible;
                                state.handle_visibility_changed(new_visible, emu);
                            }
                        }
                        Message::$im_message_ident(key, message) => {
                            if let Some((state, _)) = &mut self.$im_view_ident.get_mut(&key) {
                                state.handle_message(message, emu, (notifs, key));
                            }
                        }
                    )*
                }
            }

            pub fn update<E: cpu::Engine>(
                &mut self,
                emu: &mut Emu<E>,
                frame_data: &mut FrameData,
                mut notifs: impl Notifications,
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
                        <$i_view_ty as FrameView>::EmuState as InstanceableFrameViewEmuState
                    >::finish_preparing_frame_data(emu);
                )*
                $(
                    for (key, (state, _)) in &mut self.$im_view_ident {
                        state.update(emu, (&mut notifs, *key));
                    }
                )*
            }
        }

        pub struct FrameData {
            $(
                $s_view_ident: Option<
                    <<$s_view_ty as FrameView>::EmuState as FrameViewEmuState>::FrameData
                >,
            )*
            $(
                $i_view_ident: HashMap<
                    ViewKey,
                    <<$i_view_ty as FrameView>::EmuState as FrameViewEmuState>::FrameData,
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
            $(
                $s_view_ident: Option<(Option<$s_view_ty>, bool)>,
            )*
            $(
                $i_view_ident: HashMap<ViewKey, (Option<$i_view_ty>, bool)>,
            )*
            $(
                $ss_view_ident: Option<Option<StaticViewState<$ss_view_ty>>>,
            )*
            $(
                $im_view_ident: HashMap<ViewKey, (Option<$im_view_ty>, bool)>,
            )*
        }

        impl UiState {
            #[inline]
            pub fn new() -> Self {
                UiState {
                    $(
                        $s_view_ident: None,
                    )*
                    $(
                        $i_view_ident: HashMap::default(),
                    )*
                    $(
                        $ss_view_ident: None,
                    )*
                    $(
                        $im_view_ident: HashMap::default(),
                    )*
                }
            }

            pub fn handle_notif(&mut self, notif: Notification, window: &mut Window) {
                match notif {
                    $(
                        Notification::$ss_reply_message_ident(data) => {
                            if let Some(Some(view @ StaticViewState::Loading)) =
                                &mut self.$ss_view_ident
                            {
                                *view = StaticViewState::Loaded(<$ss_view_ty>::new(data, window));
                            }
                        }
                    )*
                    $(
                        Notification::$im_notif_ident(key, notif) => {
                            if let Some((Some(view), _)) = self.$im_view_ident.get_mut(&key) {
                                view.handle_notif(notif, window);
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

            pub fn emu_started(
                &mut self,
                window: &mut Window,
                mut messages: impl Messages,
            ) {
                $(
                    if let Some((view @ None, visible)) = &mut self.$s_view_ident {
                        let view = view.insert(<$s_view_ty>::new(window));
                        let data = view.emu_state();
                        messages.push(Message::$s_init_message_ident(data, *visible));
                    }
                )*
                $(
                    for (key, view) in &mut self.$i_view_ident {
                        let (view @ None, visible) = view else { continue };
                        let view = view.insert(<$i_view_ty>::new(window));
                        let data = view.emu_state();
                        messages.push(Message::$i_init_message_ident(*key, data, *visible));
                    }
                )*
                $(
                    if let Some(view @ None) = &mut self.$ss_view_ident {
                        *view = Some(StaticViewState::Loading);
                        messages.push(Message::$ss_fetch_message_ident);
                    }
                )*
                $(
                    for (key, view) in &mut self.$im_view_ident {
                        let (view @ None, visible) = view else { continue };
                        let view = view.insert(<$im_view_ty>::new(window));
                        let data = view.emu_state();
                        messages.push(Message::$im_init_message_ident(*key, data, *visible));
                    }
                )*
            }

            pub fn emu_stopped(
                &mut self,
                window: &mut Window,
                mut messages: impl Messages,
            ) {
                $(
                    if let Some((view, _)) = &mut self.$s_view_ident {
                        if let Some(view) = view.take() {
                            view.destroy(window);
                            messages.push(Message::$s_destroy_message_ident);
                        };
                    }
                )*
                $(
                    for (key, (view, _)) in &mut self.$i_view_ident {
                        if let Some(view) = view.take() {
                            view.destroy(window);
                            messages.push(Message::$i_destroy_message_ident(*key));
                        }
                    }
                )*
                $(
                    if let Some(view) = &mut self.$ss_view_ident {
                        if let Some(StaticViewState::Loaded(view)) = view.take() {
                            view.destroy(window);
                        };
                    }
                )*
                $(
                    for (key, (view, _)) in &mut self.$im_view_ident {
                        if let Some(view) = view.take() {
                            view.destroy(window);
                            messages.push(Message::$im_destroy_message_ident(*key));
                        }
                    }
                )*
            }

            pub fn draw_menu(
                &mut self,
                ui: &imgui::Ui,
                window: &mut Window,
                mut messages: Option<impl Messages>,
            ) {
                $(
                    if ui.menu_item_config(<$ss_view_ty>::MENU_NAME)
                        .selected(self.$ss_view_ident.is_some())
                        .build()
                    {
                        if let Some(view) = self.$ss_view_ident.take() {
                            if let Some(StaticViewState::Loaded(view)) = view {
                                view.destroy(window);
                            }
                        } else {
                            self.$ss_view_ident = Some(if let Some(messages) = &mut messages {
                                messages.push(Message::$ss_fetch_message_ident);
                                Some(StaticViewState::Loading)
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
                                messages.as_mut().unwrap().push(Message::$s_destroy_message_ident);
                            }
                        } else {
                            self.$s_view_ident = Some((
                                if let Some(messages) = &mut messages {
                                    let view = <$s_view_ty>::new(window);
                                    let data = view.emu_state();
                                    messages.push(Message::$s_init_message_ident(data, true));
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
                                if let Some(messages) = &mut messages {
                                    let view = <$i_view_ty>::new(window);
                                    let data = view.emu_state();
                                    messages.push(Message::$i_init_message_ident(key, data, true));
                                    Some(view)
                                } else {
                                    None
                                },
                                true,
                            ),
                        );
                    }
                )*
                $(
                    if ui.menu_item(<$im_view_ty>::MENU_NAME) {
                        let mut key = 1;
                        while self.$im_view_ident.contains_key(&key) {
                            key += 1;
                        }
                        self.$im_view_ident.insert(
                            key,
                            (
                                if let Some(messages) = &mut messages {
                                    let view = <$im_view_ty>::new(window);
                                    let data = view.emu_state();
                                    messages.push(Message::$im_init_message_ident(key, data, true));
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

            fn draw_loading_view(ui: &imgui::Ui) {
                ui.text("Loading...");
            }

            pub fn draw(
                &mut self,
                ui: &imgui::Ui,
                window: &mut Window,
                mut messages: Option<impl Messages>,
            ) {
                $(
                    if let Some((view, visible)) = &mut self.$s_view_ident {
                        let messages = messages.as_mut().unwrap();
                        let mut opened = true;
                        let was_visible = *visible;
                        *visible = false;
                        if let Some(view) = view {
                            let ui_window = view.window(ui).opened(&mut opened);
                            ui_window.build(|| {
                                *visible = true;
                                view.draw(ui, window, &mut *messages);
                            });
                            if !opened {
                                let Some((Some(view), _)) = self.$s_view_ident.take() else {
                                    unreachable!();
                                };
                                view.destroy(window);
                                messages.push(Message::$s_destroy_message_ident);
                            } else if was_visible != *visible {
                                messages.push(Message::$s_visibility_changed_message_ident(
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
                                let messages = messages.as_mut().unwrap();
                                view.window(*key, ui).opened(&mut opened).build(|| {
                                    *visible = true;
                                    view.draw(ui, window, (&mut *messages, *key));
                                });
                                if !opened {
                                    messages.push(Message::$i_destroy_message_ident(*key));
                                    return true;
                                } else if was_visible != *visible {
                                    messages.push(Message::$i_visibility_changed_message_ident(
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
                    if let Some(view) = &mut self.$ss_view_ident {
                        let mut opened = true;
                        if let Some(view) = view {
                            if let StaticViewState::Loaded(view) = view {
                                let ui_window = view.window(ui).opened(&mut opened);
                                ui_window.build(|| {
                                    view.draw(ui, window);
                                });
                                if !opened {
                                    let Some(Some(StaticViewState::Loaded(view))) =
                                        self.$ss_view_ident.take() else {
                                        unreachable!();
                                    };
                                    view.destroy(window);
                                }
                            } else {
                                <$ss_view_ty>::window_stopped(ui)
                                    .opened(&mut opened)
                                    .build(|| {
                                        Self::draw_loading_view(ui);
                                    });
                                if !opened {
                                    self.$ss_view_ident.take();
                                }
                            }
                        } else {
                            <$ss_view_ty>::window_stopped(ui)
                                .opened(&mut opened)
                                .build(|| {
                                    Self::draw_unavailable_view(ui);
                                });
                            if !opened {
                                self.$ss_view_ident.take();
                            }
                        }
                    }
                )*
                $(
                    let closed_views = self.$im_view_ident.extract_if(
                        |key, (view, visible)| {
                            let mut opened = true;
                            let was_visible = *visible;
                            *visible = false;
                            if let Some(view) = view {
                                let messages = messages.as_mut().unwrap();
                                view.window(*key, ui).opened(&mut opened).build(|| {
                                    *visible = true;
                                    view.draw(ui, window, (&mut *messages, *key));
                                });
                                if !opened {
                                    messages.push(Message::$im_destroy_message_ident(*key));
                                    return true;
                                } else if was_visible != *visible {
                                    messages.push(
                                        Message::$im_visibility_changed_message_ident(
                                            *key,
                                            *visible,
                                        ),
                                    );
                                }
                                false
                            } else {
                                <$im_view_ty>::window_stopped(*key, ui)
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
            }
        }

        $(
            impl<M: Messages> FrameViewMessages<$s_view_ty> for &mut M {
                fn push(
                    &mut self,
                    message: <<$s_view_ty as FrameView>::EmuState as FrameViewEmuState>::Message,
                ) {
                    Messages::push(self, Message::$s_message_ident(message));
                }
            }
        )*

        $(
            impl<M: Messages> FrameViewMessages<$i_view_ty> for (&mut M, ViewKey) {
                fn push(
                    &mut self,
                    message: <<$i_view_ty as FrameView>::EmuState as FrameViewEmuState>::Message,
                ) {
                    self.0.push(Message::$i_message_ident(self.1, message));
                }
            }
        )*

        $(
            impl<M: Messages> MessageViewMessages<$im_view_ty> for (&mut M, ViewKey) {
                fn push(
                    &mut self,
                    message: <
                        <$im_view_ty as MessageView>::EmuState as MessageViewEmuState
                    >::Message,
                ) {
                    self.0.push(Message::$im_message_ident(self.1, message));
                }
            }
        )*

        $(
            impl<N: Notifications> MessageViewNotifications<
                <$im_view_ty as MessageView>::EmuState
            > for (N, ViewKey) {
                fn push(
                    &mut self,
                    notif: <
                        <$im_view_ty as MessageView>::EmuState as MessageViewEmuState
                    >::Notification,
                ) {
                    self.0.push(Notification::$im_notif_ident(self.1, notif));
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
    [
        (ds_rom_info, DsRomInfo, FetchDsRomInfo, ReplyDsRomInfo)
    ],
    [
        (fs, Fs, InitFs, DestroyFs, FsVisibility, FsMessage, FsNotif)
    ]
);
