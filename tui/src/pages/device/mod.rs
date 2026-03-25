/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
mod callbacks;
pub mod events;
mod render;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use human_bytes::human_bytes;
use penumbra::core::devinfo::DevInfoData;
use penumbra::core::storage::{Partition, Storage};
use penumbra::{Device, DeviceBuilder, find_mtk_port};
#[cfg(target_os = "windows")]
use ratatui::crossterm::event::KeyEventKind;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::Frame;
use strum::IntoEnumIterator;
use tokio::spawn;
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tokio::time::{Duration, sleep};

use crate::app::{AppCtx, AppPage};
use crate::components::selectable_list::{
    ListItemEntry, ListItemEntryBuilder, SelectableList, SelectableListBuilder,
};
use crate::components::{ExplorerResult, FileExplorer, ProgressBar, Stars};
use crate::pages::Page;

use self::callbacks::{
    LockBootloaderCallback, ReadPartitionCallback, UnlockBootloaderCallback, WritePartitionCallback,
};
use self::events::{
    CallbackEvent, DeviceAction, DeviceActionCallback, DeviceEvent, DeviceState, DeviceStatus,
    FocusedPanel,
};

pub struct DevicePage {
    pub device: Option<Arc<Mutex<Device>>>,
    pub device_state: DeviceState,
    pub status_message: Option<String>,

    // Event channel for async communication
    /// UI Events
    pub event_tx: mpsc::Sender<DeviceEvent>,
    pub event_rx: mpsc::Receiver<DeviceEvent>,
    /// Callbacks Events, for transmitting Callback results
    pub callback_tx: Option<mpsc::Sender<CallbackEvent>>,
    pub callback_rx: Option<mpsc::Receiver<CallbackEvent>>,

    // Action callbacks and active operations
    pub action_callbacks: HashMap<DeviceAction, Arc<dyn DeviceActionCallback>>,
    pub active_operations: HashMap<DeviceAction, JoinHandle<()>>,

    // UI components (foundation only, not rendered yet)
    pub(super) stars: Stars,
    pub(super) progress_bar: ProgressBar,
    pub(super) menu: SelectableList,
    pub(super) partition_list: SelectableList,
    pub(super) explorer: Option<FileExplorer>,

    // UI State
    pub focused_panel: FocusedPanel,
    pub input_enabled: bool,

    // Various Device Info
    pub partitions: Vec<Partition>,
    pub devinfo: Option<DevInfoData>,
    pub storage: Option<Arc<dyn Storage + Send + Sync>>,
}

impl DevicePage {
    pub fn new() -> Self {
        let (event_tx, event_rx) = mpsc::channel(32);
        let progress_bar = ProgressBar::new();

        // Build menu from actions
        let actions: Vec<DeviceAction> = DeviceAction::iter().collect();
        let menu_items: Vec<ListItemEntry> = actions
            .iter()
            .map(|action| {
                let icon = match action {
                    DeviceAction::UnlockBootloader => '🔓',
                    DeviceAction::LockBootloader => '🔒',
                    DeviceAction::ReadPartition => '📁',
                    DeviceAction::WritePartition => '📝',
                    DeviceAction::BackToMenu => '↩',
                };
                ListItemEntryBuilder::new(action.as_ref().to_string()).icon(icon).build().unwrap()
            })
            .collect();

        let menu = SelectableListBuilder::default()
            .items(menu_items)
            .highlight_symbol(">> ".to_string())
            .build()
            .unwrap();

        let partition_list = SelectableListBuilder::default()
            .items(Vec::new())
            .highlight_symbol(">> ".to_string())
            .build()
            .unwrap();

        let mut page = Self {
            device: None,
            device_state: DeviceState::new(),
            status_message: None,
            event_tx,
            event_rx,
            callback_tx: None,
            callback_rx: None,
            action_callbacks: HashMap::new(),
            active_operations: HashMap::new(),
            stars: Stars::default(),
            progress_bar,
            menu,
            explorer: None,
            focused_panel: FocusedPanel::Menu,
            input_enabled: true,
            partition_list,
            partitions: Vec::new(),
            devinfo: None,
            storage: None,
        };

        page.register_action(DeviceAction::UnlockBootloader, Arc::new(UnlockBootloaderCallback));
        page.register_action(DeviceAction::LockBootloader, Arc::new(LockBootloaderCallback));
        page.register_action(DeviceAction::ReadPartition, Arc::new(ReadPartitionCallback));
        page.register_action(DeviceAction::WritePartition, Arc::new(WritePartitionCallback));

        page
    }

    pub fn register_action(
        &mut self,
        action: DeviceAction,
        callback: Arc<dyn DeviceActionCallback>,
    ) {
        self.action_callbacks.insert(action, callback);
    }

    pub async fn execute_action(&mut self, action: DeviceAction) {
        // Abort any existing operation for the same action, as a safety measure
        if let Some(handle) = self.active_operations.remove(&action) {
            handle.abort();
        }

        let Some(device) = self.device.clone() else {
            self.event_tx.send(DeviceEvent::Error("Device not connected".to_string())).await.ok();
            return;
        };

        let Some(callback) = self.action_callbacks.get(&action).cloned() else {
            self.event_tx.send(DeviceEvent::Error("No callback registered".to_string())).await.ok();
            return;
        };

        // From executor to callback
        let (cb_tx_to_callback, cb_rx_from_callback) = mpsc::channel(1);
        // From callback to executor
        let (cb_tx_from_callback, cb_rx_to_main) = mpsc::channel(1);

        self.callback_tx = Some(cb_tx_to_callback);
        self.callback_rx = Some(cb_rx_to_main);

        let event_tx = self.event_tx.clone();

        let handle = tokio::spawn(async move {
            let result = callback
                .execute(device, event_tx.clone(), cb_tx_from_callback, cb_rx_from_callback)
                .await;
            if let Err(e) = result {
                event_tx.send(DeviceEvent::Error(e.to_string())).await.ok();
            }
        });

        self.active_operations.insert(action, handle);
    }

    /// Process all pending events from the event channel
    pub async fn process_events(&mut self, ctx: &mut AppCtx) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                DeviceEvent::ProgressStart { total_bytes, message } => {
                    self.progress_bar.start(total_bytes, message);
                }
                DeviceEvent::ProgressUpdate { written, message } => {
                    self.progress_bar.set_written(written);
                    if let Some(msg) = message {
                        self.progress_bar.set_message(msg);
                    }
                }
                DeviceEvent::ProgressFinish { message } => {
                    self.progress_bar.finish();
                    self.status_message = Some(message);
                }

                DeviceEvent::StatusChanged(status) => {
                    self.device_state.set_status(status);
                }
                DeviceEvent::Connected(mut device) => {
                    self.devinfo = Some(device.dev_info.get_data().await);

                    let partitions = device.get_partitions().await;
                    let partition_list_items: Vec<ListItemEntry> = partitions
                        .iter()
                        .map(|p| {
                            ListItemEntryBuilder::new(format!(
                                "{} ({})",
                                p.name,
                                human_bytes(p.size as f64)
                            ))
                            .value(p.name.clone())
                            .build()
                            .unwrap()
                        })
                        .collect();

                    self.partition_list.items = partition_list_items;

                    self.partitions = partitions;
                    self.storage = device.dev_info.storage().await.clone();
                    self.device = Some(Arc::new(Mutex::new(device)));
                    self.device_state.set_status(DeviceStatus::Connected);
                }

                DeviceEvent::FocusPanel(panel) => {
                    self.focused_panel = panel;
                }
                DeviceEvent::Input(flag) => {
                    self.input_enabled = flag;
                }
                DeviceEvent::ShowExplorer(explorer) => {
                    self.explorer = Some(explorer);
                }
                DeviceEvent::ExplorerResult(result) => {
                    match result {
                        ExplorerResult::Cancelled | ExplorerResult::Selected(_) => {
                            log::debug!("Closing file explorer");
                            self.explorer = None;
                        }
                        _ => {
                            log::debug!("Explorer result received: {:?}", result);
                        }
                    }

                    if let Some(cb_tx) = &self.callback_tx {
                        cb_tx.send(CallbackEvent::ExplorerResult(result)).await.ok();
                    }
                }
                DeviceEvent::Error(msg) => {
                    error_dialog!(ctx, msg);
                }
                DeviceEvent::HeaderStatus(msg) => {
                    self.status_message = Some(msg);
                }
            }
        }
    }

    pub fn cancel_all_operations(&mut self) {
        for (_, handle) in self.active_operations.drain() {
            handle.abort();
        }
    }

    pub fn connect_device(&mut self, ctx: &mut AppCtx) {
        if self.device.is_some() || self.device_state.status == DeviceStatus::Connecting {
            return;
        }

        let tx = self.event_tx.clone();

        let da_data = ctx.loader().map(|da| da.file().da_raw_data.clone());
        let pl_data = ctx.preloader().map(|pl| pl.data());

        spawn(async move {
            let port = loop {
                match find_mtk_port().await {
                    Some(p) => break p,
                    None => sleep(Duration::from_millis(700)).await,
                }
            };
            let _ = tx.send(DeviceEvent::StatusChanged(DeviceStatus::Connecting)).await;

            let mut devbuilder = DeviceBuilder::default().with_mtk_port(port);

            if let Some(da) = da_data {
                devbuilder = devbuilder.with_da_data(da);
            }
            if let Some(pl) = pl_data {
                devbuilder = devbuilder.with_preloader(pl);
            }

            match devbuilder.build() {
                Ok(mut dev) => {
                    if let Err(e) = dev.init().await {
                        let _ = tx.send(DeviceEvent::Error(format!("Init failed: {}", e))).await;
                        let _ =
                            tx.send(DeviceEvent::StatusChanged(DeviceStatus::Disconnected)).await;
                        return;
                    }

                    if let Err(e) = dev.enter_da_mode().await {
                        let _ = tx.send(DeviceEvent::Error(format!("DA Mode failed: {}", e))).await;
                        let _ =
                            tx.send(DeviceEvent::StatusChanged(DeviceStatus::Disconnected)).await;
                        return;
                    }

                    let _ = tx.send(DeviceEvent::Connected(dev)).await;
                }
                Err(e) => {
                    let _ = tx.send(DeviceEvent::Error(format!("Build failed: {}", e))).await;
                    let _ = tx.send(DeviceEvent::StatusChanged(DeviceStatus::Disconnected)).await;
                }
            }
        });
    }

    /// Handles the action menu input
    async fn handle_menu_input(&mut self, ctx: &mut AppCtx, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.menu.previous(),
            KeyCode::Down => self.menu.next(),

            KeyCode::Right => {
                if self.device_state.is_connected() {
                    let _ = self
                        .event_tx
                        .send(DeviceEvent::FocusPanel(FocusedPanel::PartitionMenu))
                        .await;
                }
            }

            KeyCode::Enter => {
                if let Some(idx) = self.menu.selected_index()
                    && let Some(action) = DeviceAction::iter().nth(idx)
                {
                    if action == DeviceAction::BackToMenu {
                        ctx.change_page(AppPage::Welcome);
                        return;
                    }
                    self.execute_action(action).await;
                }
            }

            _ => {}
        }
    }

    /// Handles the partition menu input
    async fn handle_partition_input(&mut self, _ctx: &mut AppCtx, key: KeyEvent) {
        match key.code {
            KeyCode::Up => self.partition_list.previous(),
            KeyCode::Down => self.partition_list.next(),

            KeyCode::Esc => {
                self.partition_list.toggled = false;
                self.partition_list.clear_selections();
                let _ = self.event_tx.send(DeviceEvent::FocusPanel(FocusedPanel::Menu)).await;
            }

            KeyCode::Enter => {
                if let Some(cb_tx) = &self.callback_tx {
                    let selected: HashSet<&str> = self
                        .partition_list
                        .checked_items()
                        .iter()
                        .filter_map(|item| item.value.as_deref())
                        .collect();

                    let partitions: Vec<_> = self
                        .partitions
                        .iter()
                        .filter(|p| selected.contains(p.name.as_str()))
                        .cloned()
                        .collect();

                    if !partitions.is_empty() {
                        let _ = cb_tx.send(CallbackEvent::PartitionsSelected(partitions)).await;
                    }
                }
            }
            KeyCode::Char('x') => {
                self.partition_list.toggled = true;
                self.partition_list.toggle_selected();

                let cb_tx = match &self.callback_tx {
                    Some(tx) => tx,
                    None => return,
                };

                let part_item = match self.partition_list.selected_item() {
                    Some(part) => part,
                    None => return,
                };

                let value = match &part_item.value {
                    Some(value) => value,
                    None => return,
                };

                let part = match self.partitions.iter().find(|p| p.name == *value) {
                    Some(part) => part,
                    None => return,
                };

                let is_checked = part_item.is_toggled();
                cb_tx.send(CallbackEvent::PartitionToggled(part.clone(), is_checked)).await.ok();
            }
            _ => {}
        }
    }
}

#[async_trait]
impl Page for DevicePage {
    fn render(&mut self, frame: &mut Frame<'_>, ctx: &mut AppCtx) {
        let area = frame.area();

        self.render_background(frame, area, ctx);
        self.render_layout(frame, area, ctx);

        if let Some(explorer) = &mut self.explorer {
            explorer.render_modal(area, frame.buffer_mut(), &ctx.theme);
        }
    }

    async fn handle_input(&mut self, ctx: &mut AppCtx, key: KeyEvent) {
        #[cfg(target_os = "windows")]
        if key.kind != KeyEventKind::Press {
            return;
        }

        if !self.input_enabled {
            return;
        }

        // The explorer takes priority if active
        if let Some(explorer) = &mut self.explorer {
            let result = explorer.handle_key(key);
            let _ = self.event_tx.send(DeviceEvent::ExplorerResult(result)).await;
            return;
        }

        match self.focused_panel {
            FocusedPanel::Menu => self.handle_menu_input(ctx, key).await,
            FocusedPanel::PartitionMenu => self.handle_partition_input(ctx, key).await,
        }
    }

    async fn on_enter(&mut self, ctx: &mut AppCtx) {
        self.device_state.set_status(DeviceStatus::Disconnected);

        self.connect_device(ctx);
    }

    async fn on_exit(&mut self, _ctx: &mut AppCtx) {
        self.cancel_all_operations();
        // TODO: Add device shutdown if connected
    }

    async fn update(&mut self, ctx: &mut AppCtx) {
        self.process_events(ctx).await;
    }
}
