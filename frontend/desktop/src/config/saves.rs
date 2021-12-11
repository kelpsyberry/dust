use serde::{Deserialize, Serialize};
use std::{
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SavePathConfig {
    GlobalSingle,
    GlobalMultiSlot {
        cur_slot_name: String,
    },
    Single(PathBuf),
    MultiSlot {
        base_dir: PathBuf,
        base_name: Option<OsString>,
        cur_slot_name: String,
    },
}

fn slot_path(base_dir: &Path, base_name: Option<&OsStr>, slot_name: &str) -> PathBuf {
    let mut path = if let Some(base_name) = &base_name {
        let mut path = base_dir.join(&base_name).into_os_string();
        path.push(".");
        path.push(slot_name);
        path
    } else {
        base_dir.join(slot_name).into_os_string()
    };
    path.push(".sav");
    PathBuf::from(path)
}

pub fn save_path(
    save_dir_path: &Path,
    save_path_config: &Option<SavePathConfig>,
    game_title: &str,
) -> Option<PathBuf> {
    match save_path_config {
        None => None,
        Some(SavePathConfig::GlobalSingle) => {
            let mut path = save_dir_path.join(game_title).into_os_string();
            path.push(".sav");
            Some(PathBuf::from(path))
        }
        Some(SavePathConfig::GlobalMultiSlot { cur_slot_name }) => Some(slot_path(
            save_dir_path,
            Some(game_title.as_ref()),
            cur_slot_name,
        )),
        Some(SavePathConfig::Single(path)) => Some(path.clone()),
        Some(SavePathConfig::MultiSlot {
            base_dir,
            base_name,
            cur_slot_name,
        }) => Some(slot_path(base_dir, base_name.as_deref(), cur_slot_name)),
    }
}

pub fn make_multi_slot(
    prev_path: &Path,
    base_dir: &Path,
    base_name: Option<&OsStr>,
    new_slot_name: &str,
) -> io::Result<PathBuf> {
    let new_primary_save_path = slot_path(base_dir, base_name, "0");
    fs::rename(prev_path, &new_primary_save_path)?;
    Ok(slot_path(base_dir, base_name, new_slot_name))
}

pub fn change_save_slot(
    save_dir_path: &Path,
    save_path_config: &mut Option<SavePathConfig>,
    game_title: &str,
    new_slot_name: &str,
) -> io::Result<PathBuf> {
    match save_path_config {
        None => unreachable!(),
        Some(SavePathConfig::GlobalSingle) => {
            let base_dir = save_dir_path.to_owned();
            let mut prev_path = base_dir.join(game_title).into_os_string();
            prev_path.push(".sav");
            let base_name = OsString::from(game_title);
            let new_path = make_multi_slot(
                &PathBuf::from(prev_path),
                &base_dir,
                Some(&base_name),
                new_slot_name,
            )?;
            *save_path_config = Some(SavePathConfig::MultiSlot {
                base_dir,
                base_name: Some(base_name),
                cur_slot_name: new_slot_name.to_string(),
            });
            Ok(new_path)
        }
        Some(SavePathConfig::GlobalMultiSlot { cur_slot_name }) => {
            *cur_slot_name = new_slot_name.to_string();
            Ok(slot_path(
                save_dir_path,
                Some(game_title.as_ref()),
                new_slot_name,
            ))
        }
        Some(SavePathConfig::Single(prev_path)) => {
            let base_dir = prev_path
                .parent()
                .unwrap_or_else(|| Path::new("/"))
                .to_path_buf();
            let base_name = prev_path.file_stem().map(OsStr::to_os_string);
            let new_path =
                make_multi_slot(prev_path, &base_dir, base_name.as_deref(), new_slot_name)?;
            *save_path_config = Some(SavePathConfig::MultiSlot {
                base_dir,
                base_name,
                cur_slot_name: new_slot_name.to_string(),
            });
            Ok(new_path)
        }
        Some(SavePathConfig::MultiSlot {
            base_dir,
            base_name,
            cur_slot_name,
        }) => {
            *cur_slot_name = new_slot_name.to_string();
            Ok(slot_path(base_dir, base_name.as_deref(), new_slot_name))
        }
    }
}
