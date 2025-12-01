//! Camera hardware controls for V4L2 devices
//!
//! This module provides functionality to query and modify camera hardware controls
//! such as brightness, contrast, saturation, exposure, etc. Controls are queried
//! directly from the camera hardware and can be adjusted in real-time.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io;
use tracing::{debug, info, warn};
use v4l::control::{Control, Description, Flags, MenuItem, Type, Value};
use v4l::Device;

/// Represents a camera control with its metadata and current value
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraControl {
    /// Control ID (V4L2 control ID)
    pub id: u32,
    /// Human-readable name
    pub name: String,
    /// Control type
    #[serde(rename = "type")]
    pub control_type: ControlType,
    /// Minimum value (for integer/integer64 types)
    pub minimum: i64,
    /// Maximum value (for integer/integer64 types)
    pub maximum: i64,
    /// Step size (for integer types)
    pub step: u64,
    /// Default value
    pub default: i64,
    /// Current value
    pub value: ControlValue,
    /// Menu items (for menu type controls)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub menu_items: Option<Vec<MenuItemInfo>>,
    /// Control flags
    pub flags: ControlFlags,
}

/// Simplified control type for serialization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ControlType {
    Integer,
    Boolean,
    Menu,
    Button,
    Integer64,
    String,
    Bitmask,
    IntegerMenu,
    Unknown,
}

impl From<Type> for ControlType {
    fn from(t: Type) -> Self {
        match t {
            Type::Integer => ControlType::Integer,
            Type::Boolean => ControlType::Boolean,
            Type::Menu => ControlType::Menu,
            Type::Button => ControlType::Button,
            Type::Integer64 => ControlType::Integer64,
            Type::String => ControlType::String,
            Type::Bitmask => ControlType::Bitmask,
            Type::IntegerMenu => ControlType::IntegerMenu,
            _ => ControlType::Unknown,
        }
    }
}

/// Simplified control value for serialization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ControlValue {
    Integer(i64),
    Boolean(bool),
    String(String),
    None,
}

impl From<Value> for ControlValue {
    fn from(v: Value) -> Self {
        match v {
            Value::Integer(i) => ControlValue::Integer(i),
            Value::Boolean(b) => ControlValue::Boolean(b),
            Value::String(s) => ControlValue::String(s),
            Value::None => ControlValue::None,
            _ => ControlValue::None,
        }
    }
}

impl ControlValue {
    /// Convert to V4L2 control value
    pub fn to_v4l2_value(&self) -> Value {
        match self {
            ControlValue::Integer(i) => Value::Integer(*i),
            ControlValue::Boolean(b) => Value::Boolean(*b),
            ControlValue::String(s) => Value::String(s.clone()),
            ControlValue::None => Value::None,
        }
    }

    /// Get as i64 for simple controls
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            ControlValue::Integer(i) => Some(*i),
            ControlValue::Boolean(b) => Some(if *b { 1 } else { 0 }),
            _ => None,
        }
    }
}

/// Menu item information for menu-type controls
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuItemInfo {
    /// Menu item index
    pub index: u32,
    /// Menu item name or value
    pub label: String,
}

/// Control flags
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct ControlFlags {
    pub disabled: bool,
    pub grabbed: bool,
    pub read_only: bool,
    pub inactive: bool,
    pub write_only: bool,
    pub volatile: bool,
}

impl From<Flags> for ControlFlags {
    fn from(f: Flags) -> Self {
        ControlFlags {
            disabled: f.contains(Flags::DISABLED),
            grabbed: f.contains(Flags::GRABBED),
            read_only: f.contains(Flags::READ_ONLY),
            inactive: f.contains(Flags::INACTIVE),
            write_only: f.contains(Flags::WRITE_ONLY),
            volatile: f.contains(Flags::VOLATILE),
        }
    }
}

/// Known V4L2 control IDs for common camera controls
#[allow(dead_code)]
pub mod control_ids {
    // User controls (V4L2_CID_BASE = 0x00980900)
    pub const BRIGHTNESS: u32 = 0x00980900;
    pub const CONTRAST: u32 = 0x00980901;
    pub const SATURATION: u32 = 0x00980902;
    pub const HUE: u32 = 0x00980903;
    pub const AUTO_WHITE_BALANCE: u32 = 0x0098090c;
    pub const RED_BALANCE: u32 = 0x0098090e;
    pub const BLUE_BALANCE: u32 = 0x0098090f;
    pub const GAMMA: u32 = 0x00980910;
    pub const EXPOSURE: u32 = 0x00980911;
    pub const AUTOGAIN: u32 = 0x00980912;
    pub const GAIN: u32 = 0x00980913;
    pub const HFLIP: u32 = 0x00980914;
    pub const VFLIP: u32 = 0x00980915;
    pub const POWER_LINE_FREQUENCY: u32 = 0x00980918;
    pub const HUE_AUTO: u32 = 0x00980919;
    pub const WHITE_BALANCE_TEMPERATURE: u32 = 0x0098091a;
    pub const SHARPNESS: u32 = 0x0098091b;
    pub const BACKLIGHT_COMPENSATION: u32 = 0x0098091c;

    // Camera class controls (V4L2_CID_CAMERA_CLASS_BASE = 0x009a0900)
    pub const EXPOSURE_AUTO: u32 = 0x009a0901;
    pub const EXPOSURE_ABSOLUTE: u32 = 0x009a0902;
    pub const EXPOSURE_AUTO_PRIORITY: u32 = 0x009a0903;
    pub const PAN_RELATIVE: u32 = 0x009a0904;
    pub const TILT_RELATIVE: u32 = 0x009a0905;
    pub const PAN_ABSOLUTE: u32 = 0x009a0908;
    pub const TILT_ABSOLUTE: u32 = 0x009a0909;
    pub const FOCUS_ABSOLUTE: u32 = 0x009a090a;
    pub const FOCUS_RELATIVE: u32 = 0x009a090b;
    pub const FOCUS_AUTO: u32 = 0x009a090c;
    pub const ZOOM_ABSOLUTE: u32 = 0x009a090d;
    pub const ZOOM_RELATIVE: u32 = 0x009a090e;
    pub const ZOOM_CONTINUOUS: u32 = 0x009a090f;
}

/// Camera controls manager
pub struct CameraControlsManager {
    device_path: String,
    /// Cached control descriptions
    controls: Vec<CameraControl>,
}

impl CameraControlsManager {
    /// Create a new camera controls manager for the given device
    pub fn new(device_path: &str) -> Self {
        Self {
            device_path: device_path.to_string(),
            controls: Vec::new(),
        }
    }

    /// Query all available controls from the camera
    pub fn query_controls(&mut self) -> Result<&Vec<CameraControl>> {
        let device = Device::with_path(&self.device_path)
            .with_context(|| format!("Failed to open device: {}", self.device_path))?;

        let descriptions = device
            .query_controls()
            .with_context(|| "Failed to query camera controls")?;

        self.controls.clear();

        for desc in descriptions {
            // Skip controls that are disabled or are control classes
            if desc.flags.contains(Flags::DISABLED) {
                continue;
            }
            if desc.typ == Type::CtrlClass {
                continue;
            }

            // Get the current value
            let value = match device.control(desc.id) {
                Ok(ctrl) => ControlValue::from(ctrl.value),
                Err(e) => {
                    debug!("Could not read control {}: {}", desc.name, e);
                    ControlValue::None
                }
            };

            let menu_items = desc.items.as_ref().map(|items| {
                items
                    .iter()
                    .map(|(idx, item)| MenuItemInfo {
                        index: *idx,
                        label: match item {
                            MenuItem::Name(name) => name.clone(),
                            MenuItem::Value(val) => val.to_string(),
                        },
                    })
                    .collect()
            });

            let control = CameraControl {
                id: desc.id,
                name: desc.name.clone(),
                control_type: ControlType::from(desc.typ),
                minimum: desc.minimum,
                maximum: desc.maximum,
                step: desc.step,
                default: desc.default,
                value,
                menu_items,
                flags: ControlFlags::from(desc.flags),
            };

            debug!(
                "Found control: {} (id={:#x}, type={:?}, value={:?})",
                control.name, control.id, control.control_type, control.value
            );

            self.controls.push(control);
        }

        info!(
            "Discovered {} camera controls on {}",
            self.controls.len(),
            self.device_path
        );

        Ok(&self.controls)
    }

    /// Get the cached controls (call query_controls first)
    pub fn get_controls(&self) -> &Vec<CameraControl> {
        &self.controls
    }

    /// Get a specific control by ID
    pub fn get_control(&self, id: u32) -> Option<&CameraControl> {
        self.controls.iter().find(|c| c.id == id)
    }

    /// Get a specific control by name (case-insensitive)
    pub fn get_control_by_name(&self, name: &str) -> Option<&CameraControl> {
        let name_lower = name.to_lowercase();
        self.controls
            .iter()
            .find(|c| c.name.to_lowercase() == name_lower)
    }

    /// Set a control value by ID
    pub fn set_control(&mut self, id: u32, value: ControlValue) -> Result<()> {
        let device = Device::with_path(&self.device_path)
            .with_context(|| format!("Failed to open device: {}", self.device_path))?;

        let v4l2_value = value.to_v4l2_value();

        let ctrl = Control {
            id,
            value: v4l2_value,
        };

        device
            .set_control(ctrl)
            .with_context(|| format!("Failed to set control {:#x}", id))?;

        // Update cached value
        if let Some(control) = self.controls.iter_mut().find(|c| c.id == id) {
            control.value = value.clone();
            info!(
                "Set control '{}' (id={:#x}) to {:?}",
                control.name, id, value
            );
        }

        Ok(())
    }

    /// Set a control value by name (case-insensitive)
    pub fn set_control_by_name(&mut self, name: &str, value: ControlValue) -> Result<()> {
        let name_lower = name.to_lowercase();
        let id = self
            .controls
            .iter()
            .find(|c| c.name.to_lowercase() == name_lower)
            .map(|c| c.id)
            .with_context(|| format!("Control not found: {}", name))?;

        self.set_control(id, value)
    }

    /// Reset a control to its default value
    pub fn reset_control(&mut self, id: u32) -> Result<()> {
        let default_value = self
            .controls
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.default)
            .with_context(|| format!("Control not found: {:#x}", id))?;

        self.set_control(id, ControlValue::Integer(default_value))
    }

    /// Reset all controls to their default values
    pub fn reset_all_controls(&mut self) -> Result<()> {
        let defaults: Vec<(u32, i64)> = self
            .controls
            .iter()
            .filter(|c| !c.flags.read_only && !c.flags.inactive)
            .map(|c| (c.id, c.default))
            .collect();

        for (id, default) in defaults {
            if let Err(e) = self.set_control(id, ControlValue::Integer(default)) {
                warn!("Failed to reset control {:#x}: {}", id, e);
            }
        }

        // Re-query to get updated values
        self.query_controls()?;
        Ok(())
    }

    /// Refresh the current values of all controls
    pub fn refresh_values(&mut self) -> Result<()> {
        let device = Device::with_path(&self.device_path)
            .with_context(|| format!("Failed to open device: {}", self.device_path))?;

        for control in &mut self.controls {
            match device.control(control.id) {
                Ok(ctrl) => {
                    control.value = ControlValue::from(ctrl.value);
                }
                Err(e) => {
                    debug!("Could not refresh control {}: {}", control.name, e);
                }
            }
        }

        Ok(())
    }

    /// Export current control values to a HashMap (for saving to config)
    pub fn export_settings(&self) -> HashMap<String, ControlValue> {
        let mut settings = HashMap::new();

        for control in &self.controls {
            // Only export controls that can be set
            if control.flags.read_only || control.flags.inactive {
                continue;
            }

            // Use a normalized name as key
            let key = control.name.to_lowercase().replace(' ', "_");
            settings.insert(key, control.value.clone());
        }

        settings
    }

    /// Import control values from a HashMap (from config)
    pub fn import_settings(&mut self, settings: &HashMap<String, ControlValue>) -> Result<()> {
        for (name, value) in settings {
            // Find control by normalized name
            let name_normalized = name.to_lowercase().replace('_', " ");
            if let Some(control) = self
                .controls
                .iter()
                .find(|c| c.name.to_lowercase() == name_normalized)
            {
                if control.flags.read_only || control.flags.inactive {
                    debug!("Skipping read-only/inactive control: {}", name);
                    continue;
                }

                if let Err(e) = self.set_control(control.id, value.clone()) {
                    warn!("Failed to import control '{}': {}", name, e);
                }
            } else {
                debug!("Control not found during import: {}", name);
            }
        }

        Ok(())
    }
}

/// Stored camera settings for config file
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CameraSettings {
    /// Control values by normalized name
    #[serde(default)]
    pub controls: HashMap<String, i64>,
}

impl CameraSettings {
    /// Create from a controls manager
    pub fn from_manager(manager: &CameraControlsManager) -> Self {
        let mut controls = HashMap::new();

        for control in manager.get_controls() {
            if control.flags.read_only || control.flags.inactive {
                continue;
            }

            if let Some(val) = control.value.as_i64() {
                let key = control.name.to_lowercase().replace(' ', "_");
                controls.insert(key, val);
            }
        }

        Self { controls }
    }

    /// Apply settings to a controls manager
    pub fn apply_to_manager(&self, manager: &mut CameraControlsManager) -> Result<()> {
        for (name, value) in &self.controls {
            let name_normalized = name.to_lowercase().replace('_', " ");
            if let Some(control) = manager
                .get_controls()
                .iter()
                .find(|c| c.name.to_lowercase() == name_normalized)
            {
                if control.flags.read_only || control.flags.inactive {
                    continue;
                }

                let ctrl_value = if control.control_type == ControlType::Boolean {
                    ControlValue::Boolean(*value != 0)
                } else {
                    ControlValue::Integer(*value)
                };

                if let Err(e) = manager.set_control(control.id, ctrl_value) {
                    warn!("Failed to apply setting '{}': {}", name, e);
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_control_type_conversion() {
        assert_eq!(ControlType::from(Type::Integer), ControlType::Integer);
        assert_eq!(ControlType::from(Type::Boolean), ControlType::Boolean);
        assert_eq!(ControlType::from(Type::Menu), ControlType::Menu);
    }

    #[test]
    fn test_control_value_conversion() {
        let v = ControlValue::Integer(100);
        assert_eq!(v.as_i64(), Some(100));

        let v = ControlValue::Boolean(true);
        assert_eq!(v.as_i64(), Some(1));

        let v = ControlValue::Boolean(false);
        assert_eq!(v.as_i64(), Some(0));
    }
}
