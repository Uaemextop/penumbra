/*
    SPDX-License-Identifier: AGPL-3.0-or-later
    SPDX-FileCopyrightText: 2025 Shomy
*/

const EXT_LOADER: &[u8] = include_bytes!("../../../payloads/extloader_v5.bin");

use log::{info, warn};

use crate::da::xflash::XFlash;
use crate::da::{DA, DAEntryRegion};
use crate::error::Result;
use crate::utilities::analysis::{ArchAnalyzer, Thumb2Analyzer};
use crate::utilities::arm::*;
use crate::utilities::hash::hash;
use crate::utilities::patching::*;

/// Patches both DA1 and DA2, specific for V5 DA
pub fn patch_da(xflash: &mut XFlash) -> Result<DA> {
    let da2 = patch_da2(xflash)?;
    let mut da1 = patch_da1(xflash)?;

    let hash_pos = xflash.da.find_da_hash_offset();
    match hash_pos {
        Some(pos) => {
            let hash_type = xflash.da.get_hash_type();
            let hash_result =
                hash(hash_type, &da2.data[..da2.data.len().saturating_sub(da2.sig_len as usize)]);
            patch(&mut da1.data, pos, &bytes_to_hex(&hash_result))?;

            let original_da = &xflash.da;
            let da = DA {
                da_type: xflash.da.da_type.clone(),
                regions: vec![original_da.regions[0].clone(), da1.clone(), da2.clone()],
                magic: original_da.magic,
                hw_code: original_da.hw_code,
                hw_sub_code: original_da.hw_sub_code,
            };
            Ok(da)
        }
        None => {
            info!("[Penumbra] Could not find DA1 hash position, skipping patching");
            Ok(xflash.da.clone())
        }
    }
}

/// Patches only DA1, specific for V5 DA
pub fn patch_da1(xflash: &mut XFlash) -> Result<DAEntryRegion> {
    let da1 = xflash.da.get_da1().cloned().unwrap();
    Ok(da1)
}

/// Patches only DA2, specific for V5 DA
pub fn patch_da2(xflash: &mut XFlash) -> Result<DAEntryRegion> {
    let mut da2 = xflash.da.get_da2().cloned().unwrap();

    let analyzer = Thumb2Analyzer::new(da2.data.clone(), da2.addr as u64);

    patch_security(&mut da2, &analyzer)?;
    patch_boot_to(&mut da2)?;

    Ok(da2)
}

fn patch_security(da: &mut DAEntryRegion, analyzer: &Thumb2Analyzer) -> Result<bool> {
    patch_lock_state(da, analyzer)?;
    patch_sec_policy(da, analyzer)?;
    patch_da_sla(da, analyzer)
}

fn patch_lock_state(da: &mut DAEntryRegion, analyzer: &Thumb2Analyzer) -> Result<bool> {
    #[rustfmt::skip]
    let lks_patch = vec![
            0x00, 0x23, // movs r3, #0
            0x03, 0x60, // str r3, [r0, #0]
            0x00, 0x20, // movs r0, #0
            0x10, 0xBD, // pop {r4, pc}
        ];

    let Some(off) = analyzer.find_function_from_string("[SEC_POLICY] lock_state = 0x") else {
        warn!("Could not patch lock state!");
        return Ok(false);
    };

    let sboot_state_bl = analyzer.get_next_bl_from_off(off).unwrap_or(0);

    let seccfg_bl = analyzer.get_next_bl_from_off(sboot_state_bl + 4).unwrap_or(0);
    let get_lock_state = analyzer.get_bl_target(seccfg_bl).unwrap_or(0);
    let get_lock_state_off = analyzer.va_to_offset(get_lock_state).unwrap_or(0);

    if get_lock_state_off == 0 {
        warn!("Could not find lock state function to patch!");
        return Ok(false);
    }

    patch(&mut da.data, get_lock_state_off, &bytes_to_hex(&lks_patch))?;
    info!("Patched DA2 to always report unlocked state.");
    Ok(true)
}

fn patch_sec_policy(da: &mut DAEntryRegion, analyzer: &Thumb2Analyzer) -> Result<bool> {
    const POLICY_FUNC: &str = "==========security policy==========";

    let Some(part_sec_pol_off) = analyzer.find_function_from_string(POLICY_FUNC) else {
        warn!("Could not find security policy function!");
        return Ok(false);
    };

    // BL policy_index
    // BL hash_binding
    // BL verify_policy
    // BL download_policy
    let Some(policy_idx_bl) = analyzer.get_next_bl_from_off(part_sec_pol_off) else {
        warn!("Could not find policy_idx call");
        return Ok(false);
    };
    let Some(hash_binding_bl) = analyzer.get_next_bl_from_off(policy_idx_bl + 4) else {
        warn!("Could not find hash_binding call");
        return Ok(false);
    };
    let Some(verify_bl) = analyzer.get_next_bl_from_off(hash_binding_bl + 4) else {
        warn!("Could not find verify_policy call");
        return Ok(false);
    };
    let Some(download_bl) = analyzer.get_next_bl_from_off(verify_bl + 4) else {
        warn!("Could not find download_policy call");
        return Ok(false);
    };

    let targets =
        [(hash_binding_bl, "Hash Binding"), (verify_bl, "Verification"), (download_bl, "Download")];

    let mut patched_any = false;

    for (bl_offset, desc) in targets {
        if let Some(func_offset) = analyzer.get_bl_target_offset(bl_offset) {
            force_return(&mut da.data, func_offset, 0, true)?;
            info!("Patched DA2 to skip security policy ({desc})");
            patched_any = true;
        } else {
            warn!("Failed to resolve target for {desc}");
        }
    }

    if !patched_any {
        warn!("Could not patch security policy!");
    }

    Ok(patched_any)
}

/// Adds back the boot_to command to da2, allowing to load extensions.
/// This is needed only on DAs which build date is >= late 2023
fn patch_boot_to(da: &mut DAEntryRegion) -> Result<bool> {
    // We only need to patch if the DA doesn't support this cmd.
    if find_pattern(&da.data, "636D645F626F6F745F746F00", 0) != HEX_NOT_FOUND {
        return Ok(false);
    }

    let dagent_reg_cmds = find_pattern(&da.data, "08B54FF460200021XXF7", 0);
    let devc_read_reg = find_pattern(&da.data, "30B5002385B004460193", 0);
    let unsupported_cmd = find_pattern(&da.data, "084B13B504460193", 0);
    let register_maj_cmd = find_pattern(&da.data, "38B5054610200C46", 0);

    // Patch the devc_read_reg to be our new cmd
    patch(&mut da.data, devc_read_reg, &bytes_to_hex(EXT_LOADER))?;

    // Find the LDR of unsupported cmd and patch it with devc_read_reg address (thumb addr)
    let unsupported_cmd_addr = to_thumb_addr(unsupported_cmd, da.addr).to_le_bytes();
    let devc_read_reg_addr = to_thumb_addr(devc_read_reg, da.addr).to_le_bytes();

    // Patch the DAT to point to the new injected cmd
    let unsupported_cmd_dat = find_pattern(&da.data, &bytes_to_hex(&unsupported_cmd_addr), 0);
    patch(&mut da.data, unsupported_cmd_dat, &bytes_to_hex(&devc_read_reg_addr))?;

    let mut reg_cmd_patch = Vec::with_capacity(20);

    #[rustfmt::skip]
    reg_cmd_patch.extend_from_slice(&[
        0x4F, 0xF4, 0x80, 0x30, // mov.w r0, #0x10000
        0x08, 0x30,             // adds r0, #0x8
    ]);

    let ldr_off = dagent_reg_cmds + 0x2 + reg_cmd_patch.len();
    let ldr = encode_ldr(1, ldr_off, unsupported_cmd_dat, da.addr)?;

    // da_agent addr + skip push + length of the patch
    let bl_addr = ldr_off as u32 + 0x2 + da.addr;
    let reg_maj_cmd_addr = to_thumb_addr(register_maj_cmd, da.addr);
    let shellcode = encode_bl(bl_addr, reg_maj_cmd_addr);

    reg_cmd_patch.extend_from_slice(&ldr);
    reg_cmd_patch.extend_from_slice(&shellcode);
    reg_cmd_patch.extend_from_slice(&[0xAF, 0xF3, 0x00, 0x80]); // nop.w
    reg_cmd_patch.extend_from_slice(&[0xAF, 0xF3, 0x00, 0x80]); // nop.w

    patch(&mut da.data, dagent_reg_cmds + 0x2, &bytes_to_hex(&reg_cmd_patch))?;

    info!("[Penumbra] Patched DA2 to add cmd_boot_to");

    Ok(true)
}

fn patch_da_sla(da: &mut DAEntryRegion, analyzer: &Thumb2Analyzer) -> Result<bool> {
    let Some(devc_sla_status) = analyzer.find_string_xref("devc_get_sla_enabled_status") else {
        // If the DA doesn't have this string, it likely doesn't have SLA to begin with
        return Ok(true);
    };

    // dprintf
    let Some(first_bl) = analyzer.get_next_bl_from_off(devc_sla_status) else {
        warn!("Could not patch DA SLA!");
        return Ok(false);
    };

    let Some(off) = analyzer.get_next_bl_from_off(first_bl + 4) else {
        warn!("Could not patch DA SLA!");
        return Ok(false);
    };

    let target = analyzer.get_bl_target(off).unwrap_or(0);

    let target_off = analyzer.va_to_offset(target).unwrap_or(0);

    if target_off != 0 {
        force_return(&mut da.data, target_off, 0, true)?;
        info!("Patched DA2 SLA to be disabled.");
    } else {
        warn!("Could not patch DA SLA!");
    }

    Ok(true)
}
