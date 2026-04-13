//! Screen control module with pluggable backends.

mod brightness_ctrl;
#[cfg(feature = "wayland-control")]
mod wayland;

use tracing::{debug, info};

use crate::config::{ScreenConfig, ScreenMethod};
use crate::error::ScreenError;

use brightness_ctrl::BrightnessController;
#[cfg(feature = "wayland-control")]
use wayland::WaylandController;

/// Screen controller enum - avoids trait objects and async-trait overhead.
pub enum ScreenController {
    /// No-op controller when screen control is disabled
    NoOp,
    /// Brightness control via sysfs or brightness crate
    Brightness(BrightnessController),
    /// Wayland wlr-output-power control
    #[cfg(feature = "wayland-control")]
    Wayland(WaylandController),
}

impl ScreenController {
    /// Turn the screen on.
    pub async fn turn_on(&self) -> Result<(), ScreenError> {
        match self {
            Self::NoOp => {
                debug!("Screen control disabled: turn_on ignored");
                Ok(())
            }
            Self::Brightness(ctrl) => ctrl.turn_on().await,
            #[cfg(feature = "wayland-control")]
            Self::Wayland(ctrl) => ctrl.turn_on().await,
        }
    }

    /// Turn the screen off.
    pub async fn turn_off(&self) -> Result<(), ScreenError> {
        match self {
            Self::NoOp => {
                debug!("Screen control disabled: turn_off ignored");
                Ok(())
            }
            Self::Brightness(ctrl) => ctrl.turn_off().await,
            #[cfg(feature = "wayland-control")]
            Self::Wayland(ctrl) => ctrl.turn_off().await,
        }
    }

    /// Set brightness level (0-255).
    pub async fn set_brightness(&self, level: u8) -> Result<(), ScreenError> {
        match self {
            Self::NoOp => {
                debug!("Screen control disabled: set_brightness ignored");
                Ok(())
            }
            Self::Brightness(ctrl) => ctrl.set_brightness(level).await,
            #[cfg(feature = "wayland-control")]
            Self::Wayland(ctrl) => ctrl.set_brightness(level).await,
        }
    }

    /// Check if screen control is available.
    pub fn is_available(&self) -> bool {
        !matches!(self, Self::NoOp)
    }
}

/// Create a screen controller based on configuration.
pub fn create_controller(config: &ScreenConfig) -> Result<ScreenController, ScreenError> {
    if !config.enabled {
        info!("Screen control disabled");
        return Ok(ScreenController::NoOp);
    }

    match config.method {
        ScreenMethod::None => {
            info!("Screen control method: none");
            Ok(ScreenController::NoOp)
        }
        ScreenMethod::Brightness => {
            info!("Screen control method: brightness (sysfs)");
            Ok(ScreenController::Brightness(BrightnessController::new(
                config.brightness_path.clone(),
            )?))
        }
        #[cfg(feature = "wayland-control")]
        ScreenMethod::Wayland => {
            info!("Screen control method: wayland");
            Ok(ScreenController::Wayland(WaylandController::new()?))
        }
        #[cfg(not(feature = "wayland-control"))]
        ScreenMethod::Wayland => Err(ScreenError::NotAvailable(
            "Wayland support not compiled in. Rebuild with --features wayland-control".to_string(),
        )),
        ScreenMethod::Xscreensaver => Err(ScreenError::NotAvailable(
            "xscreensaver backend not implemented in Rust rewrite. Use brightness or wayland."
                .to_string(),
        )),
    }
}

/// Screen manager that handles brightness transitions and timeouts.
pub struct ScreenManager {
    controller: ScreenController,
    config: ScreenConfig,
    current_brightness: u8,
}

impl ScreenManager {
    /// Create a new screen manager.
    pub fn new(config: &ScreenConfig) -> Result<Self, ScreenError> {
        let controller = create_controller(config)?;
        let current_brightness = config.bright_brightness;

        Ok(Self {
            controller,
            config: config.clone(),
            current_brightness,
        })
    }

    /// Handle motion detected - brighten screen.
    pub async fn on_motion(&mut self) -> Result<(), ScreenError> {
        if !self.config.enabled {
            return Ok(());
        }

        info!("Motion detected: brightening screen");

        // Turn on and set to bright
        self.controller.turn_on().await?;

        if self.config.transition_time_secs > 0 {
            self.transition_brightness(self.config.bright_brightness)
                .await?;
        } else {
            self.controller
                .set_brightness(self.config.bright_brightness)
                .await?;
            self.current_brightness = self.config.bright_brightness;
        }

        Ok(())
    }

    /// Handle motion timeout - dim screen.
    pub async fn on_motion_timeout(&mut self) -> Result<(), ScreenError> {
        if !self.config.enabled {
            return Ok(());
        }

        info!("Motion timeout: dimming screen");

        if self.config.transition_time_secs > 0 {
            self.transition_brightness(self.config.dim_brightness)
                .await?;
        } else {
            self.controller
                .set_brightness(self.config.dim_brightness)
                .await?;
            self.current_brightness = self.config.dim_brightness;
        }

        Ok(())
    }

    /// Handle night mode - turn off screen.
    pub async fn on_night_mode(&mut self) -> Result<(), ScreenError> {
        if !self.config.enabled {
            return Ok(());
        }

        info!("Night mode: turning off screen");
        self.controller.turn_off().await?;
        self.current_brightness = 0;

        Ok(())
    }

    /// Smoothly transition to a target brightness.
    async fn transition_brightness(&mut self, target: u8) -> Result<(), ScreenError> {
        let steps = 20u32;
        let delay_ms = (self.config.transition_time_secs * 1000) / steps as u64;

        let current = self.current_brightness as i32;
        let target_i = target as i32;
        let step_size = (target_i - current) / steps as i32;

        for i in 1..=steps {
            let brightness = if i == steps {
                target
            } else {
                (current + step_size * i as i32).clamp(0, 255) as u8
            };

            self.controller.set_brightness(brightness).await?;
            tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
        }

        self.current_brightness = target;
        Ok(())
    }

    /// Check if screen control is available.
    pub fn is_available(&self) -> bool {
        self.controller.is_available()
    }
}
