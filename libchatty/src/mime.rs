use mime::{Name, Params};
use serde::{Serialize, Deserialize};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct Mime(mime::Mime);

impl Mime {
    pub fn type_(&self) -> Name<'_> {
        self.0.type_()
    }

    pub fn subtype(&self) -> Name<'_> {
        self.0.subtype()
    }

    pub fn suffix(&self) -> Option<Name<'_>> {
        self.0.suffix()
    }

    pub fn get_param<'a, N>(&'a self, attr: N) -> Option<Name<'a>>
    where
        N: PartialEq<Name<'a>>
    {
        self.0.get_param(attr)
    }

    pub fn params<'a>(&'a self) -> Params<'a> {
        self.0.params()
    }

    pub fn essence_str(&self) -> &str {
        self.0.essence_str()
    }
}

impl Serialize for Mime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer {
        self.essence_str().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Mime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de> {
        let s: &str = Deserialize::deserialize(deserializer)?;
        let result = mime::Mime::from_str(s).unwrap();

        Ok(Mime(result))
    }
}

impl From<mime::Mime> for Mime {
    fn from(value: mime::Mime) -> Self {
        Self(value)
    }
}

impl FromStr for Mime {
    type Err = mime::FromStrError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mime: mime::Mime = s.parse()?;
        Ok(mime.into())
    }
}
