use std::io::Write;
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

pub enum Style {
    LineNumber,
    Error,
    Warning,
    Info,
    Bold,
    Normal,
}

pub struct ColoredWriter {
    inner: StandardStream,
}

impl ColoredWriter {
    pub fn new() -> Self {
        Self {
            inner: StandardStream::stderr(ColorChoice::Auto),
        }
    }

    pub fn write(&mut self, message: impl std::fmt::Display, style: Style) -> std::io::Result<()> {
        match style {
            Style::LineNumber => {
                self.inner
                    .set_color(ColorSpec::new().set_bold(true).set_intense(true).set_fg(
                        if cfg!(windows) {
                            Some(Color::Cyan)
                        } else {
                            Some(Color::Blue)
                        },
                    ))?
            }

            Style::Error => self
                .inner
                .set_color(ColorSpec::new().set_bold(true).set_fg(Some(Color::Red)))?,

            Style::Warning => self
                .inner
                .set_color(ColorSpec::new().set_bold(true).set_fg(Some(Color::Yellow)))?,

            Style::Info => self
                .inner
                .set_color(ColorSpec::new().set_bold(true).set_fg(Some(Color::Blue)))?,

            Style::Bold => self.inner.set_color(ColorSpec::new().set_bold(true))?,

            Style::Normal => self.inner.reset()?,
        }
        write!(self.inner, "{}", message)?;

        Ok(())
    }
}
