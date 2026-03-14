/*
 * driver.c
 *
 * DriverEntry and EvtDriverDeviceAdd for the MediaTek USB CDC ACM
 * serial port KMDF driver.
 *
 * SPDX-License-Identifier: MIT
 */

#include "mtk_usb2ser.h"

#ifdef ALLOC_PRAGMA
#pragma alloc_text(INIT, DriverEntry)
#pragma alloc_text(PAGE, EvtDriverDeviceAdd)
#pragma alloc_text(PAGE, EvtDriverCleanup)
#endif

/* Global device instance counter for unique device names */
LONG g_DeviceCount = 0;

/* =========================================================================
 *  DriverEntry — KMDF driver initialisation
 *  Equivalent to the original WDM DriverEntry at RVA 0x21980 (x64)
 * ========================================================================= */
NTSTATUS
DriverEntry(
    _In_ PDRIVER_OBJECT  DriverObject,
    _In_ PUNICODE_STRING RegistryPath
    )
{
    NTSTATUS            status;
    WDF_DRIVER_CONFIG   driverConfig;
    WDF_OBJECT_ATTRIBUTES driverAttrs;

    WDF_DRIVER_CONFIG_INIT(&driverConfig, EvtDriverDeviceAdd);

    WDF_OBJECT_ATTRIBUTES_INIT(&driverAttrs);
    driverAttrs.EvtCleanupCallback = EvtDriverCleanup;

    status = WdfDriverCreate(
        DriverObject,
        RegistryPath,
        &driverAttrs,
        &driverConfig,
        WDF_NO_HANDLE
        );

    return status;
}

/* =========================================================================
 *  EvtDriverCleanup — called when the driver object is being deleted
 * ========================================================================= */
VOID
EvtDriverCleanup(
    _In_ WDFOBJECT DriverObject
    )
{
    PAGED_CODE();
    UNREFERENCED_PARAMETER(DriverObject);
}

/* =========================================================================
 *  EvtDriverDeviceAdd — Create and initialise the KMDF device object
 *  Equivalent to the original WDM AddDevice at section 4
 * ========================================================================= */
NTSTATUS
EvtDriverDeviceAdd(
    _In_    WDFDRIVER       Driver,
    _Inout_ PWDFDEVICE_INIT DeviceInit
    )
{
    NTSTATUS                        status;
    WDFDEVICE                       device;
    PDEVICE_CONTEXT                 devCtx;
    WDF_OBJECT_ATTRIBUTES           deviceAttrs;
    WDF_PNPPOWER_EVENT_CALLBACKS    pnpCallbacks;
    WDF_FILEOBJECT_CONFIG           fileConfig;
    WDF_OBJECT_ATTRIBUTES           fileAttrs;
    DECLARE_UNICODE_STRING_SIZE(deviceName, 64);
    LONG                            portIndex;

    PAGED_CODE();
    UNREFERENCED_PARAMETER(Driver);

    /* Set device type to serial port (FILE_DEVICE_SERIAL_PORT) */
    WdfDeviceInitSetDeviceType(DeviceInit, FILE_DEVICE_SERIAL_PORT);
    WdfDeviceInitSetIoType(DeviceInit, WdfDeviceIoBuffered);
    WdfDeviceInitSetExclusive(DeviceInit, FALSE);

    /* Assign a unique device name: \\Device\\mtkcdcacm<N> */
    portIndex = InterlockedIncrement(&g_DeviceCount) - 1;
    status = RtlUnicodeStringPrintf(&deviceName, L"\\Device\\mtkcdcacm%d", portIndex);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    status = WdfDeviceInitAssignName(DeviceInit, &deviceName);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    /* Configure PnP/Power callbacks */
    WDF_PNPPOWER_EVENT_CALLBACKS_INIT(&pnpCallbacks);
    pnpCallbacks.EvtDevicePrepareHardware = EvtDevicePrepareHardware;
    pnpCallbacks.EvtDeviceReleaseHardware = EvtDeviceReleaseHardware;
    pnpCallbacks.EvtDeviceD0Entry         = EvtDeviceD0Entry;
    pnpCallbacks.EvtDeviceD0Exit          = EvtDeviceD0Exit;
    WdfDeviceInitSetPnpPowerEventCallbacks(DeviceInit, &pnpCallbacks);

    /* Configure File Object callbacks (Create/Close/Cleanup) */
    WDF_FILEOBJECT_CONFIG_INIT(&fileConfig,
                               EvtDeviceFileCreate,
                               EvtFileClose,
                               EvtFileCleanup);
    WDF_OBJECT_ATTRIBUTES_INIT(&fileAttrs);
    WdfDeviceInitSetFileObjectConfig(DeviceInit, &fileConfig, &fileAttrs);

    /* Create the KMDF device */
    WDF_OBJECT_ATTRIBUTES_INIT_CONTEXT_TYPE(&deviceAttrs, DEVICE_CONTEXT);
    status = WdfDeviceCreate(&DeviceInit, &deviceAttrs, &device);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    devCtx = GetDeviceContext(device);
    RtlZeroMemory(devCtx, sizeof(DEVICE_CONTEXT));
    devCtx->Device    = device;
    devCtx->PortIndex = portIndex;

    /* Store the device name for symbolic link creation */
    devCtx->DeviceName.Buffer        = devCtx->DeviceNameBuf;
    devCtx->DeviceName.MaximumLength = sizeof(devCtx->DeviceNameBuf);
    devCtx->DeviceName.Length        = deviceName.Length;
    RtlCopyMemory(devCtx->DeviceNameBuf, deviceName.Buffer, deviceName.Length);
    devCtx->DeviceNameBuf[deviceName.Length / sizeof(WCHAR)] = L'\0';

    /* Initialise serial port defaults (115200, 8-N-1) */
    devCtx->LineCoding.dwDTERate    = DEFAULT_BAUD_RATE;
    devCtx->LineCoding.bCharFormat  = 0;    /* 1 stop bit */
    devCtx->LineCoding.bParityType  = 0;    /* No parity */
    devCtx->LineCoding.bDataBits    = DEFAULT_DATA_BITS;

    devCtx->InQueueSize  = DEFAULT_IN_QUEUE_SIZE;
    devCtx->OutQueueSize = DEFAULT_OUT_QUEUE_SIZE;

    /* Default timeout: return immediately if no data */
    devCtx->Timeouts.ReadIntervalTimeout         = MAXULONG;
    devCtx->Timeouts.ReadTotalTimeoutMultiplier   = 0;
    devCtx->Timeouts.ReadTotalTimeoutConstant     = 0;
    devCtx->Timeouts.WriteTotalTimeoutMultiplier  = 0;
    devCtx->Timeouts.WriteTotalTimeoutConstant    = 0;

    /* Default idle settings */
    devCtx->IdleTimeSeconds = DEFAULT_IDLE_TIME_SEC;
    devCtx->IdleEnabled     = FALSE;
    devCtx->IdleWWBound     = TRUE;

    /* Create spin locks */
    status = WdfSpinLockCreate(WDF_NO_OBJECT_ATTRIBUTES, &devCtx->EventLock);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    status = WdfSpinLockCreate(WDF_NO_OBJECT_ATTRIBUTES, &devCtx->WriteLock);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    status = WdfSpinLockCreate(WDF_NO_OBJECT_ATTRIBUTES, &devCtx->ReadBuffer.Lock);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    /* Create I/O queues */
    status = QueueInitialize(device);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    /* Register device interface for COM port */
    status = WdfDeviceCreateDeviceInterface(
        device,
        &GUID_MTK_DEVINTERFACE_COMPORT,
        NULL
        );
    if (!NT_SUCCESS(status)) {
        return status;
    }

    /* Register WMI */
    status = WmiRegistration(device);
    if (!NT_SUCCESS(status)) {
        /* WMI failure is non-fatal */
    }

    return STATUS_SUCCESS;
}
