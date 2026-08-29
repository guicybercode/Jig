use portable_pty::PtySize;

use crate::SessionError;

/// Validated character and pixel dimensions for one pseudo-terminal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TerminalSize {
    /// Visible terminal rows.
    rows: u16,
    /// Visible terminal columns.
    cols: u16,
    /// Optional width in pixels; zero means unspecified.
    pixel_width: u16,
    /// Optional height in pixels; zero means unspecified.
    pixel_height: u16,
}

impl TerminalSize {
    /// Creates dimensions with unspecified pixel measurements.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::InvalidTerminalSize`] when either character
    /// dimension is zero.
    pub fn new(rows: u16, cols: u16) -> Result<Self, SessionError> {
        Self::with_pixels(rows, cols, 0, 0)
    }

    /// Creates dimensions including optional pixel measurements.
    ///
    /// # Errors
    ///
    /// Returns [`SessionError::InvalidTerminalSize`] when either character
    /// dimension is zero.
    pub fn with_pixels(
        rows: u16,
        cols: u16,
        pixel_width: u16,
        pixel_height: u16,
    ) -> Result<Self, SessionError> {
        if rows == 0 || cols == 0 {
            return Err(SessionError::InvalidTerminalSize { rows, cols });
        }
        Ok(Self {
            rows,
            cols,
            pixel_width,
            pixel_height,
        })
    }

    /// Returns the visible terminal row count.
    #[must_use]
    pub const fn rows(self) -> u16 {
        self.rows
    }

    /// Returns the visible terminal column count.
    #[must_use]
    pub const fn cols(self) -> u16 {
        self.cols
    }

    /// Returns the terminal width in pixels, or zero when unspecified.
    #[must_use]
    pub const fn pixel_width(self) -> u16 {
        self.pixel_width
    }

    /// Returns the terminal height in pixels, or zero when unspecified.
    #[must_use]
    pub const fn pixel_height(self) -> u16 {
        self.pixel_height
    }

    pub(crate) fn validate(self) -> Result<(), SessionError> {
        if self.rows == 0 || self.cols == 0 {
            Err(SessionError::InvalidTerminalSize {
                rows: self.rows,
                cols: self.cols,
            })
        } else {
            Ok(())
        }
    }
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        }
    }
}

impl From<TerminalSize> for PtySize {
    fn from(value: TerminalSize) -> Self {
        Self {
            rows: value.rows,
            cols: value.cols,
            pixel_width: value.pixel_width,
            pixel_height: value.pixel_height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dimensions_require_rows_and_columns() {
        assert!(matches!(
            TerminalSize::new(0, 80),
            Err(SessionError::InvalidTerminalSize { rows: 0, cols: 80 })
        ));
        assert!(matches!(
            TerminalSize::new(24, 0),
            Err(SessionError::InvalidTerminalSize { rows: 24, cols: 0 })
        ));
    }

    #[test]
    fn default_matches_a_conventional_terminal() {
        assert_eq!(TerminalSize::default(), TerminalSize::new(24, 80).unwrap());
    }

    #[test]
    fn validated_dimensions_are_available_only_through_getters() {
        let size = TerminalSize::with_pixels(37, 119, 952, 592).unwrap();

        assert_eq!(size.rows(), 37);
        assert_eq!(size.cols(), 119);
        assert_eq!(size.pixel_width(), 952);
        assert_eq!(size.pixel_height(), 592);
        assert!(size.validate().is_ok());
    }

    #[test]
    fn validation_defends_spawn_and_resize_boundaries() {
        let invalid = TerminalSize {
            rows: 0,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        };

        assert!(matches!(
            invalid.validate(),
            Err(SessionError::InvalidTerminalSize { rows: 0, cols: 80 })
        ));
    }
}
