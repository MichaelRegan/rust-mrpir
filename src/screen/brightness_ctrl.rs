//! Brightness control via sysfs or the brightness crate.

use async_trait::async_trait;
use std::path::PathBuf;
use tracing::{debug, warn};

use crate::error::ScreenError;
use crate::screen::ScreenController;

/// Brightness controller using sysfs or the brightness crate.
pub struct BrightnessController {
    /// Optional manual brightness path (sysfs)
    manual_path: Option<PathBuf>,
    /// Whether the controller is available
    available: bool,
}

impl BrightnessController {
    /// Create a new brightness controller.
    pub fn new(manual_path: Option<PathBuf>) -> Result<Self, ScreenError> {
        let available = if manual_path.is_some() {
            // Check if manual path exists
            manual_path.as_ref().unwrap().exists()
        } else {
            // Check if brightness crate can find devices
            #[cfg(feature = "brightness-control")]
            {
                // We'll check availability on first use
                true
            }
            #[cfg(not(feature = "brightness-control"))]
            {
                false
            }
        };

        if !available {
            warn!("Brightness control not available");
        }

        Ok(Self {
            manual_path,
            available,
        })
    }

    /// Write brightness to sysfs path.
    async fn write_sysfs(&self, path: &PathBuf, value: u8) -> Result<(), ScreenError> {
        tokio::fs::write(path, value.to_string())
            .await
            .map_err(|e| ScreenError::BrightnessFailed(format!("Failed to write to sysfs: {}", e)))
    }

    /// Read brightness from sysfs path.
    async fn read_sysfs(&self, path: &PathBuf) -> Result<u8, ScreenError> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| ScreenError::BrightnessFailed(format!("Failed to read sysfs: {}", e)))?;

        content
            .trim()
            .parse()
            .map_err(|e| ScreenError::BrightnessFailed(format!("Invalid brightness value: {}", e)))
    }
}

#[async_trait]
impl ScreenController for BrightnessController {
    async fn turn_on(&self) -> Result<(), ScreenError> {
        debug!("Brightness controller: turn_on (set to max)");
        self.set_brightness(255).await
    }

    async fn turn_off(&self) -> Result<(), ScreenError> {
        debug!("Brightness controller: turn_off (set to 0)");
        self.set_brightness(0).await
    }

    async fn set_brightness(&self, level: u8) -> Result<(), ScreenError> {
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
                    Ok(dev) => {
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

    async fn get_brightness(&self) -> Result<u8, ScreenError> {
        if let Some(ref path) = self.manual_path {
            return self.read_sysfs(path).await;
        }

        #[cfg(feature = "brightness-control")]
        {
            use brightness::Brightness;
            use futures_util::StreamExt;

            let mut devices = brightness::brightness_devices();

            if let Some(device) = devices.next().await {
                match device {
                    Ok(dev) => {
                        let percentage = dev.get().await.map_err(|e| {
                            ScreenError::BrightnessFailed(format!("Failed to get brightness: {}", e))
                        })?;
                        // Convert percentage to 0-255
                        return Ok(((percentage * 255) / 100) as u8);
                    }
                    Err(e) => {
                        return Err(ScreenError::BrightnessFailed(format!(
                            "Failed to enumerate device: {}",
                            e
                        )));
                    }
                }
            }

            Err(ScreenError::NotAvailable(
                "No brightness devices found".to_string(),
            ))
        }

        #[cfg(not(feature = "brightness-control"))]
        {
            Err(ScreenError::NotAvailable(
                "Brightness control not compiled in".to_string(),
            ))
        }
    }

    fn is_available(&self) -> bool {
        self.available
    }
}
