use core::{convert::identity, fmt::Display};
use std::error::Error;

#[derive(Debug)]
pub struct CycleFound;

impl Display for CycleFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a cycle has been found in this graph")
    }
}

impl Error for CycleFound {}

#[derive(Debug)]
pub struct EdgeNotFound {
    pub from_port: Option<bool>,
    pub to_port: Option<bool>,
}

impl Display for EdgeNotFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(port) = self.to_port {
            // the graph in the AudioGraph struct from the public api is transposed.
            // Hence, source is destination and vice versa
            f.write_str("source node found, ")?;
            if port {
                f.write_str("source port found, ")?
            } else {
                f.write_str("source port not found, ")?
            }
        } else {
            f.write_str("source node not found, ")?
        }

        if let Some(port) = self.from_port {
            f.write_str("destination node found, ")?;
            if port {
                f.write_str("destination port found")?
            } else {
                f.write_str("destination port not found")?
            }
        } else {
            f.write_str("destination node not found")?
        }

        Ok(())
    }
}

impl Error for EdgeNotFound {}

impl EdgeNotFound {
    pub(super) fn is_not_error(&self) -> bool {
        self.from_port.is_some_and(identity) && self.to_port.is_some_and(identity)
    }
}

#[derive(Debug)]
pub enum EdgeInsertError {
    NotFound(EdgeNotFound),
    CycleFound(CycleFound),
}

impl Display for EdgeInsertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EdgeInsertError::NotFound(e) => e.fmt(f),
            EdgeInsertError::CycleFound(e) => e.fmt(f),
        }
    }
}

impl Error for EdgeInsertError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(match self {
            EdgeInsertError::NotFound(e) => e,
            EdgeInsertError::CycleFound(e) => e,
        })
    }
}
