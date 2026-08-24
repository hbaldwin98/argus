use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! id_type {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
        )]
        pub struct $name(pub u64);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

id_type!(WorkspaceId);
id_type!(ProjectId);
id_type!(CheckoutId);
id_type!(PaneId);

#[derive(Debug, Default)]
pub struct IdGen {
    next: u64,
}

impl IdGen {
    pub fn alloc(&mut self) -> u64 {
        self.next += 1;
        self.next
    }
}
