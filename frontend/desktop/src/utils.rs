use dust_core::{ds_slot::rom::icon_title, utils::zeroed_box};
use std::array;
use std::{
    borrow::Cow,
    fmt,
    path::{Path, PathBuf},
    str,
    sync::LazyLock,
};

macro_rules! style {
    ($ui: expr, $ident: ident) => {
        unsafe { $ui.style() }.$ident
    };
}

macro_rules! format_list {
    ($list: expr) => {
        $list.into_iter().fold(String::new(), |mut acc, v| {
            #[allow(unused_imports)]
            use std::fmt::Write;
            let _ = write!(acc, "\n- {v}");
            acc
        })
    };
}

macro_rules! warning {
    (yes_no, $title: expr, $($desc: tt)*) => {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title($title)
            .set_description(&format!($($desc)*))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
        == rfd::MessageDialogResult::Yes
    };
    ($title: expr, $($desc: tt)*) => {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title($title)
            .set_description(&format!($($desc)*))
            .set_buttons(rfd::MessageButtons::Ok)
            .show()
    };
}

macro_rules! error {
    (yes_no, $title: expr, $($desc: tt)*) => {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title($title)
            .set_description(&format!($($desc)*))
            .set_buttons(rfd::MessageButtons::YesNo)
            .show()
        == rfd::MessageDialogResult::Yes
    };
    ($title: expr, $($desc: tt)*) => {
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title($title)
            .set_description(&format!($($desc)*))
            .set_buttons(rfd::MessageButtons::Ok)
            .show()
    };
}

pub struct BaseDirs {
    pub config: PathBuf,
    pub data: PathBuf,
}

static BASE_DIRS: LazyLock<BaseDirs> = LazyLock::new(|| {
    if let Some(base_dirs) = directories::BaseDirs::new() {
        BaseDirs {
            config: base_dirs.config_dir().join("dust"),
            data: base_dirs.data_local_dir().join("dust").to_path_buf(),
        }
    } else {
        BaseDirs {
            config: Path::new("/.config/dust").to_path_buf(),
            data: Path::new("/.local/share/dust").to_path_buf(),
        }
    }
});

pub fn base_dirs<'a>() -> &'a BaseDirs {
    &BASE_DIRS
}

pub struct Lazy<T> {
    value: Option<T>,
}

impl<T> Lazy<T> {
    pub fn new() -> Self {
        Lazy { value: None }
    }

    pub fn get(&mut self, f: impl FnOnce() -> T) -> &T {
        self.value.get_or_insert_with(f)
    }

    pub fn invalidate(&mut self) {
        self.value = None;
    }
}

static HOME: LazyLock<Option<PathBuf>> =
    LazyLock::new(|| Some(directories::BaseDirs::new()?.home_dir().to_path_buf()));

struct HomePathBufVisitor;

impl<'de> serde::de::Visitor<'de> for HomePathBufVisitor {
    type Value = HomePathBuf;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("path string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(From::from(v))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(From::from(v))
    }

    fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        str::from_utf8(v)
            .map(From::from)
            .map_err(|_| E::invalid_value(serde::de::Unexpected::Bytes(v), &self))
    }

    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        String::from_utf8(v)
            .map(From::from)
            .map_err(|e| E::invalid_value(serde::de::Unexpected::Bytes(&e.into_bytes()), &self))
    }
}

impl<'de> serde::Deserialize<'de> for HomePathBuf {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_string(HomePathBufVisitor)
    }
}

impl serde::Serialize for HomePathBuf {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self.to_string() {
            Some(s) => s.serialize(serializer),
            None => Err(serde::ser::Error::custom(
                "path contains invalid UTF-8 characters",
            )),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HomePathBuf(pub PathBuf);

impl HomePathBuf {
    pub fn to_string(&self) -> Option<Cow<str>> {
        if let Some(path) = HOME
            .as_ref()
            .and_then(|home_path| self.0.strip_prefix(home_path).ok())
        {
            path.to_str().map(|path| format!("~/{path}").into())
        } else {
            self.0.to_str().map(Into::into)
        }
    }
}

impl From<&str> for HomePathBuf {
    fn from(other: &str) -> Self {
        if let Some((home_path, path)) = HOME.as_ref().zip(other.strip_prefix("~/")) {
            return HomePathBuf(home_path.join(PathBuf::from(path)));
        }
        HomePathBuf(PathBuf::from(other))
    }
}

impl From<String> for HomePathBuf {
    fn from(other: String) -> Self {
        other.as_str().into()
    }
}

pub mod double_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn deserialize<'de, D: Deserializer<'de>, T>(
        deserializer: D,
    ) -> Result<Option<Option<T>>, D::Error>
    where
        T: Deserialize<'de>,
    {
        Deserialize::deserialize(deserializer).map(Some)
    }

    pub fn serialize<S: Serializer, T>(
        values: &Option<Option<T>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        T: Serialize,
    {
        match values {
            None => serializer.serialize_unit(),
            Some(None) => serializer.serialize_none(),
            Some(Some(v)) => serializer.serialize_some(&v),
        }
    }
}

pub fn icon_data_to_rgba8(
    palette: &icon_title::Palette,
    pixels: &icon_title::Pixels,
) -> Box<[u8; 0x1000]> {
    let palette: [u32; 0x10] = array::from_fn(|i| {
        if i == 0 {
            return 0;
        }
        let raw = palette[i] as u32;
        let rgb6 = (raw << 1 & 0x3E) | (raw << 4 & 0x3E00) | (raw << 7 & 0x3E_0000);
        0xFF00_0000 | rgb6 << 2 | (rgb6 >> 4 & 0x03_0303)
    });

    let mut rgba = zeroed_box::<[u8; 0x1000]>();
    for (i, pixel) in pixels.iter().enumerate() {
        for (j, component) in palette[*pixel as usize]
            .to_le_bytes()
            .into_iter()
            .enumerate()
        {
            rgba[i << 2 | j] = component;
        }
    }
    rgba
}
