/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use tokio::io::{AsyncRead, AsyncWrite};

use crate::core::storage::PartitionKind;
use crate::da::protocol::BootMode;
use crate::error::{Error, Result};

use super::Device;

/// Flash and transfer operations for the [`Device`].
///
/// These methods handle reading, writing, erasing, downloading, uploading,
/// and formatting partitions or arbitrary flash offsets.
impl Device {
    /// Reads data from a specified partition on the device.
    /// This function assumes the partition to be part of the user section.
    /// To read from other sections, use `read_offset` with appropriate address.
    pub async fn read_partition(
        &mut self,
        name: &str,
        progress: &mut (dyn FnMut(usize, usize) + Send),
        writer: &mut (dyn AsyncWrite + Unpin + Send),
    ) -> Result<()> {
        self.ensure_da_mode().await?;

        let part = self
            .dev_info
            .get_partition(name)
            .await
            .ok_or_else(|| Error::penumbra(format!("Partition '{}' not found", name)))?;

        let protocol = self.protocol.as_mut().unwrap();
        protocol.read_flash(part.address, part.size, part.kind, progress, writer).await
    }

    /// Writes data to a specified partition on the device.
    /// This function assumes the partition to be part of the user section.
    /// To write to other sections, use `write_offset` with appropriate address.
    pub async fn write_partition(
        &mut self,
        name: &str,
        reader: &mut (dyn AsyncRead + Unpin + Send),
        progress: &mut (dyn FnMut(usize, usize) + Send),
    ) -> Result<()> {
        self.ensure_da_mode().await?;

        let part = self
            .dev_info
            .get_partition(name)
            .await
            .ok_or_else(|| Error::penumbra(format!("Partition '{}' not found", name)))?;

        let protocol = self.protocol.as_mut().unwrap();
        protocol.write_flash(part.address, part.size, reader, part.kind, progress).await
    }

    /// Erases a specified partition on the device.
    /// This function assumes the partition to be part of the user section.
    /// To erase other sections, use `erase_offset` with the appropriate address.
    ///
    /// # Examples
    /// ```rust
    /// use penumbra::{DeviceBuilder, find_mtk_port};
    ///
    /// let mtk_port = find_mtk_port().await.ok_or("No MTK port found")?;
    /// let da_data = std::fs::read("path/to/da/file").expect("Failed to read DA file");
    /// let mut device =
    ///     DeviceBuilder::default().with_mtk_port(mtk_port).with_da_data(da_data).build()?;
    ///
    /// device.init().await?;
    /// let mut progress = |_erased: usize, _total: usize| {};
    /// device.erase_partition("userdata", &mut progress).await?;
    /// ```
    pub async fn erase_partition(
        &mut self,
        partition: &str,
        progress: &mut (dyn FnMut(usize, usize) + Send),
    ) -> Result<()> {
        self.ensure_da_mode().await?;

        let part = self
            .dev_info
            .get_partition(partition)
            .await
            .ok_or_else(|| Error::penumbra(format!("Partition '{}' not found", partition)))?;

        let protocol = self.protocol.as_mut().unwrap();
        protocol.erase_flash(part.address, part.size, part.kind, progress).await
    }

    /// Reads data from a specified offset and size on the device.
    /// This allows reading from arbitrary locations, not limited to named partitions.
    /// To specify the section (e.g., user, pl_part1, pl_part2), provide the appropriate
    /// `PartitionKind`.
    ///
    /// # Examples
    /// ```rust
    /// // Let's assume we want to read preloader
    /// use penumbra::{DeviceBuilder, PartitionKind, find_mtk_port};
    ///
    /// let mtk_port = find_mtk_port().await.ok_or("No MTK port found")?;
    /// let mut device = DeviceBuilder::default().with_mtk_port(mtk_port).build()?;
    ///
    /// device.init().await?;
    ///
    /// let mut progress = |_read: usize, _total: usize| {};
    /// let preloader_data = device
    ///     .read_offset(0x0, 0x40000, PartitionKind::Emmc(EmmcPartition::Boot1), &mut progress)
    ///     .await?;
    /// ```
    pub async fn read_offset(
        &mut self,
        address: u64,
        size: usize,
        section: PartitionKind,
        progress: &mut (dyn FnMut(usize, usize) + Send),
        writer: &mut (dyn AsyncWrite + Unpin + Send),
    ) -> Result<()> {
        self.ensure_da_mode().await?;

        let protocol = self.protocol.as_mut().unwrap();
        protocol.read_flash(address, size, section, progress, writer).await
    }

    /// Writes data to a specified offset and size on the device.
    /// This allows writing to arbitrary locations, not limited to named partitions.
    /// To specify the section (e.g., user, pl_part1, pl_part2), provide the appropriate
    /// `PartitionKind`.
    ///
    /// # Examples
    /// ```rust
    /// // Let's assume we want to write to preloader
    /// use penumbra::{DeviceBuilder, PartitionKind, find_mtk_port};
    ///
    /// let mtk_port = find_mtk_port().await.ok_or("No MTK port found")?;
    /// let mut device = DeviceBuilder::default().with_mtk_port(mtk_port).build()?;
    ///
    /// device.init().await?;
    ///
    /// let preloader_data = std::fs::read("path/to/preloader_penangf.bin").expect("Failed to read preloader");
    /// let mut progress = |_written: usize, _total: usize| {};
    /// device
    ///     .write_offset(
    ///         0x1000, // Actual preloader offset is 0x0, but we skip the header to ensure correct writing
    ///         preloader_data.len(),
    ///         &preloader_data,
    ///         PartitionKind::Emmc(EmmcPartition::Boot1),
    ///         &mut progress,
    ///     )
    ///     .await?;
    /// ```
    pub async fn write_offset(
        &mut self,
        address: u64,
        size: usize,
        reader: &mut (dyn AsyncRead + Unpin + Send),
        section: PartitionKind,
        progress: &mut (dyn FnMut(usize, usize) + Send),
    ) -> Result<()> {
        self.ensure_da_mode().await?;

        let protocol = self.protocol.as_mut().unwrap();
        protocol.write_flash(address, size, reader, section, progress).await
    }

    /// Erases data at a specified offset and size on the device.
    /// This allows erasing arbitrary locations, not limited to named partitions.
    /// To specify the section (e.g., user, pl_part1, pl_part2), provide the appropriate
    /// `PartitionKind`.
    ///
    /// # Examples
    /// ```rust
    /// use penumbra::{DeviceBuilder, PartitionKind, find_mtk_port};
    ///
    /// let mtk_port = find_mtk_port().await.ok_or("No MTK port found")?;
    /// let da_data = std::fs::read("path/to/da/file").expect("Failed to read DA file");
    /// let mut device =
    ///     DeviceBuilder::default().with_mtk_port(mtk_port).with_da_data(da_data).build()?;
    ///
    /// device.init().await?;
    /// let mut progress = |_erased: usize, _total: usize| {};
    /// device
    ///     .erase_offset(0x0, 0x40000, PartitionKind::Emmc(EmmcPartition::Boot1), &mut progress)
    ///     .await?;
    /// ```
    pub async fn erase_offset(
        &mut self,
        address: u64,
        size: usize,
        section: PartitionKind,
        progress: &mut (dyn FnMut(usize, usize) + Send),
    ) -> Result<()> {
        self.ensure_da_mode().await?;

        let protocol = self.protocol.as_mut().unwrap();
        protocol.erase_flash(address, size, section, progress).await
    }

    /// Like `write_partition`, but instead of writing using offsets and sizes from GPT,
    /// it uses the partition name directly.
    ///
    /// This is the same method uses by SP Flash Tool when flashing firmware files.
    /// On locked bootloader, this is the only method that works for flashing stock firmware
    /// without hitting security checks, since the data is first uploaded and then verified as a
    /// whole.
    ///
    /// # Examples
    /// ```rust
    /// use penumbra::{DeviceBuilder, find_mtk_port};
    ///
    /// let mtk_port = find_mtk_port().await.ok_or("No MTK port found")?;
    /// let mut device = DeviceBuilder::default().with_mtk_port(mtk_port).build()?;
    ///
    /// device.init().await?;
    /// let firmware_data = std::fs::read("logo.bin").expect("Failed to read firmware");
    /// device.download("logo", &firmware_data).await?;
    /// ```
    pub async fn download(
        &mut self,
        partition: &str,
        size: usize,
        reader: &mut (dyn AsyncRead + Unpin + Send),
        progress: &mut (dyn FnMut(usize, usize) + Send),
    ) -> Result<()> {
        self.ensure_da_mode().await?;

        let protocol = self.protocol.as_mut().unwrap();
        protocol.download(partition.to_string(), size, reader, progress).await
    }

    /// Like `read_partition`, but instead of reading using offsets and sizes from GPT,
    /// it uses the partition name directly.
    ///
    /// This is the same method uses by SP Flash Tool when reading back without scatter.
    ///
    /// # Examples
    /// ```rust
    /// use penumbra::{DeviceBuilder, find_mtk_port};
    /// use tokio::fs::File;
    /// use tokio::io::BufWriter;
    ///
    /// let mtk_port = find_mtk_port().await.ok_or("No MTK port found")?;
    /// let da_data = std::fs::read("path/to/da/file").expect("Failed to read DA file");
    /// let mut device =
    ///     DeviceBuilder::default().with_mtk_port(mtk_port).with_da_data(da_data).build()?;
    ///
    /// device.init().await?;
    /// // Readsback "logo" partition to "logo.bin"
    /// let file = File::create("logo.bin").await?;
    /// let mut writer = BufWriter::new(file);
    /// let mut progress = |_written: usize, _total: usize| {};
    /// device.upload("logo", &mut writer, &mut progress).await?;
    /// ```
    pub async fn upload(
        &mut self,
        partition: &str,
        writer: &mut (dyn AsyncWrite + Unpin + Send),
        progress: &mut (dyn FnMut(usize, usize) + Send),
    ) -> Result<()> {
        self.ensure_da_mode().await?;

        let protocol = self.protocol.as_mut().unwrap();
        protocol.upload(partition.to_string(), writer, progress).await
    }

    /// Formats a specified partition on the device
    ///
    /// # Examples
    /// ```rust
    /// use penumbra::{DeviceBuilder, find_mtk_port};
    ///
    /// let mtk_port = find_mtk_port().await.ok_or("No MTK port found")?;
    /// let da_data = std::fs::read("path/to/da/file").expect("Failed to read DA file");
    /// let mut device =
    ///     DeviceBuilder::default().with_mtk_port(mtk_port).with_da_data(da_data).build()?;
    ///
    /// device.init().await?;
    /// let mut progress = |_erased: usize, _total: usize| {};
    /// device.format("userdata", &mut progress).await?;
    /// ```
    pub async fn format(
        &mut self,
        partition: &str,
        progress: &mut (dyn FnMut(usize, usize) + Send),
    ) -> Result<()> {
        self.ensure_da_mode().await?;

        let protocol = self.protocol.as_mut().unwrap();
        protocol.format(partition.to_string(), progress).await
    }

    /// Shuts down the device
    ///
    /// # Examples
    /// ```rust
    /// use penumbra::{DeviceBuilder, find_mtk_port};
    ///
    /// let mtk_port = find_mtk_port().await.ok_or("No MTK port found")?;
    /// let da_data = std::fs::read("path/to/da/file").expect("Failed to read DA file");
    /// let mut device =
    ///     DeviceBuilder::default().with_mtk_port(mtk_port).with_da_data(da_data).build()?;
    ///
    /// device.init().await?;
    /// device.shutdown().await?;
    /// ```
    pub async fn shutdown(&mut self) -> Result<()> {
        self.ensure_da_mode().await?;

        let protocol = self.protocol.as_mut().unwrap();
        protocol.shutdown().await
    }

    /// Reboots the device into the specified boot mode.
    /// Supported boot modes include `Normal`, `HomeScreen`, `Fastboot`, `Test`, and `Meta`.
    ///
    /// # Examples
    /// ```rust
    /// use penumbra::{BootMode, DeviceBuilder, find_mtk_port};
    ///
    /// let mtk_port = find_mtk_port().await.ok_or("No MTK port found")?;
    /// let da_data = std::fs::read("path/to/da/file").expect("Failed to read DA file");
    /// let mut device =
    ///     DeviceBuilder::default().with_mtk_port(mtk_port).with_da_data(da_data).build()?;
    ///
    /// device.init().await?;
    /// device.reboot(BootMode::Normal).await?;
    /// ```
    pub async fn reboot(&mut self, bootmode: BootMode) -> Result<()> {
        self.ensure_da_mode().await?;

        let protocol = self.protocol.as_mut().unwrap();
        protocol.reboot(bootmode).await
    }
}
