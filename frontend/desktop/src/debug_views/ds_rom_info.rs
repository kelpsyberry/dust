use super::{
    common::{format_size, format_size_shift},
    BaseView, SingletonView, StaticView,
};
use crate::{ui::window::Window, utils::icon_data_to_rgba8};
use dust_core::{
    cpu,
    ds_slot::rom::{
        header::{Header, Region, UnitCode},
        icon_title::{self, IconTitle},
        Rom,
    },
    emu::Emu,
    utils::{zeroed_box, Bytes},
};
use imgui::{
    Image, StyleColor, TableColumnFlags, TableColumnSetup, TableFlags, TextureId, TreeNodeFlags,
};
use std::borrow::Cow;

pub struct TransferData {
    chip_id: u32,
    header_bytes: Box<Bytes<0x170>>,
    icon_title: Option<Box<IconTitle>>,
}

struct Data {
    chip_id: u32,
    header_bytes: Box<Bytes<0x170>>,
    icon_title: Option<(TextureId, Box<IconTitle>)>,
}

pub struct DsRomInfo {
    data: Option<Data>,
}

impl BaseView for DsRomInfo {
    const MENU_NAME: &'static str = "DS ROM info";
}

impl StaticView for DsRomInfo {
    type Data = Option<TransferData>;

    fn fetch_data<E: cpu::Engine>(emu: &mut Emu<E>) -> Self::Data {
        let Rom::Normal(rom) = &emu.ds_slot.rom else {
            return None;
        };

        let mut header_bytes = zeroed_box();
        rom.contents().read_header(&mut header_bytes);

        let icon_title = IconTitle::decode_at_offset(
            Header::new(&header_bytes).icon_title_offset(),
            rom.contents(),
        )
        .ok()
        .map(Box::new);

        Some(TransferData {
            chip_id: rom.chip_id(),
            header_bytes,
            icon_title,
        })
    }

    fn new(data: Self::Data, window: &mut Window) -> Self {
        let data = data.map(|data| {
            let icon_title = data.icon_title.map(|icon_title| {
                let icon_tex = window.imgui_gfx.create_owned_texture(
                    Some("Icon".into()),
                    imgui_wgpu::TextureDescriptor {
                        width: 32,
                        height: 32,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        ..Default::default()
                    },
                    imgui_wgpu::SamplerDescriptor {
                        mag_filter: wgpu::FilterMode::Nearest,
                        min_filter: wgpu::FilterMode::Linear,
                        ..Default::default()
                    },
                );
                icon_tex.set_data(
                    window.gfx_device(),
                    window.gfx_queue(),
                    &*icon_data_to_rgba8(
                        &icon_title.default_icon.palette,
                        &icon_title.default_icon.pixels,
                    ),
                    Default::default(),
                );
                (
                    window
                        .imgui_gfx
                        .add_texture(imgui_wgpu::Texture::Owned(icon_tex)),
                    icon_title,
                )
            });
            Data {
                chip_id: data.chip_id,
                header_bytes: data.header_bytes,
                icon_title,
            }
        });
        DsRomInfo { data }
    }

    fn draw(&mut self, ui: &imgui::Ui, _window: &mut Window) {
        let Some(data) = &self.data else {
            return ui.text("No ROM provided.");
        };

        macro_rules! data {
            ($name: expr, $value: expr) => {
                ui.table_next_row();
                ui.table_next_column();
                ui.text(concat!($name, ":"));
                ui.table_next_column();
                ui.text_wrapped($value);
            };
        }

        let header = Header::new(&data.header_bytes);

        if let Some((icon_tex_id, icon_title)) = &data.icon_title {
            let mut cursor_pos = ui.cursor_pos();
            cursor_pos[0] += (ui.content_region_avail()[0] - 128.0) * 0.5;
            ui.set_cursor_pos(cursor_pos);
            Image::new(*icon_tex_id, [128.0; 2])
                .border_col(ui.style_color(StyleColor::Border))
                .build(ui);

            if ui.collapsing_header(
                "Titles",
                TreeNodeFlags::DEFAULT_OPEN | TreeNodeFlags::NO_TREE_PUSH_ON_OPEN,
            ) {
                if let Some(_table) =
                    ui.begin_table_with_flags("icon-title", 2, TableFlags::NO_CLIP)
                {
                    macro_rules! title {
                        ($name: expr, $value: expr) => {
                            data!($name, $value.as_deref().unwrap_or("<invalid UTF-16>"));
                        };
                    }

                    ui.table_setup_column_with(TableColumnSetup {
                        flags: TableColumnFlags::WIDTH_FIXED,
                        ..TableColumnSetup::new("Name")
                    });
                    ui.table_setup_column("Value");

                    title!("Japanese", icon_title.titles.japanese);
                    title!("English", icon_title.titles.english);
                    title!("French", icon_title.titles.french);
                    title!("German", icon_title.titles.german);
                    title!("Italian", icon_title.titles.italian);
                    title!("Spanish", icon_title.titles.spanish);
                    if let Some(chinese) = &icon_title.titles.chinese {
                        title!("Chinese", chinese);
                    }
                    if let Some(korean) = &icon_title.titles.korean {
                        title!("Korean", korean);
                    }
                }
            }
        }

        if ui.collapsing_header(
            "General",
            TreeNodeFlags::DEFAULT_OPEN | TreeNodeFlags::NO_TREE_PUSH_ON_OPEN,
        ) {
            if let Some(_table) = ui.begin_table_with_flags("other", 2, TableFlags::NO_CLIP) {
                ui.table_setup_column_with(TableColumnSetup {
                    flags: TableColumnFlags::WIDTH_FIXED,
                    ..TableColumnSetup::new("Name")
                });
                ui.table_setup_column("Value");

                data!(
                    "Game title",
                    header
                        .game_title()
                        .map(|s| Cow::from(format!("{s:?}")))
                        .unwrap_or("<invalid UTF-8>".into())
                );
                data!("Game code", {
                    let (code, str) = header.game_code();
                    if code == 0 {
                        Cow::from("Homebrew (0)")
                    } else if let Some(str) = str {
                        format!("{str:?} ({code:#010X})").into()
                    } else {
                        format!("{code:#010X}").into()
                    }
                });
                data!("Maker code", {
                    let (code, str) = header.maker_code();
                    if code == 0 {
                        Cow::from("Homebrew (0)")
                    } else if let Some(str) = str {
                        format!("{str:?} ({code:#06X})").into()
                    } else {
                        format!("{code:#06X}").into()
                    }
                });
                data!(
                    "Unit code",
                    match header.unit_code() {
                        Ok(UnitCode::Ds) => Cow::from("DS"),
                        Ok(UnitCode::DsAndDsi) => "DS and DSi".into(),
                        Ok(UnitCode::Dsi) => "DSi".into(),
                        Err(code) => format!("Unknown ({code:#04X})").into(),
                    }
                );
                data!(
                    "ROM size",
                    format!(
                        "{} (capacity {})",
                        format_size(header.used_rom_size()),
                        format_size_shift(header.capacity().0 as usize + 17)
                    )
                );
                data!(
                    "Region",
                    match header.region() {
                        Ok(Region::Normal) => Cow::from("Normal"),
                        Ok(Region::Korea) => "Korea".into(),
                        Ok(Region::China) => "China".into(),
                        Err(code) => format!("Unknown ({code:#04X})").into(),
                    }
                );
                data!("Version", format!("{:#04X}", header.version()));
                data!("Auto-start", if header.auto_start() { "On" } else { "Off" });
            }
        }

        if ui.collapsing_header("Advanced", TreeNodeFlags::NO_TREE_PUSH_ON_OPEN) {
            if let Some(_table) =
                ui.begin_table_with_flags("other-advanced", 2, TableFlags::NO_CLIP)
            {
                ui.table_setup_column_with(TableColumnSetup {
                    flags: TableColumnFlags::WIDTH_FIXED,
                    ..TableColumnSetup::new("Name")
                });
                ui.table_setup_column("Value");

                data!("Chip ID", format!("{:#010X}", data.chip_id));
                data!(
                    "Encryption seed",
                    match header.encryption_seed() {
                        Ok(seed) => format!("{}", seed.get()),
                        Err(seed) => format!("Unknown ({seed:#04X})"),
                    }
                );
                data!("Icon/title", {
                    if let Some(icon_title) = &data.icon_title {
                        let size = match icon_title.1.version_crc_data.version {
                            icon_title::Version::Base | icon_title::Version::Chinese => 0xA00,
                            icon_title::Version::Korean => 0xC00,
                            icon_title::Version::AnimatedIcon => 0x2400,
                        };
                        format!(
                            "ROM: {:#010X}..{:#010X} ({})",
                            header.icon_title_offset(),
                            header.icon_title_offset() as u64 + size as u64,
                            format_size(size)
                        )
                    } else {
                        format!("ROM offset: {:#010X}", header.icon_title_offset())
                    }
                });
                data!("ARM7 payload", {
                    let size = header.arm7_size();
                    format!(
                        "ROM: {:#010X}..{:#010X}\nRAM: {:#010X}..{:#010X}\n({})",
                        header.arm7_rom_offset(),
                        header.arm7_rom_offset() as u64 + size as u64,
                        header.arm7_ram_addr(),
                        header.arm7_ram_addr() as u64 + size as u64,
                        format_size(size)
                    )
                });
                data!(
                    "ARM7 entry address",
                    format!("{:#010X}", header.arm7_entry_addr())
                );
                data!("ARM9 payload", {
                    let size = header.arm9_size();
                    format!(
                        "ROM: {:#010X}..{:#010X}\nRAM: {:#010X}..{:#010X}\n({})",
                        header.arm9_rom_offset(),
                        header.arm9_rom_offset() as u64 + size as u64,
                        header.arm9_ram_addr(),
                        header.arm9_ram_addr() as u64 + size as u64,
                        format_size(size)
                    )
                });
                data!(
                    "ARM9 entry address",
                    format!("{:#010X}", header.arm9_entry_addr())
                );
                data!("FNT", {
                    let size = header.fnt_size();
                    format!(
                        "ROM: {:#010X}..{:#010X} ({})",
                        header.fnt_offset(),
                        header.fnt_offset() as u64 + size as u64,
                        format_size(size)
                    )
                });
                data!("FAT", {
                    let size = header.fat_size();
                    format!(
                        "ROM: {:#010X}..{:#010X} ({})",
                        header.fat_offset(),
                        header.fat_offset() as u64 + size as u64,
                        format_size(size)
                    )
                });
                data!("ARM7 overlay", {
                    let size = header.arm7_overlay_size();
                    if size == 0 {
                        Cow::from("Not present")
                    } else {
                        format!(
                            "ROM: {:#010X}..{:#010X} ({})",
                            header.arm7_overlay_offset(),
                            header.arm7_overlay_offset() as u64 + size as u64,
                            format_size(size)
                        )
                        .into()
                    }
                });
                data!("ARM9 overlay", {
                    let size = header.arm9_overlay_size();
                    if size == 0 {
                        Cow::from("Not present")
                    } else {
                        format!(
                            "ROM: {:#010X}..{:#010X} ({})",
                            header.arm9_overlay_offset(),
                            header.arm9_overlay_offset() as u64 + size as u64,
                            format_size(size)
                        )
                        .into()
                    }
                });
                data!(
                    "ROMCTRL normal setting",
                    format!("{:#010X}", header.rom_control_normal().0)
                );
                data!(
                    "ROMCTRL key1 setting",
                    format!("{:#010X}", header.rom_control_key1().0)
                );
            }
        }
    }
}

impl SingletonView for DsRomInfo {}
