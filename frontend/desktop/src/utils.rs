use std::{
    borrow::Cow,
    env, fmt,
    path::{Path, PathBuf},
    str,
    sync::{LazyLock, OnceLock},
};

macro_rules! style {
    ($ui: expr, $ident: ident) => {
        unsafe { $ui.style() }.$ident
    };
}

pub fn config_base<'a>() -> &'a Path {
    static CONFIG_BASE: OnceLock<PathBuf> = OnceLock::new();
    CONFIG_BASE.get_or_init(|| match env::var_os("XDG_CONFIG_HOME") {
        Some(config_dir) => Path::new(&config_dir).join("dust"),
        None => HOME
            .as_ref()
            .map(|home| home.join(".config/dust"))
            .unwrap_or_else(|| PathBuf::from("/.config/dust")),
    })
}

pub fn data_base<'a>() -> &'a Path {
    static DATA_BASE: OnceLock<PathBuf> = OnceLock::new();
    DATA_BASE.get_or_init(|| match env::var_os("XDG_DATA_HOME") {
        Some(data_home) => Path::new(&data_home).join("dust"),
        None => HOME
            .as_ref()
            .map(|home| home.join(".local/share/dust"))
            .unwrap_or_else(|| PathBuf::from("/.local/share/dust")),
    })
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

static HOME: LazyLock<Option<PathBuf>> = LazyLock::new(home::home_dir);

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
