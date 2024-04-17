use super::{
    common::{format_size, format_size_u64},
    BaseView, InstanceableView, MessageView, MessageViewEmuState, MessageViewMessages,
    MessageViewNotifications,
};
use crate::{emu::ds_slot_rom::ArcDsSlotRom, ui::window::Window};
use dust_core::{
    cpu,
    ds_slot::rom::header::Header,
    emu::Emu,
    utils::{mem_prelude::*, zeroed_box},
};
use imgui::{
    MouseButton, StyleVar, TableColumnFlags, TableColumnSetup, TableFlags, TreeNodeFlags,
    TreeNodeId,
};
use imgui_memory_editor::{MemoryEditor, RangeInclusive};
use rfd::FileDialog;
use std::{
    fmt::Write as _,
    fs,
    io::{self, Write as _},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub struct FileRangeContents {
    range: RangeInclusive<u32>,
    data: BoxedByteSlice,
}

pub enum Message {
    ReadFileRange {
        id: u16,
        start: u32,
        range: RangeInclusive<u32>,
    },
    Export {
        files: Vec<(PathBuf, u32, u32)>,
        exported_size: Arc<AtomicU64>,
    },
}

pub enum Notification {
    NoRom,
    FntFat((BoxedByteSlice, BoxedByteSlice)),
    FileRangeContents {
        id: u16,
        contents: FileRangeContents,
    },
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
            Message::ReadFileRange { id, start, range } => {
                let size = (range.end - range.start) as usize + 1;
                let mut contents = BoxedByteSlice::new_zeroed(size);
                emu.ds_slot
                    .rom
                    .contents()
                    .expect("DS slot ROM contents should have been present")
                    .read_slice_wrapping(start.wrapping_add(range.start), &mut contents);
                notifs.push(Notification::FileRangeContents {
                    id,
                    contents: FileRangeContents {
                        range,
                        data: contents,
                    },
                });
            }
            Message::Export {
                files,
                exported_size: exported_size_shared,
            } => {
                let rom = Arc::clone(
                    &emu.ds_slot
                        .rom
                        .contents()
                        .expect("DS slot ROM contents should have been present")
                        .as_any()
                        .downcast_ref::<ArcDsSlotRom>()
                        .expect("unexpected DS slot ROM contents")
                        .0,
                );

                thread::Builder::new()
                    .name("Export".to_owned())
                    .spawn(move || {
                        const EXPORT_CHUNK_SIZE: usize = 0x100000; // 1 MB

                        let mut exported_size = 0;
                        let mut buffer = zeroed_box::<Bytes<EXPORT_CHUNK_SIZE>>();

                        if let Err((err, dst_path)) =
                            files.into_iter().try_for_each(|(dst_path, start, size)| {
                                (|| -> io::Result<()> {
                                    use dust_core::ds_slot::rom::Contents;

                                    if let Some(parent) = dst_path.parent() {
                                        fs::create_dir_all(parent)?;
                                    }
                                    let mut file = fs::File::create(&dst_path)?;

                                    let mut addr = start;
                                    let end = start + size;
                                    while addr < end {
                                        let len = buffer.len().min((end - addr) as usize);

                                        let buffer = &mut buffer[..len];
                                        rom.read_slice_wrapping(addr, buffer);
                                        file.write_all(buffer)?;

                                        addr += len as u32;
                                        exported_size += len as u64;
                                        exported_size_shared
                                            .store(exported_size, Ordering::Relaxed);
                                    }
                                    Ok(())
                                })()
                                .map_err(|e| (e, dst_path))
                            })
                        {
                            error!(
                                "Export error",
                                "Couldn't complete export at `{}`: {err}",
                                dst_path.display()
                            );
                        }
                    })
                    .expect("couldn't spawn export thread");
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
    path: String,
    name: String,
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

struct InProgressExport {
    source: String,
    destination: PathBuf,
    exported_size: Arc<AtomicU64>,
    total_size: u64,
    finish_time: Option<Instant>,
}

impl InProgressExport {
    const FADE_OUT_DURATION: Duration = Duration::from_secs(1);
}

struct FileViewState {
    visible_addrs: RangeInclusive<u32>,
    contents: FileRangeContents,
}

pub struct Fs {
    has_rom: bool,
    dir_entry_components: DirEntryComponents,
    root_dir: Option<Option<Vec<DirEntry>>>,
    selected_file: Option<(FileSelection, Option<FileViewState>)>,
    in_progress_exports: Vec<InProgressExport>,
    editor: MemoryEditor,
}

impl Fs {
    fn read_root_dir(
        &mut self,
        fnt_fat: (BoxedByteSlice, BoxedByteSlice),
    ) -> Option<Vec<DirEntry>> {
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
                let size = fat_entry.read_le::<u32>(4).checked_sub(start)?;

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

        read_dir_entries(0xF000, None, &fnt_fat)
    }
}

impl BaseView for Fs {
    const MENU_NAME: &'static str = "Filesystem";
}

impl MessageView for Fs {
    type EmuState = EmuState;

    fn new(_window: &mut Window) -> Self {
        let mut editor = MemoryEditor::new();
        editor.set_read_only(true);
        Fs {
            has_rom: true,
            dir_entry_components: DirEntryComponents::FILE_SIZE | DirEntryComponents::DIR_ENTRIES,
            root_dir: None,
            selected_file: None,
            in_progress_exports: Vec::new(),
            editor,
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
                self.root_dir = Some(self.read_root_dir(fnt_fat));
            }

            Notification::FileRangeContents { id, contents } => {
                if let Some((selection, Some(view_state))) = &mut self.selected_file {
                    if selection.id == id {
                        view_state.contents = contents;
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

        fn export_dir(
            src_path: String,
            entries: &[DirEntry],
            in_progress_exports: &mut Vec<InProgressExport>,
            messages: &mut impl MessageViewMessages<Fs>,
        ) {
            if let Some(dst_path) = FileDialog::new().pick_folder() {
                let exported_size = Arc::new(AtomicU64::new(0));

                fn scan_dir(
                    dst_path: &mut PathBuf,
                    entries: &[DirEntry],
                    files: &mut Vec<(PathBuf, u32, u32)>,
                ) -> u64 {
                    let mut total_size = 0;

                    for entry in entries {
                        dst_path.push(&entry.name);

                        match &entry.contents {
                            DirEntryContents::File(file) => {
                                total_size += file.size as u64;
                                files.push((dst_path.clone(), file.start, file.size));
                            }
                            DirEntryContents::Directory(dir) => {
                                total_size += scan_dir(dst_path, &dir.entries, files);
                            }
                        }

                        dst_path.pop();
                    }

                    total_size
                }

                let mut files = Vec::new();
                let total_size = scan_dir(&mut dst_path.clone(), entries, &mut files);

                in_progress_exports.push(InProgressExport {
                    source: src_path,
                    destination: dst_path.clone(),
                    exported_size: Arc::clone(&exported_size),
                    total_size,
                    finish_time: None,
                });
                messages.push(Message::Export {
                    files,
                    exported_size,
                });
            }
        }

        fn export_file(
            src_path: String,
            name: &str,
            file: File,
            in_progress_exports: &mut Vec<InProgressExport>,
            messages: &mut impl MessageViewMessages<Fs>,
        ) {
            if let Some(dst_path) = FileDialog::new().set_file_name(name).save_file() {
                let exported_size = Arc::new(AtomicU64::new(0));
                in_progress_exports.push(InProgressExport {
                    source: src_path,
                    destination: dst_path.clone(),
                    exported_size: Arc::clone(&exported_size),
                    total_size: file.size as u64,
                    finish_time: None,
                });
                messages.push(Message::Export {
                    files: vec![(dst_path, file.start, file.size)],
                    exported_size,
                });
            }
        }

        #[allow(clippy::too_many_arguments)]
        fn draw_dir_entry(
            path: &mut String,
            name: Option<&str>,
            id: u16,
            entries: &[DirEntry],
            selected_id: Option<u16>,
            dir_entry_components: DirEntryComponents,
            in_progress_exports: &mut Vec<InProgressExport>,
            ui: &imgui::Ui,
            messages: &mut impl MessageViewMessages<Fs>,
        ) -> Option<FileSelection> {
            let mut flags =
                TreeNodeFlags::NAV_LEFT_JUMPS_BACK_HERE | TreeNodeFlags::SPAN_AVAIL_WIDTH;
            if entries.is_empty() {
                flags |= TreeNodeFlags::LEAF;
            }

            let _id = ui.push_id_int(id as i32);

            let tree_node = ui
                .tree_node_config("")
                .label::<TreeNodeId<&str>, &str>(name.unwrap_or("/"))
                .flags(flags)
                .push();

            if ui.is_item_clicked_with_button(MouseButton::Right) {
                ui.open_popup("export");
            }

            ui.popup("export", || {
                if ui.button("Export...") {
                    export_dir(
                        if path.is_empty() {
                            "/".to_owned()
                        } else {
                            path.clone()
                        },
                        entries,
                        in_progress_exports,
                        messages,
                    );
                }
            });

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
                let mut selection = None;

                for entry in entries {
                    let prev_path_len = path.len();

                    path.push('/');
                    path.push_str(&entry.name);

                    match &entry.contents {
                        DirEntryContents::File(file) => {
                            let mut flags = TreeNodeFlags::NAV_LEFT_JUMPS_BACK_HERE
                                | TreeNodeFlags::SPAN_AVAIL_WIDTH
                                | TreeNodeFlags::LEAF;
                            if selected_id == Some(entry.id) {
                                flags |= TreeNodeFlags::SELECTED;
                            }

                            let _id = ui.push_id_ptr(entry);

                            ui.tree_node_config("")
                                .label::<TreeNodeId<&str>, &str>(&entry.name)
                                .flags(flags)
                                .push();

                            if ui.is_item_clicked() {
                                selection = Some(FileSelection {
                                    path: path.clone(),
                                    name: entry.name.clone(),
                                    id: entry.id,
                                    file: file.clone(),
                                });
                            }

                            if ui.is_item_clicked_with_button(MouseButton::Right) {
                                ui.open_popup("export");
                            }

                            ui.popup("export", || {
                                if ui.button("Export...") {
                                    export_file(
                                        path.clone(),
                                        &entry.name,
                                        file.clone(),
                                        in_progress_exports,
                                        messages,
                                    );
                                }
                            });

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
                                path,
                                Some(&entry.name),
                                entry.id,
                                &dir.entries,
                                selected_id,
                                dir_entry_components,
                                in_progress_exports,
                                ui,
                                messages,
                            ) {
                                selection = Some(new_selection);
                            }
                        }
                    }

                    path.truncate(prev_path_len);
                }

                selection
            } else {
                None
            }
        }

        if !self.has_rom {
            return ui.text("No ROM provided.");
        }
        let root_dir = match &self.root_dir {
            Some(Some(root_dir)) => root_dir,
            Some(None) => return ui.text("Error reading directory tree."),
            None => return ui.text("Loading..."),
        };

        if let Some(_table) = ui.begin_table_with_flags(
            "main",
            2,
            TableFlags::NO_CLIP | TableFlags::BORDERS_INNER_V | TableFlags::RESIZABLE,
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
                        &mut String::new(),
                        None,
                        0xF000,
                        root_dir,
                        prev_selection_id,
                        self.dir_entry_components,
                        &mut self.in_progress_exports,
                        ui,
                        &mut messages,
                    ) {
                        if prev_selection_id != Some(selection.id) {
                            if selection.file.size == 0 {
                                self.selected_file = Some((selection, None));
                            } else {
                                self.editor.set_selected_addr(0, true);
                                self.editor
                                    .set_addr_range((0_u64, selection.file.size as u64 - 1).into());
                                self.selected_file = Some((
                                    selection,
                                    Some(FileViewState {
                                        visible_addrs: (0, 0).into(),
                                        contents: FileRangeContents {
                                            range: (0, 0).into(),
                                            data: BoxedByteSlice::new_zeroed(0),
                                        },
                                    }),
                                ));
                            }
                        }
                    }
                });

            ui.table_next_column();

            let item_spacing_y = style!(ui, item_spacing)[1];
            let cell_padding_y = style!(ui, cell_padding)[1];
            let in_progress_exports_height = if self.in_progress_exports.is_empty() {
                0.0
            } else {
                item_spacing_y * (self.in_progress_exports.len() + 1) as f32
                    + ui.frame_height() * self.in_progress_exports.len() as f32
            };

            if let Some((selection, view_state)) = &mut self.selected_file {
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
                ui.enabled(view_state.is_some(), || {
                    if ui.button("Export") {
                        export_file(
                            selection.path.clone(),
                            &selection.name,
                            selection.file.clone(),
                            &mut self.in_progress_exports,
                            &mut messages,
                        );
                    }
                });

                ui.separator();

                match view_state {
                    Some(view_state) => {
                        let _font = ui.push_font(window.imgui.mono_font);
                        self.editor.draw_callbacks(
                            ui,
                            imgui_memory_editor::DisplayMode::Child {
                                height: ui.content_region_avail()[1]
                                    - cell_padding_y
                                    - in_progress_exports_height,
                            },
                            &mut (),
                            |_, addr| {
                                if view_state.contents.range.contains(&(addr as u32)) {
                                    let offset =
                                        (addr as u32 - view_state.contents.range.start) as usize;
                                    if offset < view_state.contents.data.len() << 2 {
                                        Some(view_state.contents.data[offset])
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            },
                            |_, _, _| unreachable!(),
                        );

                        let visible_addrs = self.editor.visible_addrs(1, ui);
                        let visible_addrs =
                            (visible_addrs.start as u32, visible_addrs.end as u32).into();
                        if visible_addrs != view_state.visible_addrs {
                            view_state.visible_addrs = visible_addrs;
                            messages.push(Message::ReadFileRange {
                                id: selection.id,
                                start: selection.file.start,
                                range: visible_addrs,
                            });
                        }
                    }
                    None => ui.text("Empty file."),
                }
            } else {
                ui.text("No file opened.");
            }

            if !self.in_progress_exports.is_empty() {
                ui.set_cursor_pos([
                    ui.cursor_pos()[0],
                    ui.content_region_max()[1] - cell_padding_y - in_progress_exports_height
                        + item_spacing_y,
                ]);

                ui.separator();

                if let Some(_table) = ui.begin_table_with_sizing(
                    "exports",
                    2,
                    TableFlags::NO_CLIP,
                    [0.0, ui.content_region_avail()[1] - cell_padding_y],
                    0.0,
                ) {
                    ui.table_setup_column_with(TableColumnSetup {
                        flags: TableColumnFlags::WIDTH_FIXED,
                        ..TableColumnSetup::new("Description")
                    });
                    ui.table_setup_column("Progress");

                    let now = Instant::now();

                    for export in &mut self.in_progress_exports {
                        let exported_size = export.exported_size.load(Ordering::Relaxed);
                        let fraction = (exported_size as f64 / export.total_size as f64) as f32;

                        if export.finish_time.is_none() && exported_size >= export.total_size {
                            export.finish_time = Some(now);
                        }

                        let _alpha = export.finish_time.map(|finish_time| {
                            ui.push_style_var(StyleVar::Alpha(
                                1.0 - (now - finish_time).as_secs_f32()
                                    / InProgressExport::FADE_OUT_DURATION.as_secs_f32(),
                            ))
                        });

                        ui.table_next_row();
                        ui.table_next_column();
                        ui.text(&format!(
                            "{} \u{f061} {}",
                            export.source,
                            export.destination.display()
                        ));

                        ui.table_next_column();
                        imgui::ProgressBar::new(fraction)
                            .overlay_text(format!(
                                "{} / {} ({:.0}%)",
                                format_size_u64(exported_size),
                                format_size_u64(export.total_size),
                                fraction * 100.0
                            ))
                            .build(ui);
                    }

                    self.in_progress_exports.retain(|export| {
                        export.finish_time.as_ref().map_or(true, |finish_time| {
                            now - *finish_time < InProgressExport::FADE_OUT_DURATION
                        })
                    });
                }
            }
        }
    }
}

impl InstanceableView for Fs {
    fn window<'ui>(
        &mut self,
        key: u32,
        ui: &'ui imgui::Ui,
    ) -> imgui::Window<'ui, 'ui, impl AsRef<str> + 'static> {
        ui.window(format!("{} {key}", Self::MENU_NAME))
            .scrollable(false)
            .scroll_bar(false)
    }
}
