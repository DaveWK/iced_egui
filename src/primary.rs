//! Optional primary-selection providers.
use std::{cell::RefCell, rc::Rc};

/// A primary-selection clipboard, distinct from the standard copy/paste buffer.
///
/// Implement this for a host toolkit or use `IcedHost::enable_primary_selection`
/// on Linux with the `primary-selection` feature. Keep the provider alive for as
/// long as it should own the selection. Errors are available from the host.
pub trait PrimarySelection {
    /// Read the current selection. `None` means that no text is available.
    fn read(&mut self) -> Result<Option<String>, String>;
    /// Claim the primary selection with the supplied text.
    fn write(&mut self, text: String) -> Result<(), String>;
}

#[derive(Default)]
pub(crate) struct Primary {
    pub provider: Option<Box<dyn PrimarySelection>>,
    pub error: Option<String>,
}
pub(crate) type SharedPrimary = Rc<RefCell<Primary>>;
impl Primary {
    pub fn read(&mut self) -> Option<String> {
        match self.provider.as_mut()?.read() {
            Ok(text) => text,
            Err(error) => {
                self.error = Some(error);
                None
            }
        }
    }
    pub fn write(&mut self, text: String) {
        if let Some(provider) = &mut self.provider
            && let Err(error) = provider.write(text)
        {
            self.error = Some(error);
        }
    }
}

#[cfg(all(feature = "primary-selection", target_os = "linux"))]
pub(crate) struct Native(pub arboard::Clipboard);
#[cfg(all(feature = "primary-selection", target_os = "linux"))]
impl PrimarySelection for Native {
    fn read(&mut self) -> Result<Option<String>, String> {
        use arboard::GetExtLinux;
        match self
            .0
            .get()
            .clipboard(arboard::LinuxClipboardKind::Primary)
            .text()
        {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }
    fn write(&mut self, text: String) -> Result<(), String> {
        use arboard::SetExtLinux;
        self.0
            .set()
            .clipboard(arboard::LinuxClipboardKind::Primary)
            .text(text)
            .map_err(|error| error.to_string())
    }
}
