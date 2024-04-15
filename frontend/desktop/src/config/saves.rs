use crate::utils::HomePathBuf;
use serde::{Deserialize, Serialize};
use std::{
    ffi::{OsStr, OsString},
    fs, mem,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LocationKind {
    Global,
    Custom,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CustomLocation {
    pub base_dir: HomePathBuf,
    pub base_name: Option<OsString>,
    pub extension: Option<OsString>,
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Location {
    #[default]
    Global,
    Custom(CustomLocation),
}

impl Location {
    pub fn kind(&self) -> LocationKind {
        match self {
            Location::Global => LocationKind::Global,
            Location::Custom { .. } => LocationKind::Custom,
        }
    }

    pub fn default_for_kind(kind: LocationKind, save_dir: &Path, game_title: &str) -> Self {
        match kind {
            LocationKind::Global => Location::Global,
            LocationKind::Custom => Location::Custom(CustomLocation {
                base_dir: HomePathBuf(save_dir.to_path_buf()),
                base_name: Some(OsString::from(game_title)),
                extension: Some(OsString::from("sav")),
            }),
        }
    }

    pub fn path_components<'a>(
        &'a self,
        save_dir: &'a Path,
        game_title: &'a str,
    ) -> (&'a Path, Option<&'a OsStr>, Option<&'a OsStr>) {
        match self {
            Location::Global => (save_dir, Some(game_title.as_ref()), Some("sav".as_ref())),
            Location::Custom(CustomLocation {
                base_dir,
                base_name,
                extension,
            }) => (
                base_dir.0.as_path(),
                base_name.as_deref(),
                extension.as_deref(),
            ),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SlotsKind {
    Single,
    Multiple,
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Slots {
    #[default]
    Single,
    Multiple {
        current: Option<usize>,
        slots: Vec<String>,
    },
}

impl Slots {
    pub fn kind(&self) -> SlotsKind {
        match self {
            Slots::Single => SlotsKind::Single,
            Slots::Multiple { .. } => SlotsKind::Multiple,
        }
    }

    pub fn default_for_kind(kind: SlotsKind) -> Self {
        match kind {
            SlotsKind::Single => Slots::Single,
            SlotsKind::Multiple => Slots::Multiple {
                current: Some(0),
                slots: vec!["0".to_owned()],
            },
        }
    }
}

#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PathConfig {
    pub location: Location,
    pub slots: Slots,
}

fn path(
    base_dir: &Path,
    base_name: Option<&OsStr>,
    extension: Option<&OsStr>,
    slot_name: Option<&str>,
) -> PathBuf {
    let mut path = if let Some(base_name) = base_name {
        let mut path = base_dir.join(base_name).into_os_string();
        if let Some(slot_name) = slot_name {
            path.push(".");
            path.push(slot_name);
        }
        path
    } else {
        base_dir.join(slot_name.unwrap()).into_os_string()
    };
    if let Some(extension) = extension {
        path.push(".");
        path.push(extension);
    }
    PathBuf::from(path)
}

impl PathConfig {
    pub fn path(&self, save_dir: &Path, game_title: &str) -> Option<PathBuf> {
        let (base_dir, base_name, extension) = self.location.path_components(save_dir, game_title);

        let slot_name = match &self.slots {
            Slots::Single => None,
            Slots::Multiple { current, slots } => Some(slots[(*current)?].as_str()),
        };

        Some(path(base_dir, base_name, extension, slot_name))
    }

    pub fn change_slots(&mut self, new_slots_kind: SlotsKind, save_dir: &Path, game_title: &str) {
        if self.slots.kind() == new_slots_kind {
            return;
        }

        let prev_slots = mem::replace(&mut self.slots, Slots::default_for_kind(new_slots_kind));

        if let Slots::Multiple {
            current: current_i,
            slots,
        } = prev_slots
        {
            let (base_dir, base_name, extension) =
                self.location.path_components(save_dir, game_title);

            for (i, slot_name) in slots.into_iter().enumerate() {
                if Some(i) != current_i {
                    let _ = fs::remove_file(path(base_dir, base_name, extension, Some(&slot_name)));
                }
            }
        }
    }

    pub fn change_location(
        &mut self,
        new_location_kind: LocationKind,
        save_dir: &Path,
        game_title: &str,
    ) {
        if self.location.kind() == new_location_kind {
            return;
        }

        let prev_location = mem::replace(
            &mut self.location,
            Location::default_for_kind(new_location_kind, save_dir, game_title),
        );

        if let Location::Custom(CustomLocation {
            base_dir,
            base_name,
            extension,
        }) = prev_location
        {
            if let Slots::Multiple {
                current: current_i,
                slots,
            } = &self.slots
            {
                let (new_base_dir, new_base_name, new_extension) =
                    self.location.path_components(save_dir, game_title);
                for (i, slot_name) in slots.iter().enumerate() {
                    if Some(i) != *current_i {
                        let _ = fs::rename(
                            path(
                                &base_dir.0,
                                base_name.as_deref(),
                                extension.as_deref(),
                                Some(slot_name),
                            ),
                            path(new_base_dir, new_base_name, new_extension, Some(slot_name)),
                        );
                    }
                }
            }
        }
    }

    pub fn make_multi_slot(&mut self) {
        if let Slots::Single = &self.slots {
            self.slots = Slots::Multiple {
                current: Some(0),
                slots: vec!["0".to_owned()],
            };
        }
    }

    pub fn switch_slot(&mut self, i: usize) -> bool {
        if let Slots::Multiple { current, .. } = &mut self.slots {
            if Some(i) == *current {
                return false;
            }
            *current = Some(i);
            return true;
        }
        false
    }

    pub fn create_slot(&mut self, name: String) {
        if let Slots::Multiple { slots, .. } = &mut self.slots {
            if !slots.contains(&name) {
                slots.push(name);
            }
        }
    }

    pub fn rename_slot(&mut self, i: usize, new_name: String, save_dir: &Path, game_title: &str) {
        if let Slots::Multiple { current, slots } = &mut self.slots {
            if slots.contains(&new_name) {
                return;
            }

            let (base_dir, base_name, extension) =
                self.location.path_components(save_dir, game_title);
            let prev_name = mem::replace(&mut slots[i], new_name.to_owned());
            if *current != Some(i) {
                let _ = fs::rename(
                    path(base_dir, base_name, extension, Some(&prev_name)),
                    path(base_dir, base_name, extension, Some(&new_name)),
                );
            }
        }
    }

    pub fn remove_slot(&mut self, i: usize, save_dir: &Path, game_title: &str) {
        if let Slots::Multiple { current, slots } = &mut self.slots {
            let name = slots.remove(i);
            if let Some(current_) = current {
                if *current_ == i {
                    *current = None;
                    return;
                }

                let (base_dir, base_name, extension) =
                    self.location.path_components(save_dir, game_title);
                let _ = fs::remove_file(path(base_dir, base_name, extension, Some(&name)));

                if *current_ > i {
                    *current_ -= 1;
                }
            }
        }
    }
}
