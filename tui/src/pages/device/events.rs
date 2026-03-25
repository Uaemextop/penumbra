/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use async_trait::async_trait;
use penumbra::Device;
use penumbra::core::storage::Partition;
use strum::IntoEnumIterator;
use strum_macros::{AsRefStr, EnumIter};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;

use crate::components::{ExplorerResult, FileExplorer};

/// Which panel is currently focused
pub enum FocusedPanel {
    Menu,
    PartitionMenu,
}

/// Device connection status, used for UI updates
#[derive(Debug, Clone, PartialEq)]
pub enum DeviceStatus {
    Disconnected,
    Connecting,
    Connected,
}

/// A list of event to which the DevicePage can respond
pub enum DeviceEvent {
    // Progress Bar Events
    /// Start a progress operation, and set the max bytes
    /// and a message
    ProgressStart {
        total_bytes: u64,
        message: String,
    },
    /// Update progress with bytes written
    /// If message is Some, update the message as well
    ProgressUpdate {
        written: u64,
        message: Option<String>,
    },
    /// Finish progress with a final message
    ProgressFinish {
        message: String,
    },
    /// Notify of device status change (Disconnected, Connecting, Connected)
    StatusChanged(DeviceStatus),
    /// Notify that device is connected (To be sent once)
    Connected(Device),

    /// Change focused panel
    FocusPanel(FocusedPanel),
    /// Show the provided file explorer
    ShowExplorer(FileExplorer),
    /// Yield result from the file explorer
    ExplorerResult(ExplorerResult),

    // Opens the dialog with an error message
    Error(String),
    // Little text on top
    HeaderStatus(String),

    /// Whether to enable or disable input.
    /// Used to block input during operations
    Input(bool),
}

/// A list of event used by the page and callbacks to communicate to
/// each other.
/// Works via a bi-directional channel.
#[derive(Debug, Clone)]
pub enum CallbackEvent {
    /// All selected partitions
    PartitionsSelected(Vec<Partition>),
    /// A partition that got selected or unselected in the explorer
    PartitionToggled(Partition, bool),
    ExplorerResult(ExplorerResult),
}

/// The Menu Actions available
/// Used for both mapping a menu entry to a callback, and rendering the menu
#[derive(EnumIter, AsRefStr, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceAction {
    #[strum(serialize = "Unlock Bootloader")]
    UnlockBootloader,
    #[strum(serialize = "Lock Bootloader")]
    LockBootloader,
    #[strum(serialize = "Read Partition")]
    ReadPartition,
    #[strum(serialize = "Write Partition")]
    WritePartition,
    #[strum(serialize = "Back to Menu")]
    BackToMenu,
}

/// Represent a callback for a device action
/// The callback is executed in an async task, allowing for background operations.
/// The callback can communicate with the page via the provided channels.
/// * Event TX: To ask to the page to perform UI actions, specifically for UI events
/// * Callback TX/RX: To communicate with the page for specific data (like selected partitions)
#[async_trait]
pub trait DeviceActionCallback: Send + Sync {
    async fn execute(
        &self,
        device: Arc<Mutex<Device>>,
        event_tx: mpsc::Sender<DeviceEvent>,
        cb_tx: mpsc::Sender<CallbackEvent>,
        cb_rx: mpsc::Receiver<CallbackEvent>,
    ) -> Result<()>;
}

pub struct DeviceState {
    pub status: DeviceStatus,
    pub last_status_change: Instant,
}

impl DeviceState {
    pub fn new() -> Self {
        Self { status: DeviceStatus::Disconnected, last_status_change: Instant::now() }
    }

    pub fn set_status(&mut self, status: DeviceStatus) {
        self.status = status;
        self.last_status_change = Instant::now();
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.status, DeviceStatus::Connected)
    }
}
