/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use penumbra::Device;
use penumbra::core::seccfg::LockFlag;
use penumbra::core::storage::Partition;
use tokio::fs::File;
use tokio::io::{BufReader, BufWriter};
use tokio::spawn;
use tokio::sync::{Mutex, mpsc};

use crate::components::{ExplorerResult, FileExplorer};

use super::events::{CallbackEvent, DeviceActionCallback, DeviceEvent, DeviceStatus, FocusedPanel};

pub struct UnlockBootloaderCallback;
#[async_trait]
impl DeviceActionCallback for UnlockBootloaderCallback {
    async fn execute(
        &self,
        device: Arc<Mutex<Device>>,
        event_tx: mpsc::Sender<DeviceEvent>,
        _cb_tx: mpsc::Sender<CallbackEvent>,
        _cb_rx: mpsc::Receiver<CallbackEvent>,
    ) -> Result<()> {
        let _ = event_tx.send(DeviceEvent::HeaderStatus("Unlocking bootloader...".into())).await;

        let mut dev = device.lock().await;
        match dev.set_seccfg_lock_state(LockFlag::Unlock).await {
            Some(_) => {
                let _ =
                    event_tx.send(DeviceEvent::HeaderStatus("Bootloader unlocked.".into())).await;
                Ok(())
            }
            None => Err(anyhow!("Failed to unlock bootloader")),
        }
    }
}

pub struct LockBootloaderCallback;
#[async_trait]
impl DeviceActionCallback for LockBootloaderCallback {
    async fn execute(
        &self,
        device: Arc<Mutex<Device>>,
        event_tx: mpsc::Sender<DeviceEvent>,
        _cb_tx: mpsc::Sender<CallbackEvent>,
        _cb_rx: mpsc::Receiver<CallbackEvent>,
    ) -> Result<()> {
        event_tx.send(DeviceEvent::HeaderStatus("Locking bootloader...".into())).await.ok();

        let mut dev = device.lock().await;
        match dev.set_seccfg_lock_state(LockFlag::Unlock).await {
            Some(_) => {
                event_tx.send(DeviceEvent::HeaderStatus("Bootloader locked.".into())).await.ok();
                Ok(())
            }
            None => Err(anyhow!("Failed to lock bootloader")),
        }
    }
}

pub struct ReadPartitionCallback;
#[async_trait]
impl DeviceActionCallback for ReadPartitionCallback {
    async fn execute(
        &self,
        device: Arc<Mutex<Device>>,
        event_tx: mpsc::Sender<DeviceEvent>,
        _cb_tx: mpsc::Sender<CallbackEvent>,
        mut cb_rx: mpsc::Receiver<CallbackEvent>,
    ) -> Result<()> {
        let _ = event_tx.send(DeviceEvent::FocusPanel(FocusedPanel::PartitionMenu)).await;

        let explorer = FileExplorer::new("Output dump directory")?.directories_only();

        let partitions = loop {
            match cb_rx.recv().await {
                Some(CallbackEvent::PartitionsSelected(parts)) => break parts,
                Some(CallbackEvent::ExplorerResult(ExplorerResult::Cancelled)) => {
                    return Ok(());
                }
                _ => {}
            }
        };

        let _ = event_tx.send(DeviceEvent::ShowExplorer(explorer)).await;

        let output_dir = loop {
            match cb_rx.recv().await {
                Some(CallbackEvent::ExplorerResult(ExplorerResult::Selected(path))) => {
                    break path;
                }
                Some(CallbackEvent::ExplorerResult(ExplorerResult::Cancelled)) => {
                    return Ok(());
                }
                _ => {}
            }
        };

        let total_size = partitions.iter().map(|p| p.size as u64).sum::<u64>();

        let mut bytes_read: u64 = 0;

        let mut dev = device.lock().await;
        // Block page input to avoid interruptions
        event_tx.send(DeviceEvent::Input(false)).await.ok();

        event_tx
            .send(DeviceEvent::ProgressStart {
                total_bytes: total_size,
                message: "Reading partitions...".into(),
            })
            .await
            .ok();
        for partition in partitions {
            let output_path = output_dir.join(format!("{}.bin", partition.name));
            let file = File::create(&output_path).await?;
            let mut writer = BufWriter::new(file);

            let mut progress_cb = |written: usize, _total_partition_bytes: usize| {
                let total_bytes = bytes_read + written as u64;

                let event_tx = event_tx.clone();
                let part_name = partition.name.clone();
                spawn(async move {
                    let _ = event_tx
                        .send(DeviceEvent::ProgressUpdate {
                            written: total_bytes,
                            message: Some(format!("Reading partition '{}'...", part_name,)),
                        })
                        .await;
                });
            };

            dev.upload(&partition.name, &mut writer, &mut progress_cb).await?;

            bytes_read += partition.size as u64;
        }

        let _ = event_tx
            .send(DeviceEvent::ProgressFinish { message: "Partition read complete.".into() })
            .await;

        // Focus back the menu panel to avoid confusion
        let _ = event_tx.send(DeviceEvent::FocusPanel(FocusedPanel::Menu)).await;
        event_tx.send(DeviceEvent::Input(true)).await.ok();

        Ok(())
    }
}

pub struct WritePartitionCallback;
#[async_trait]
impl DeviceActionCallback for WritePartitionCallback {
    async fn execute(
        &self,
        device: Arc<Mutex<Device>>,
        event_tx: mpsc::Sender<DeviceEvent>,
        _cb_tx: mpsc::Sender<CallbackEvent>,
        mut cb_rx: mpsc::Receiver<CallbackEvent>,
    ) -> Result<()> {
        event_tx.send(DeviceEvent::FocusPanel(FocusedPanel::PartitionMenu)).await.ok();

        let mut partition_map: HashMap<String, PathBuf> = HashMap::new();

        let partitions: Vec<Partition>;

        loop {
            match cb_rx.recv().await {
                Some(CallbackEvent::PartitionToggled(partition, selected)) => {
                    if selected {
                        // Show file explorer to select partition file
                        let explorer = FileExplorer::new(format!(
                            "Select file for partition '{}'",
                            partition.name
                        ))?;

                        event_tx.send(DeviceEvent::ShowExplorer(explorer)).await.ok();

                        let path = loop {
                            match cb_rx.recv().await {
                                Some(CallbackEvent::ExplorerResult(ExplorerResult::Selected(
                                    path,
                                ))) => break path,
                                Some(CallbackEvent::ExplorerResult(ExplorerResult::Cancelled)) => {
                                    continue;
                                }
                                _ => {}
                            }
                        };

                        partition_map.insert(partition.name.clone(), path);
                    } else {
                        partition_map.remove(&partition.name);
                    }
                }
                Some(CallbackEvent::ExplorerResult(ExplorerResult::Cancelled)) => {
                    continue;
                }
                Some(CallbackEvent::PartitionsSelected(parts)) => {
                    partitions = parts;
                    break;
                }
                _ => {}
            }
        }

        let part_to_write: Vec<(Partition, PathBuf)> = partitions
            .into_iter()
            .filter_map(|p| partition_map.get(&p.name).cloned().map(|path| (p, path)))
            .collect();

        let total_size = part_to_write.iter().map(|(p, _)| p.size as u64).sum::<u64>();

        let mut bytes_written: u64 = 0;

        let mut dev = device.lock().await;
        // Block page input to avoid interruptions
        event_tx.send(DeviceEvent::Input(false)).await.ok();

        event_tx
            .send(DeviceEvent::ProgressStart {
                total_bytes: total_size,
                message: "Writing partitions...".into(),
            })
            .await
            .ok();

        for (partition, path) in part_to_write {
            let file = File::open(path).await?;
            let mut reader = BufReader::new(file);

            let mut progress_cb = |written: usize, _total_partition_bytes: usize| {
                let total_bytes = bytes_written + written as u64;

                let event_tx = event_tx.clone();
                let part_name = partition.name.clone();
                spawn(async move {
                    let _ = event_tx
                        .send(DeviceEvent::ProgressUpdate {
                            written: total_bytes,
                            message: Some(format!("Flashing partition '{}'...", part_name,)),
                        })
                        .await;
                });
            };

            dev.download(&partition.name, partition.size, &mut reader, &mut progress_cb).await?;

            bytes_written += partition.size as u64;
        }

        let _ = event_tx
            .send(DeviceEvent::ProgressFinish { message: "Partition write complete.".into() })
            .await;

        // Focus back the menu panel to avoid confusion
        let _ = event_tx.send(DeviceEvent::FocusPanel(FocusedPanel::Menu)).await;
        event_tx.send(DeviceEvent::Input(true)).await.ok();

        Ok(())
    }
}
