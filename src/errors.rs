use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub enum ServiceError {
    UnknownTag(String),
}

impl Display for ServiceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceError::UnknownTag(text) => {
                write!(f, "{}", text)
            }
        }
    }
}