//! Brightness control via sysfs or the brightness crate.

use std::path::PathBuf;
use tracing::{debug, warn};

use crate::error::ScreenError;

/// Brightness controller using sysfs or the brightness crate.
pub struct BrightnessController {
    /// Optional manual brightness path (sysfs)
    manual_path: Option<PathBuf>,
}

impl BrightnessController {
    /// Create a new brightness controller.
    pub fn new(manual_path: Option<PathBuf>) -> Result<Self, ScreenError> {
        #[cfg(not(feature = "brightness-control"))]
        if manual_path.is_none() {
            return Err(ScreenError::NotAvailable(
                "Brightness control not compiled in and no manual path provided".to_string(),
            ));
        }

        if let Some(ref path) = manual_path {
            if !path.exists() {
                warn!("Brightness sysfs path does not exist: {:?}", path);
            }
        }

        Ok(Self { manual_path })
    }

    /// Write brightness to sysfs path.
    async fn write_sysfs(&self, path: &PathBuf, value: u8) -> Result<(), ScreenError> {
        tokio::fs::write(path, value.to_string())
            .await
            .map_err(|e| ScreenError::BrightnessFailed(format!("Failed to write to sysfs: {}", e)))
    }

    /// Turn the screen on.
    pub async fn turn_on(&self) -> Result<(), ScreenError> {
        debug!("Brightness controller: turn_on (set to max)");
        self.set_brightness(255).await
    }

    /// Turn the screen off.
    pub async fn turn_off(&self) -> Result<(), ScreenError> {
        debug!("Brightness controller: turn_off (set to 0)");
        self.set_brightness(0).await
    }

    /// Set brightness level (0-255).
    pub async fn set_brightness(&self, level: u8) -> Result<(), ScreenError> {
        debug!("Setting brightness to {}", level);

        if let Some(ref path) = self.manual_path {
            return self.write_sysfs(path, level).await;
        }

        #[cfg(feature = "brightness-control")]
        {
            use brightness::Brightness;
            use futures_util::StreamExt;

            let mut devices = brightness::brightness_devices();

            while let Some(device) = devices.next().await {
                match device {
                    Ok(mut dev) => {
                        // Convert 0-255 to percentage
                        let percentage = (level as u32 * 100) / 255;
                        if let Err(e) = dev.set(percentage).await {
                            warn!("Failed to set brightness on device: {}", e);
                        }
                    }
                    Err(e) => {
                        warn!("Failed to enumerate brightness device: {}", e);
                    }
                }
            }

            Ok(())
        }

        #[cfg(not(feature = "brightness-control"))]
        {
            Err(ScreenError::NotAvailable(
                "Brightness control not compiled in".to_string(),
            ))
        }
    }
}
