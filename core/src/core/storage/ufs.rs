/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/
use async_trait::async_trait;

use crate::core::storage::{PartitionKind, Storage, StorageType};
use crate::error::{Error, Result};

#[derive(Debug)]
pub struct UfsInfo {
    pub kind: u32,
    pub block_size: u32,
    pub lu0_size: u64,
    pub lu1_size: u64,
    pub lu2_size: u64,
    pub cid: Vec<u8>,
    pub fwver: Vec<u8>,
    pub serial: Vec<u8>,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UfsPartition {
    /// Fallback case, should not be used
    Unknown = 0,
    /// Logical Unit 0, usually preloader
    Lu0 = 1,
    /// Logical Unit 1, usually preloader backup
    Lu1 = 2,
    /// Logical Unit 2, same as USER from EMMC
    Lu2 = 3,
    /// Logical Unit 3, RPMB
    Lu3 = 4,
    Lu4 = 5,
    Lu5 = 6,
    Lu6 = 7,
    Lu7 = 8,
    /// Both Logical Unit 0 and Logical Unit 1
    Lu0Lu1 = 9,
}

pub struct UfsStorage {
    pub info: UfsInfo,
}

#[async_trait]
impl Storage for UfsStorage {
    fn kind(&self) -> StorageType {
        StorageType::Ufs
    }

    fn block_size(&self) -> u32 {
        self.info.block_size
    }

    fn total_size(&self) -> u64 {
        self.info.lu2_size
    }

    fn get_user_part(&self) -> PartitionKind {
        PartitionKind::Ufs(UfsPartition::Lu2)
    }

    fn get_pl_part1(&self) -> PartitionKind {
        PartitionKind::Ufs(UfsPartition::Lu0)
    }

    fn get_pl_part2(&self) -> PartitionKind {
        PartitionKind::Ufs(UfsPartition::Lu1)
    }
}

impl UfsStorage {
    pub fn from_response(data: &[u8]) -> Result<Self> {
        if data.len() < 0xA8 {
            return Err(Error::io("UFS response data too short"));
        }

        let mut pos = 0;

        // 0x30 == UFS
        let kind = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap());
        let block_size = u32::from_le_bytes(data[pos + 4..pos + 8].try_into().unwrap());
        pos += 8;

        let lu0_size = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let lu1_size = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;
        let lu2_size = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;

        let cid = data[pos..pos + 16].to_vec();
        pos += 16;

        let fwver = data[pos + 0x16..pos + 0x1A].to_vec();
        let serial = data[pos + 0x1E..pos + 0x2A].to_vec();

        Ok(UfsStorage {
            info: UfsInfo { kind, block_size, lu0_size, lu1_size, lu2_size, cid, fwver, serial },
        })
    }
}
