/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2026 Shomy
*/

use std::path::PathBuf;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use clap::Args;
use log::info;
use penumbra::Device;
use penumbra::core::storage::RpmbRegion;
use tokio::fs::File;
use tokio::io::{AsyncWriteExt, BufReader, BufWriter};

use crate::cli::MtkCommand;
use crate::cli::common::{CONN_DA, CommandMetadata};
use crate::cli::helpers::AntumbraProgress;
use crate::cli::state::PersistedDeviceState;

#[derive(Debug, Args)]
pub struct RpmbReadArgs {
    /// RPMB region to use.
    #[arg(long, default_value_t = 0)]
    pub region: u8,
    /// Starting sector to read from.
    #[arg(long, default_value_t = 0)]
    pub start_sector: u32,
    /// Number of sectors to read.
    #[arg(short, long)]
    pub num_sectors: Option<u32>,
    /// File to write the read data to.
    pub file: PathBuf,
}

#[derive(Debug, Args)]
pub struct RpmbWriteArgs {
    /// RPMB region to use.
    #[arg(long, default_value_t = 0)]
    pub region: u8,
    /// Starting sector to write to.
    #[arg(long, default_value_t = 0)]
    pub start_sector: u32,
    /// Number of sectors to write.
    #[arg(short, long)]
    pub num_sectors: Option<u32>,
    /// File to read the data from.
    pub file: PathBuf,
}

#[derive(Debug, Args)]
pub struct RpmbAuthArgs {
    /// RPMB region to use.
    #[arg(long, default_value_t = 0)]
    pub region: u8,
    /// The authentication key in hex
    pub key: String,
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct RpmbArgs {
    #[command(subcommand)]
    pub command: RpmbCommand,
}

#[derive(Debug, clap::Subcommand)]
pub enum RpmbCommand {
    /// Read from RPMB.
    Read(RpmbReadArgs),
    /// Write to RPMB.
    Write(RpmbWriteArgs),
    /// Authenticate with RPMB.
    Auth(RpmbAuthArgs),
}

impl CommandMetadata for RpmbArgs {
    fn about() -> &'static str {
        "Perform RPMB operations."
    }

    fn long_about() -> &'static str {
        "Perform RPMB operations. DA Extensions must be loaded for this command to work."
    }
}

async fn perform_rpmb_io(
    dev: &mut Device,
    region: RpmbRegion,
    start_sector: u32,
    num_sectors: Option<u32>,
    file_path: &PathBuf,
    is_read: bool,
) -> Result<()> {
    let storage = dev
        .dev_info
        .storage()
        .await
        .ok_or_else(|| anyhow!("Failed to retrieve storage information"))?;

    let rpmb_size = storage.get_rpmb_size();
    if rpmb_size == 0 {
        return Err(anyhow!("Device reports 0 RPMB size or RPMB is not supported"));
    }
    let max_sectors = (rpmb_size / 256) as u32;

    let num_sectors = num_sectors.unwrap_or_else(|| max_sectors.saturating_sub(start_sector));
    if start_sector.saturating_add(num_sectors) > max_sectors {
        return Err(anyhow!(
            "RPMB {} out of bounds! Maximum sectors available: {}",
            if is_read { "read" } else { "write" },
            max_sectors
        ));
    }

    info!(
        "{} {} sectors from RPMB starting at sector {} {} {}",
        if is_read { "Reading" } else { "Writing" },
        num_sectors,
        start_sector,
        if is_read { "into" } else { "from" },
        file_path.display()
    );

    let pb = AntumbraProgress::new(num_sectors as u64 * 256);
    let mut progress_callback = |processed: usize, total: usize| {
        pb.update(processed as u64, if is_read { "Reading RPMB..." } else { "Writing RPMB..." });
        if processed >= total {
            pb.finish(if is_read { "RPMB read completed!" } else { "RPMB write completed!" });
        }
    };

    if is_read {
        let file = File::create(file_path).await?;
        let mut writer = BufWriter::new(file);
        dev.read_rpmb(region, start_sector, num_sectors, &mut writer, &mut progress_callback)
            .await?;
        writer.flush().await?;
    } else {
        let file = File::open(file_path).await?;
        let mut reader = BufReader::new(file);
        dev.write_rpmb(region, start_sector, num_sectors, &mut reader, &mut progress_callback)
            .await?;
    }

    Ok(())
}

#[async_trait]
impl MtkCommand for RpmbArgs {
    async fn run(&self, dev: &mut Device, state: &mut PersistedDeviceState) -> Result<()> {
        dev.enter_da_mode().await?;

        state.connection_type = CONN_DA;
        state.flash_mode = 1;

        let region = match &self.command {
            RpmbCommand::Read(args) => RpmbRegion::try_from(args.region).unwrap_or(RpmbRegion::R1),
            RpmbCommand::Write(args) => RpmbRegion::try_from(args.region).unwrap_or(RpmbRegion::R1),
            RpmbCommand::Auth(args) => RpmbRegion::try_from(args.region).unwrap_or(RpmbRegion::R1),
        };

        let rpmb_size = match dev.dev_info.storage().await {
            Some(storage) => storage.get_rpmb_size(),
            None => return Err(anyhow!("Failed to retrieve storage information")),
        };

        if rpmb_size == 0 {
            return Err(anyhow!("Device reports 0 RPMB size or RPMB is not supported"));
        }

        match &self.command {
            RpmbCommand::Read(args) => {
                perform_rpmb_io(dev, region, args.start_sector, args.num_sectors, &args.file, true)
                    .await?;
            }
            RpmbCommand::Write(args) => {
                perform_rpmb_io(
                    dev,
                    region,
                    args.start_sector,
                    args.num_sectors,
                    &args.file,
                    false,
                )
                .await?;
            }
            RpmbCommand::Auth(args) => {
                info!("Authenticating RPMB using provided key...");
                let key = hex::decode(&args.key)?;
                dev.auth_rpmb(region, &key).await?;
                info!("Authentication was successful!");
            }
        }

        Ok(())
    }
}
