use super::{
    common::format_size, BaseView, InstanceableView, MessageView, MessageViewEmuState,
    MessageViewMessages, MessageViewNotifications,
};
use crate::ui::window::Window;
use dust_core::{cpu, ds_slot::rom::header::Header, emu::Emu, utils::mem_prelude::*};
use imgui::{TableColumnFlags, TableColumnSetup, TableFlags, TreeNodeFlags, TreeNodeId};
use imgui_memory_editor::MemoryEditor;
use rfd::FileDialog;
use std::{fmt::Write, fs};

pub enum Message {
    ReadFile { id: u16, start: u32, size: u32 },
}

pub enum Notification {
    NoRom,
    FntFat((BoxedByteSlice, BoxedByteSlice)),
    File { id: u16, contents: BoxedByteSlice },
}

pub struct EmuState;

impl super::MessageViewEmuState for EmuState {
    type InitData = ();
    type Message = Message;
    type Notification = Notification;

    fn new<E: cpu::Engine, N: MessageViewNotifications<Self>>(
        _data: Self::InitData,
        _visible: bool,
        emu: &mut Emu<E>,
        mut notifs: N,
    ) -> Self {
        let Some(contents) = emu.ds_slot.rom.contents() else {
            notifs.push(Notification::NoRom);
            return EmuState;
        };

        let mut header_bytes = Bytes::new([0; 0x170]);
        contents.read_header(&mut header_bytes);
        let header = Header::new(&header_bytes);

        let mut fnt = BoxedByteSlice::new_zeroed(header.fnt_size() as usize);
        contents.read_slice_wrapping(header.fnt_offset(), &mut fnt);

        let mut fat = BoxedByteSlice::new_zeroed(header.fat_size() as usize);
        contents.read_slice_wrapping(header.fat_offset(), &mut fat);

        notifs.push(Notification::FntFat((fnt, fat)));

        EmuState
    }

    fn handle_message<E: cpu::Engine, N: MessageViewNotifications<Self>>(
        &mut self,
        message: Self::Message,
        emu: &mut Emu<E>,
        mut notifs: N,
    ) {
        match message {
            Message::ReadFile { id, start, size } => {
                let mut contents = BoxedByteSlice::new_zeroed(size as usize);
                emu.ds_slot
                    .rom
                    .contents()
                    .expect("DS slot ROM contents should have been present")
                    .read_slice_wrapping(start, &mut contents);
                notifs.push(Notification::File { id, contents });
            }
        }
    }
}

#[derive(Debug, Clone)]
struct File {
    start: u32,
    size: u32,
}

#[derive(Debug)]
struct Directory {
    entries: Vec<DirEntry>,
}

#[derive(Debug)]
enum DirEntryContents {
    File(File),
    Directory(Directory),
}

#[derive(Debug)]
struct DirEntry {
    name: String,
    id: u16,
    contents: DirEntryContents,
}

#[derive(Debug)]
struct FileSelection {
    name: String,
    path: String,
    id: u16,
    file: File,
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct DirEntryComponents: u8 {
        const FILE_SIZE = 1 << 0;
        const DIR_ENTRIES = 1 << 1;
        const ID = 1 << 2;
    }
}

pub struct Fs {
    has_rom: bool,
    dir_entry_components: DirEntryComponents,
    root_dir: Option<Option<Vec<DirEntry>>>,
    selected_file: Option<(FileSelection, Option<Option<BoxedByteSlice>>)>,
    editor: MemoryEditor,
}

impl Fs {
    fn read_directory_tree(&mut self, fnt_fat: (BoxedByteSlice, BoxedByteSlice)) {
        fn read_dir_entry(
            mut addr: usize,
            file_id: &mut u16,
            parent_dir_id: u16,
            fnt_fat: &(BoxedByteSlice, BoxedByteSlice),
        ) -> Option<(usize, DirEntry)> {
            let (fnt, fat) = fnt_fat;

            let type_length = *fnt.get(addr)?;
            addr += 1;
            let is_directory = type_length & 0x80 != 0;
            let name_len = type_length as usize & 0x7F;
            if name_len == 0 {
                return None;
            }

            let name_bytes = fnt.get(addr..addr + name_len)?.to_vec();
            addr += name_len;
            if name_bytes
                .iter()
                .any(|&b| (!b.is_ascii_graphic() && b != b' ') || b"\\/?\"<>*:;|".contains(&b))
            {
                return None;
            }
            let name = unsafe { String::from_utf8_unchecked(name_bytes) };

            if is_directory {
                let id_bytes = fnt.get(addr..addr + 2)?;
                addr += 2;
                let id = id_bytes.read_le::<u16>(0);
                if id < 0xF000 {
                    return None;
                }
                Some((
                    addr,
                    DirEntry {
                        name,
                        id,
                        contents: DirEntryContents::Directory(Directory {
                            entries: read_dir_entries(id, Some(parent_dir_id), fnt_fat)?,
                        }),
                    },
                ))
            } else {
                let id = *file_id;
                if id >= 0xF000 {
                    return None;
                }
                *file_id += 1;

                let fat_entry_base = ((id & 0xFFF) << 3) as usize;
                let fat_entry = fat.get(fat_entry_base..fat_entry_base + 8)?;
                let start = fat_entry.read_le::<u32>(0);
                let size = fat_entry.read_le::<u32>(4).wrapping_sub(start);

                Some((
                    addr,
                    DirEntry {
                        name,
                        id,
                        contents: DirEntryContents::File(File { start, size }),
                    },
                ))
            }
        }

        fn read_dir_entries(
            id: u16,
            parent_id: Option<u16>,
            fnt_fat: &(BoxedByteSlice, BoxedByteSlice),
        ) -> Option<Vec<DirEntry>> {
            let fnt = &fnt_fat.0;

            let entry_base = ((id & 0xFFF) << 3) as usize;
            let entry = fnt.get(entry_base..entry_base + 8)?;

            let sub_table_offset: u32 = entry.read_le(0);
            if sub_table_offset as usize >= fnt.len() {
                return None;
            }

            let mut file_id: u16 = entry.read_le(4);
            if file_id >= 0xF000 {
                return None;
            }

            let parent_id_: u16 = entry.read_le(6);
            if matches!(parent_id, Some(parent_id) if parent_id != parent_id_) {
                return None;
            }

            let mut addr = sub_table_offset as usize;
            let mut entries = Vec::new();
            while let Some((new_addr, entry)) = read_dir_entry(addr, &mut file_id, id, fnt_fat) {
                entries.push(entry);
                addr = new_addr;
            }
            Some(entries)
        }

        self.root_dir = Some(read_dir_entries(0xF000, None, &fnt_fat));
    }
}

impl BaseView for Fs {
    const MENU_NAME: &'static str = "Filesystem";
}

impl MessageView for Fs {
    type EmuState = EmuState;

    fn new(_window: &mut Window) -> Self {
        Fs {
            has_rom: true,
            dir_entry_components: DirEntryComponents::FILE_SIZE | DirEntryComponents::DIR_ENTRIES,
            root_dir: None,
            selected_file: None,
            editor: MemoryEditor::new(),
        }
    }

    fn destroy(self, _window: &mut Window) {}

    fn emu_state(&self) -> <Self::EmuState as super::MessageViewEmuState>::InitData {}

    fn handle_notif(
        &mut self,
        notif: <Self::EmuState as MessageViewEmuState>::Notification,
        _window: &mut Window,
    ) {
        match notif {
            Notification::NoRom => self.has_rom = false,

            Notification::FntFat(fnt_fat) => {
                self.read_directory_tree(fnt_fat);
            }

            Notification::File { id, contents } => {
                if let Some((selection, contents_ @ None)) = &mut self.selected_file {
                    if selection.id == id {
                        if contents.is_empty() {
                            *contents_ = Some(None);
                        } else {
                            self.editor.set_selected_addr(0, true);
                            self.editor
                                .set_addr_range((0_u64, contents.len() as u64 - 1).into());
                            *contents_ = Some(Some(contents));
                        }
                    }
                }
            }
        }
    }

    fn draw(
        &mut self,
        ui: &imgui::Ui,
        window: &mut Window,
        mut messages: impl MessageViewMessages<Self>,
    ) {
        macro_rules! dir_entry_components {
            (
                $ui: ident,
                $dir_entry_components: ident,
                |$text: ident| (
                    [$(($m_cond_flags: expr, $m_component: expr)),*],
                    [$(($o_cond_flags: expr, $o_component: expr)),*]
                )
            ) => {
                #[allow(unused_assignments)]
                if $dir_entry_components.intersects($($m_cond_flags)|* | $($o_cond_flags)|*) {
                    let mut $text = String::new();

                    let mut has_main = false;
                    $(if $dir_entry_components.contains($m_cond_flags) {
                        if has_main {
                            $text.push_str(", ");
                        }
                        has_main = true;
                        let _ = $m_component;
                    })*

                    let mut has_other = false;
                    $(if $dir_entry_components.contains($o_cond_flags) {
                        if has_other {
                            $text.push_str(", ");
                        } else {
                            if has_main {
                                $text.push(' ');
                            }
                            $text.push('(');
                        }
                        has_other = true;
                        let _ = $o_component;
                    })*
                    if has_other {
                        $text.push(')');
                    }

                    $ui.same_line();
                    $ui.text_disabled($text);
                }
            }
        }

        fn draw_dir_entry(
            name: Option<&str>,
            id: u16,
            entries: &[DirEntry],
            selected_id: Option<u16>,
            dir_entry_components: DirEntryComponents,
            ui: &imgui::Ui,
        ) -> Option<FileSelection> {
            let mut flags =
                TreeNodeFlags::NAV_LEFT_JUMPS_BACK_HERE | TreeNodeFlags::SPAN_AVAIL_WIDTH;
            if entries.is_empty() {
                flags |= TreeNodeFlags::LEAF;
            }
            let tree_node = ui
                .tree_node_config(TreeNodeId::<&str>::Ptr(id as *const _))
                .label::<TreeNodeId<&str>, &str>(name.unwrap_or("/"))
                .flags(flags)
                .push();

            dir_entry_components!(ui, dir_entry_components, |text| (
                [(
                    DirEntryComponents::DIR_ENTRIES,
                    write!(
                        text,
                        "{} {}",
                        entries.len(),
                        if entries.len() == 1 {
                            "entry"
                        } else {
                            "entries"
                        }
                    )
                )],
                [(DirEntryComponents::ID, write!(text, "ID {id:X}"))]
            ));

            if tree_node.is_some() {
                draw_dir_entries(entries, selected_id, dir_entry_components, ui).map(|selection| {
                    let mut path = name
                        .map(|name| {
                            let mut path = String::from(name);
                            path.push('/');
                            path
                        })
                        .unwrap_or_else(|| "/".into());
                    path.push_str(&selection.path);
                    FileSelection { path, ..selection }
                })
            } else {
                None
            }
        }

        fn draw_dir_entries(
            entries: &[DirEntry],
            selected_id: Option<u16>,
            dir_entry_components: DirEntryComponents,
            ui: &imgui::Ui,
        ) -> Option<FileSelection> {
            let mut selection = None;

            for entry in entries {
                match &entry.contents {
                    DirEntryContents::File(file) => {
                        let mut flags = TreeNodeFlags::NAV_LEFT_JUMPS_BACK_HERE
                            | TreeNodeFlags::SPAN_AVAIL_WIDTH
                            | TreeNodeFlags::LEAF;
                        if selected_id == Some(entry.id) {
                            flags |= TreeNodeFlags::SELECTED;
                        }
                        ui.tree_node_config(TreeNodeId::<&str>::Ptr(
                            entry as *const DirEntry as *const _,
                        ))
                        .label::<TreeNodeId<&str>, &str>(&entry.name)
                        .flags(flags)
                        .push();
                        if ui.is_item_clicked() {
                            selection = Some(FileSelection {
                                name: entry.name.clone(),
                                path: entry.name.clone(),
                                id: entry.id,
                                file: file.clone(),
                            });
                        }

                        dir_entry_components!(ui, dir_entry_components, |text| (
                            [(
                                DirEntryComponents::FILE_SIZE,
                                write!(text, "{}", format_size(file.size))
                            )],
                            [(DirEntryComponents::ID, write!(text, "ID {:X}", entry.id))]
                        ));
                    }
                    DirEntryContents::Directory(dir) => {
                        if let Some(new_selection) = draw_dir_entry(
                            Some(&entry.name),
                            entry.id,
                            &dir.entries,
                            selected_id,
                            dir_entry_components,
                            ui,
                        ) {
                            selection = Some(new_selection);
                        }
                    }
                }
            }
            selection
        }

        if !self.has_rom {
            return ui.text("No ROM provided.");
        }
        let root_dir = match &self.root_dir {
            Some(Some(root_dir)) => root_dir,
            Some(None) => return ui.text("Error reading directory tree."),
            None => return ui.text("Loading..."),
        };

        if let Some(_table) = ui.begin_table_with_sizing(
            "",
            2,
            TableFlags::NO_CLIP | TableFlags::BORDERS_INNER_V | TableFlags::RESIZABLE,
            ui.content_region_avail(),
            0.0,
        ) {
            ui.table_setup_column_with(TableColumnSetup {
                flags: TableColumnFlags::WIDTH_FIXED,
                ..TableColumnSetup::new("Tree")
            });
            ui.table_setup_column("File");

            ui.table_next_row();
            ui.table_next_column();

            if ui.button("Options...") {
                ui.open_popup("options");
            }

            ui.popup("options", || {
                let mut dir_entry_components_bits = self.dir_entry_components.bits();
                ui.checkbox_flags(
                    "Show file sizes",
                    &mut dir_entry_components_bits,
                    DirEntryComponents::FILE_SIZE.bits(),
                );
                ui.checkbox_flags(
                    "Show directory entries",
                    &mut dir_entry_components_bits,
                    DirEntryComponents::DIR_ENTRIES.bits(),
                );
                ui.checkbox_flags(
                    "Show IDs",
                    &mut dir_entry_components_bits,
                    DirEntryComponents::ID.bits(),
                );
                self.dir_entry_components =
                    DirEntryComponents::from_bits_truncate(dir_entry_components_bits);
            });

            ui.separator();

            ui.child_window("tree")
                .size([
                    0.0,
                    ui.content_region_avail()[1] - style!(ui, cell_padding)[1],
                ])
                .horizontal_scrollbar(true)
                .build(|| {
                    let prev_selection_id = self.selected_file.as_ref().map(|s| s.0.id);
                    if let Some(selection) = draw_dir_entry(
                        None,
                        0xF000,
                        root_dir,
                        prev_selection_id,
                        self.dir_entry_components,
                        ui,
                    ) {
                        if prev_selection_id != Some(selection.id) {
                            messages.push(Message::ReadFile {
                                id: selection.id,
                                start: selection.file.start,
                                size: selection.file.size,
                            });
                            self.selected_file = Some((selection, None));
                        }
                    }
                });

            ui.table_next_column();

            if let Some((selection, contents)) = &self.selected_file {
                ui.align_text_to_frame_padding();
                ui.text(&selection.path);
                ui.same_line();
                ui.text_disabled(format!(
                    "ID {:X} ({})",
                    selection.id,
                    format_size(selection.file.size)
                ));

                let export_button_width =
                    style!(ui, frame_padding)[0] * 2.0 + ui.calc_text_size("Export")[0];
                ui.same_line_with_pos(ui.content_region_avail()[0] - export_button_width);
                ui.enabled(matches!(&contents, Some(Some(_))), || {
                    if ui.button("Export") {
                        if let Some(path) =
                            FileDialog::new().set_file_name(&selection.name).save_file()
                        {
                            let Some(Some(contents)) = &contents else {
                                unreachable!()
                            };
                            if let Err(err) = fs::write(path, &**contents) {
                                error!(
                                    "Export failed",
                                    "Couldn't write the file's contents: {err}."
                                );
                            }
                        }
                    }
                });

                ui.separator();

                match contents {
                    Some(Some(contents)) => {
                        let _font = ui.push_font(window.imgui.mono_font);
                        self.editor.draw_buffer_read_only(
                            ui,
                            imgui_memory_editor::DisplayMode::Child {
                                height: ui.content_region_avail()[1] - style!(ui, cell_padding)[1],
                            },
                            contents,
                        );
                    }
                    Some(None) => ui.text("Empty file."),
                    None => ui.text("Loading..."),
                }
            } else {
                ui.text("No file opened.");
            }
        }
    }
}

impl InstanceableView for Fs {}
