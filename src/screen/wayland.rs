//! Wayland screen control using wlr-output-power-management protocol.
//!
//! This module is only compiled when the `wayland-control` feature is enabled.

use async_trait::async_trait;
use tracing::{debug, info, warn};

use crate::error::ScreenError;
use crate::screen::ScreenController;

/// Wayland screen controller using wlr-output-power-management.
///
/// Note: Full Wayland integration requires running a Wayland event loop,
/// which is complex for a daemon. This implementation provides a basic
/// wrapper that may need enhancement for production use.
pub struct WaylandController {
    available: bool,
}

impl WaylandController {
    /// Create a new Wayland controller.
    pub fn new() -> Result<Self, ScreenError> {
        // Check if we have a Wayland display
        let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
        let xdg_session_type = std::env::var("XDG_SESSION_TYPE").ok();

        let available = wayland_display.is_some()
            || xdg_session_type.as_deref() == Some("wayland");

        if !available {
            warn!("Wayland display not detected. Set WAYLAND_DISPLAY or use brightness method.");
            return Err(ScreenError::WaylandFailed(
                "No Wayland display available".to_string(),
            ));
        }

        info!(
            "Wayland controller initialized (display: {:?})",
            wayland_display
        );

        Ok(Self { available })
    }
}

#[async_trait]
impl ScreenController for WaylandController {
    async fn turn_on(&self) -> Result<(), ScreenError> {
        debug!("Wayland: turn_on");

        // Use wlr-randr as a fallback command-line tool
        // This is more reliable than direct protocol access for a daemon
        let output = tokio::process::Command::new("wlr-randr")
            .args(["--output", "DSI-1", "--on"])
            .output()
            .await
            .map_err(|e| ScreenError::WaylandFailed(format!("Failed to run wlr-randr: {}", e)))?;

        if !output.status.success() {
            // Try HDMI-1 as fallback
            let output2 = tokio::process::Command::new("wlr-randr")
                .args(["--output", "HDMI-1", "--on"])
                .output()
                .await
                .map_err(|e| {
                    ScreenError::WaylandFailed(format!("Failed to run wlr-randr: {}", e))
                })?;

            if !output2.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("wlr-randr failed: {}", stderr);
            }
        }

        Ok(())
    }

    async fn turn_off(&self) -> Result<(), ScreenError> {
        debug!("Wayland: turn_off");

        let output = tokio::process::Command::new("wlr-randr")
            .args(["--output", "DSI-1", "--off"])
            .output()
            .await
            .map_err(|e| ScreenError::WaylandFailed(format!("Failed to run wlr-randr: {}", e)))?;

        if !output.status.success() {
            // Try HDMI-1 as fallback
            let _ = tokio::process::Command::new("wlr-randr")
                .args(["--output", "HDMI-1", "--off"])
                .output()
                .await;
        }

        Ok(())
    }

    async fn set_brightness(&self, level: u8) -> Result<(), ScreenError> {
        debug!("Wayland: set_brightness to {}", level);

        // Wayland doesn't have a direct brightness protocol for most compositors
        // Fall back to sysfs or DDC for actual brightness control
        // For now, treat 0 as off and anything else as on
        if level == 0 {
            self.turn_off().await
        } else {
            self.turn_on().await
        }
    }

    async fn get_brightness(&self) -> Result<u8, ScreenError> {
        // Can't easily query brightness via Wayland
        // Return a default value
        Ok(255)
    }

    fn is_available(&self) -> bool {
        self.available
    }
}
