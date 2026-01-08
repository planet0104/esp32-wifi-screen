//! Type definitions for TJpgDec

/// Result type for JPEG operations
pub type Result<T> = core::result::Result<T, Error>;

/// Error codes for JPEG decompression
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Error {
    /// Operation succeeded
    Ok = 0,
    /// Interrupted by output function
    Interrupted = 1,
    /// Device error or wrong termination of input stream
    Input = 2,
    /// Insufficient memory pool for the image
    InsufficientMemory = 3,
    /// Insufficient stream input buffer
    InsufficientBuffer = 4,
    /// Parameter error
    Parameter = 5,
    /// Data format error (may be broken data)
    FormatError = 6,
    /// Right format but not supported
    UnsupportedFormat = 7,
    /// Not supported JPEG standard
    UnsupportedStandard = 8,
}

impl Error {
    pub fn as_str(&self) -> &'static str {
        match self {
            Error::Ok => "Success",
            Error::Interrupted => "Interrupted by output function",
            Error::Input => "Input stream error",
            Error::InsufficientMemory => "Insufficient memory",
            Error::InsufficientBuffer => "Insufficient buffer",
            Error::Parameter => "Parameter error",
            Error::FormatError => "Format error",
            Error::UnsupportedFormat => "Unsupported format",
            Error::UnsupportedStandard => "Unsupported JPEG standard",
        }
    }
}

#[cfg(feature = "std")]
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

/// Rectangular region in the output image
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rectangle {
    pub left: u16,
    pub right: u16,
    pub top: u16,
    pub bottom: u16,
}

impl Rectangle {
    pub fn new(left: u16, right: u16, top: u16, bottom: u16) -> Self {
        Self { left, right, top, bottom }
    }

    pub fn width(&self) -> u16 {
        self.right.saturating_sub(self.left).saturating_add(1)
    }

    pub fn height(&self) -> u16 {
        self.bottom.saturating_sub(self.top).saturating_add(1)
    }
}

/// Output pixel format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum OutputFormat {
    /// RGB888 (24-bit/pixel)
    Rgb888 = 0,
    /// RGB565 (16-bit/pixel)
    Rgb565 = 1,
    /// Grayscale (8-bit/pixel)
    Grayscale = 2,
}

/// YUV value type - changes based on optimization level
#[cfg(feature = "fast-decode")]
#[allow(dead_code)]
pub type YuvValue = i16;

#[cfg(not(feature = "fast-decode"))]
#[allow(dead_code)]
pub type YuvValue = u8;

/// Sampling factors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplingFactor {
    /// 4:4:4 (1x1)
    Yuv444,
    /// 4:2:2 (2x1)
    Yuv422,
    /// 4:2:0 (2x2)
    Yuv420,
}

impl SamplingFactor {
    pub fn from_factor(h: u8, v: u8) -> Option<Self> {
        match (h, v) {
            (1, 1) => Some(SamplingFactor::Yuv444),
            (2, 1) => Some(SamplingFactor::Yuv422),
            (2, 2) => Some(SamplingFactor::Yuv420),
            _ => None,
        }
    }

    pub fn mcu_width(&self) -> u8 {
        match self {
            SamplingFactor::Yuv444 => 1,
            SamplingFactor::Yuv422 | SamplingFactor::Yuv420 => 2,
        }
    }

    pub fn mcu_height(&self) -> u8 {
        match self {
            SamplingFactor::Yuv444 | SamplingFactor::Yuv422 => 1,
            SamplingFactor::Yuv420 => 2,
        }
    }
}
