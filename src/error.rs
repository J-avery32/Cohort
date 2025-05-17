/// Represents possible errors for FIFO operations.
pub enum Error {
    /// The FIFO is full.
    Full,
    /// The FIFO is empty.
    Empty,
    /// The capacity given to [`new`](crate::fifo::Fifo::new) is not divisible by 2.
    Capacity(usize),
}

impl Error {
    /// Returns a string representation of the error.
    pub fn to_string(&self) -> String {
        match self {
            Error::Full => "fifo is full".to_string(),
            Error::Empty => "fifo is empty".to_string(),
            Error::Capacity(capacity) => {
                format!("fifo capacity {} is not divisible by 2", capacity)
            }
        }
    }
}

impl std::fmt::Debug for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

impl std::error::Error for Error {}
