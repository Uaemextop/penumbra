/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use std::sync::Arc;

use log::debug;

use crate::core::storage::Storage;
use crate::core::storage::emmc::EmmcStorage;
use crate::core::storage::ufs::UfsStorage;
use crate::da::xflash::{Cmd, XFlash};

// TODO: Avoid repeated logic
pub async fn detect_storage(xflash: &mut XFlash) -> Option<Arc<dyn Storage>> {
    let emmc_response = xflash.devctrl(Cmd::GetEmmcInfo, None).await;
    let ufs_response = xflash.devctrl(Cmd::GetUfsInfo, None).await;

    if let Ok(resp) = emmc_response {
        if resp.iter().all(|&b| b == 0) == false {
            debug!("eMMC storage detected.");
            if let Ok(storage) = EmmcStorage::from_response(&resp) {
                return Some(Arc::new(storage));
            }
        }
    }

    if let Ok(resp) = ufs_response {
        if resp.iter().all(|&b| b == 0) == false {
            debug!("UFS storage detected.");
            if let Ok(storage) = UfsStorage::from_response(&resp) {
                return Some(Arc::new(storage));
            }
        }
    }

    None
}
