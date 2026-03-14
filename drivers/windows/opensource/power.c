/*
 * power.c
 *
 * Power management — selective suspend / idle settings via KMDF.
 * Equivalent to the original PWR_* functions (section 12).
 *
 * SPDX-License-Identifier: MIT
 */

#include "mtk_usb2ser.h"

#ifdef ALLOC_PRAGMA
#pragma alloc_text(PAGE, PowerConfigureIdleSettings)
#endif

/* =========================================================================
 *  PowerConfigureIdleSettings
 *
 *  Configures KMDF selective suspend / idle power policy.
 *  This replaces the original driver's manual idle timer + DPC + work item
 *  approach (PWR_IdleDpcRoutine, PWR_IdleRequestWorkerRoutine, etc.)
 *  with the KMDF built-in idle power management.
 *
 *  Registry keys used (from INF / disassembly):
 *    IdleEnable  (DWORD, default 0)  — enable/disable selective suspend
 *    IdleTime    (DWORD, default 10) — seconds before idle suspend
 *    IdleWWBinded (DWORD, default 1) — bind wait-wake to idle
 * ========================================================================= */
NTSTATUS
PowerConfigureIdleSettings(
    _In_ PDEVICE_CONTEXT DevCtx
    )
{
    NTSTATUS                        status;
    WDF_DEVICE_POWER_POLICY_IDLE_SETTINGS idleSettings;

    PAGED_CODE();

    if (!DevCtx->IdleEnabled) {
        /* Selective suspend is disabled — no idle configuration needed */
        return STATUS_SUCCESS;
    }

    WDF_DEVICE_POWER_POLICY_IDLE_SETTINGS_INIT(
        &idleSettings,
        IdleUsbSelectiveSuspend
        );

    idleSettings.IdleTimeout = DevCtx->IdleTimeSeconds * 1000; /* ms */
    idleSettings.UserControlOfIdleSettings = IdleAllowUserControl;
    idleSettings.Enabled = WdfTrue;

    status = WdfDeviceAssignS0IdleSettings(DevCtx->Device, &idleSettings);

    /*
     * STATUS_INVALID_DEVICE_REQUEST means the bus driver doesn't
     * support selective suspend — this is non-fatal.
     */
    if (status == STATUS_INVALID_DEVICE_REQUEST) {
        status = STATUS_SUCCESS;
    }

    return status;
}
