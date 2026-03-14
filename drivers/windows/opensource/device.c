/*
 * device.c
 *
 * PnP/Power lifecycle, USB configuration, pipe setup, symbolic link
 * management, and file object (Create/Close/Cleanup) handlers.
 *
 * SPDX-License-Identifier: MIT
 */

#include "mtk_usb2ser.h"

#ifdef ALLOC_PRAGMA
#pragma alloc_text(PAGE, EvtDevicePrepareHardware)
#pragma alloc_text(PAGE, EvtDeviceReleaseHardware)
#pragma alloc_text(PAGE, EvtDeviceD0Entry)
#pragma alloc_text(PAGE, EvtDeviceD0Exit)
#pragma alloc_text(PAGE, EvtDeviceFileCreate)
#pragma alloc_text(PAGE, EvtFileClose)
#pragma alloc_text(PAGE, EvtFileCleanup)
#pragma alloc_text(PAGE, DeviceConfigureUsbDevice)
#pragma alloc_text(PAGE, DeviceConfigureUsbPipes)
#pragma alloc_text(PAGE, DeviceCreateSymbolicLink)
#pragma alloc_text(PAGE, DeviceRemoveSymbolicLink)
#pragma alloc_text(PAGE, DeviceReadRegistrySettings)
#endif

/* =========================================================================
 *  EvtDevicePrepareHardware
 *  Equivalent to the original PnP_StartDevice (section 5)
 * ========================================================================= */
NTSTATUS
EvtDevicePrepareHardware(
    _In_ WDFDEVICE    Device,
    _In_ WDFCMRESLIST ResourcesRaw,
    _In_ WDFCMRESLIST ResourcesTranslated
    )
{
    NTSTATUS        status;
    PDEVICE_CONTEXT devCtx = GetDeviceContext(Device);

    PAGED_CODE();
    UNREFERENCED_PARAMETER(ResourcesRaw);
    UNREFERENCED_PARAMETER(ResourcesTranslated);

    /* Create and configure the USB device object */
    status = DeviceConfigureUsbDevice(devCtx);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    /* Enumerate and configure USB pipes (Bulk IN, Bulk OUT, Interrupt IN) */
    status = DeviceConfigureUsbPipes(devCtx);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    /* Initialise the ring buffer for incoming serial data */
    status = RingBufferInit(&devCtx->ReadBuffer, DEFAULT_READ_BUFFER_SIZE);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    /* Read registry settings (IdleTime, IdleEnable, PortName, etc.) */
    DeviceReadRegistrySettings(devCtx);

    /* Create COM port symbolic link and SERIALCOMM entry */
    status = DeviceCreateSymbolicLink(devCtx);
    if (!NT_SUCCESS(status)) {
        /* Non-fatal: device can still work via device interface */
    }

    /* Configure selective suspend / idle power policy */
    PowerConfigureIdleSettings(devCtx);

    devCtx->DeviceStarted = TRUE;

    return STATUS_SUCCESS;
}

/* =========================================================================
 *  EvtDeviceReleaseHardware
 *  Equivalent to the original PnP_RemoveDevice (section 5)
 * ========================================================================= */
NTSTATUS
EvtDeviceReleaseHardware(
    _In_ WDFDEVICE    Device,
    _In_ WDFCMRESLIST ResourcesTranslated
    )
{
    PDEVICE_CONTEXT devCtx = GetDeviceContext(Device);

    PAGED_CODE();
    UNREFERENCED_PARAMETER(ResourcesTranslated);

    devCtx->DeviceStarted = FALSE;

    /* Remove symbolic link and SERIALCOMM entry */
    DeviceRemoveSymbolicLink(devCtx);

    /* Free ring buffer */
    RingBufferFree(&devCtx->ReadBuffer);

    return STATUS_SUCCESS;
}

/* =========================================================================
 *  EvtDeviceD0Entry — device enters working state
 * ========================================================================= */
NTSTATUS
EvtDeviceD0Entry(
    _In_ WDFDEVICE              Device,
    _In_ WDF_POWER_DEVICE_STATE PreviousState
    )
{
    PAGED_CODE();
    UNREFERENCED_PARAMETER(Device);
    UNREFERENCED_PARAMETER(PreviousState);

    /*
     * The WDF continuous readers (bulk IN and interrupt) are automatically
     * started by the framework when the device enters D0.
     */

    return STATUS_SUCCESS;
}

/* =========================================================================
 *  EvtDeviceD0Exit — device leaves working state
 * ========================================================================= */
NTSTATUS
EvtDeviceD0Exit(
    _In_ WDFDEVICE              Device,
    _In_ WDF_POWER_DEVICE_STATE TargetState
    )
{
    PAGED_CODE();
    UNREFERENCED_PARAMETER(Device);
    UNREFERENCED_PARAMETER(TargetState);

    /*
     * The WDF continuous readers are automatically stopped by the
     * framework when the device leaves D0.
     */

    return STATUS_SUCCESS;
}

/* =========================================================================
 *  EvtDeviceFileCreate — handle opened on our device
 *  Equivalent to original FileOp_Create (section 6)
 * ========================================================================= */
VOID
EvtDeviceFileCreate(
    _In_ WDFDEVICE  Device,
    _In_ WDFREQUEST Request,
    _In_ WDFFILEOBJECT FileObject
    )
{
    PDEVICE_CONTEXT devCtx = GetDeviceContext(Device);
    NTSTATUS        status;

    PAGED_CODE();
    UNREFERENCED_PARAMETER(FileObject);

    if (!devCtx->DeviceStarted) {
        WdfRequestComplete(Request, STATUS_DEVICE_NOT_READY);
        return;
    }

    /* Purge any stale data in the ring buffer */
    RingBufferPurge(&devCtx->ReadBuffer);

    /* Reset performance statistics */
    RtlZeroMemory(&devCtx->PerfStats, sizeof(SERIALPERF_STATS));

    /* Set DTR and RTS (via CDC SET_CONTROL_LINE_STATE) */
    UsbControlSetDtr(devCtx);
    UsbControlSetRts(devCtx);

    /* Apply current line coding (baud, data bits, parity, stop bits) */
    status = UsbControlSetLineCoding(devCtx);
    /* Non-fatal if this fails — some devices may not implement it */

    UNREFERENCED_PARAMETER(status);

    devCtx->DeviceOpen = TRUE;

    WdfRequestComplete(Request, STATUS_SUCCESS);
}

/* =========================================================================
 *  EvtFileClose — last handle closed
 *  Equivalent to original FileOp_Close (section 6)
 * ========================================================================= */
VOID
EvtFileClose(
    _In_ WDFFILEOBJECT FileObject
    )
{
    WDFDEVICE       device = WdfFileObjectGetDevice(FileObject);
    PDEVICE_CONTEXT devCtx = GetDeviceContext(device);

    PAGED_CODE();

    devCtx->DeviceOpen = FALSE;

    /* Clear DTR and RTS */
    UsbControlClrDtr(devCtx);
    UsbControlClrRts(devCtx);
}

/* =========================================================================
 *  EvtFileCleanup — handle being cleaned up (cancel pending I/O)
 * ========================================================================= */
VOID
EvtFileCleanup(
    _In_ WDFFILEOBJECT FileObject
    )
{
    WDFDEVICE       device = WdfFileObjectGetDevice(FileObject);
    PDEVICE_CONTEXT devCtx = GetDeviceContext(device);

    PAGED_CODE();

    /* Drain and purge all manual queues */
    WdfIoQueuePurgeSynchronously(devCtx->PendingReadQueue);
    WdfIoQueuePurgeSynchronously(devCtx->PendingWaitMaskQueue);

    /* Restart the queues so they are ready for the next open */
    WdfIoQueueStart(devCtx->PendingReadQueue);
    WdfIoQueueStart(devCtx->PendingWaitMaskQueue);

    /* Purge ring buffer */
    RingBufferPurge(&devCtx->ReadBuffer);

    /* Clear event history and wait mask */
    WdfSpinLockAcquire(devCtx->EventLock);
    devCtx->WaitMask    = 0;
    devCtx->EventHistory = 0;
    WdfSpinLockRelease(devCtx->EventLock);
}

/* =========================================================================
 *  DeviceConfigureUsbDevice — create USB target device and select config
 * ========================================================================= */
NTSTATUS
DeviceConfigureUsbDevice(
    _In_ PDEVICE_CONTEXT DevCtx
    )
{
    NTSTATUS                            status;
    WDF_USB_DEVICE_CREATE_CONFIG        usbConfig;
    WDF_USB_DEVICE_SELECT_CONFIG_PARAMS configParams;

    PAGED_CODE();

    WDF_USB_DEVICE_CREATE_CONFIG_INIT(&usbConfig,
                                       USBD_CLIENT_CONTRACT_VERSION_602);

    status = WdfUsbTargetDeviceCreateWithParameters(
        DevCtx->Device,
        &usbConfig,
        WDF_NO_OBJECT_ATTRIBUTES,
        &DevCtx->UsbDevice
        );
    if (!NT_SUCCESS(status)) {
        return status;
    }

    /*
     * Select default configuration. Use MULTIPLE_INTERFACES to handle
     * CDC ACM devices that expose separate Communication and Data interfaces.
     */
    WDF_USB_DEVICE_SELECT_CONFIG_PARAMS_INIT_MULTIPLE_INTERFACES(
        &configParams,
        0,
        NULL
        );

    status = WdfUsbTargetDeviceSelectConfig(
        DevCtx->UsbDevice,
        WDF_NO_OBJECT_ATTRIBUTES,
        &configParams
        );

    return status;
}

/* =========================================================================
 *  DeviceConfigureUsbPipes — find and configure Bulk IN/OUT and Interrupt
 *  Equivalent to the original Data_GetDataConfig (section 9)
 * ========================================================================= */
NTSTATUS
DeviceConfigureUsbPipes(
    _In_ PDEVICE_CONTEXT DevCtx
    )
{
    NTSTATUS                    status = STATUS_SUCCESS;
    BYTE                        numInterfaces;
    BYTE                        interfaceIdx, pipeIdx;
    WDF_USB_PIPE_INFORMATION    pipeInfo;

    PAGED_CODE();

    DevCtx->BulkInPipe    = NULL;
    DevCtx->BulkOutPipe   = NULL;
    DevCtx->InterruptPipe = NULL;

    numInterfaces = WdfUsbTargetDeviceGetNumInterfaces(DevCtx->UsbDevice);

    for (interfaceIdx = 0; interfaceIdx < numInterfaces; interfaceIdx++) {
        WDFUSBINTERFACE usbInterface;
        BYTE            numPipes;

        usbInterface = WdfUsbTargetDeviceGetInterface(
            DevCtx->UsbDevice, interfaceIdx);
        numPipes = WdfUsbInterfaceGetNumConfiguredPipes(usbInterface);

        for (pipeIdx = 0; pipeIdx < numPipes; pipeIdx++) {
            WDFUSBPIPE pipe;

            WDF_USB_PIPE_INFORMATION_INIT(&pipeInfo);
            pipe = WdfUsbInterfaceGetConfiguredPipe(
                usbInterface, pipeIdx, &pipeInfo);

            if (pipeInfo.PipeType == WdfUsbPipeTypeBulk) {
                if (USB_ENDPOINT_DIRECTION_IN(pipeInfo.EndpointAddress)) {
                    if (DevCtx->BulkInPipe == NULL) {
                        DevCtx->BulkInPipe       = pipe;
                        DevCtx->BulkInMaxPacket  = pipeInfo.MaximumPacketSize;
                        DevCtx->UsbInterface     = usbInterface;
                    }
                } else {
                    if (DevCtx->BulkOutPipe == NULL) {
                        DevCtx->BulkOutPipe      = pipe;
                        DevCtx->BulkOutMaxPacket = pipeInfo.MaximumPacketSize;
                    }
                }
            } else if (pipeInfo.PipeType == WdfUsbPipeTypeInterrupt) {
                if (USB_ENDPOINT_DIRECTION_IN(pipeInfo.EndpointAddress)) {
                    if (DevCtx->InterruptPipe == NULL) {
                        DevCtx->InterruptPipe    = pipe;
                        DevCtx->InterfaceNumber  =
                            WdfUsbInterfaceGetInterfaceNumber(usbInterface);
                    }
                }
            }
        }
    }

    if (DevCtx->BulkInPipe == NULL || DevCtx->BulkOutPipe == NULL) {
        return STATUS_INSUFFICIENT_RESOURCES;
    }

    /*
     * If no interrupt pipe found, use the interface number from the
     * data interface (for devices using single-interface layout).
     */
    if (DevCtx->InterruptPipe == NULL && DevCtx->UsbInterface != NULL) {
        DevCtx->InterfaceNumber =
            WdfUsbInterfaceGetInterfaceNumber(DevCtx->UsbInterface);
    }

    /* Disable short-packet check for the bulk pipes */
    WdfUsbTargetPipeSetNoMaximumPacketSizeCheck(DevCtx->BulkInPipe);
    WdfUsbTargetPipeSetNoMaximumPacketSizeCheck(DevCtx->BulkOutPipe);

    /*
     * Configure KMDF continuous reader on the Bulk IN pipe.
     * This replaces the original driver's multiple outstanding IN URBs
     * with reordering (Data_StartInTrans / Data_InTransReorder).
     * KMDF handles serialised delivery and error recovery.
     */
    {
        WDF_USB_CONTINUOUS_READER_CONFIG readerConfig;

        WDF_USB_CONTINUOUS_READER_CONFIG_INIT(
            &readerConfig,
            EvtUsbBulkInReadComplete,
            DevCtx,
            MAX_TRANSFER_SIZE
            );
        readerConfig.NumPendingReads          = NUM_CONTINUOUS_READERS;
        readerConfig.EvtUsbTargetPipeReadersFailed = EvtUsbBulkInReadersFailed;

        status = WdfUsbTargetPipeConfigContinuousReader(
            DevCtx->BulkInPipe,
            &readerConfig
            );
        if (!NT_SUCCESS(status)) {
            return status;
        }
    }

    /*
     * Configure continuous reader on the Interrupt IN pipe (if present)
     * for CDC ACM serial state notifications.
     */
    if (DevCtx->InterruptPipe != NULL) {
        WDF_USB_CONTINUOUS_READER_CONFIG readerConfig;

        WDF_USB_CONTINUOUS_READER_CONFIG_INIT(
            &readerConfig,
            EvtUsbInterruptReadComplete,
            DevCtx,
            INTERRUPT_BUFFER_SIZE
            );
        readerConfig.NumPendingReads          = 1;
        readerConfig.EvtUsbTargetPipeReadersFailed =
            EvtUsbInterruptReadersFailed;

        status = WdfUsbTargetPipeConfigContinuousReader(
            DevCtx->InterruptPipe,
            &readerConfig
            );
        if (!NT_SUCCESS(status)) {
            /* Non-fatal: device can function without serial state notifications */
            DevCtx->InterruptPipe = NULL;
        }
    }

    /* Create ZLP work item for zero-length packet after aligned writes */
    {
        WDF_WORKITEM_CONFIG workConfig;
        WDF_OBJECT_ATTRIBUTES workAttrs;

        WDF_WORKITEM_CONFIG_INIT(&workConfig, EvtZlpWorkItem);
        workConfig.AutomaticSerialization = FALSE;

        WDF_OBJECT_ATTRIBUTES_INIT(&workAttrs);
        workAttrs.ParentObject = DevCtx->Device;

        status = WdfWorkItemCreate(&workConfig, &workAttrs, &DevCtx->ZlpWorkItem);
        if (!NT_SUCCESS(status)) {
            return status;
        }
    }

    return STATUS_SUCCESS;
}

/* =========================================================================
 *  DeviceCreateSymbolicLink — create \\DosDevices\\COMx symbolic link
 *  Equivalent to the original PnP_CreateSymbolicLink (section 5)
 * ========================================================================= */
NTSTATUS
DeviceCreateSymbolicLink(
    _In_ PDEVICE_CONTEXT DevCtx
    )
{
    NTSTATUS    status;
    WDFKEY      hKey = NULL;

    PAGED_CODE();

    /* Initialise PortName unicode string */
    DevCtx->PortName.Buffer        = DevCtx->PortNameBuf;
    DevCtx->PortName.MaximumLength = sizeof(DevCtx->PortNameBuf);
    DevCtx->PortName.Length        = 0;

    /* Read PortName from device hardware registry key (set by MsPorts.dll) */
    {
        DECLARE_CONST_UNICODE_STRING(portNameValueName, L"PortName");

        status = WdfDeviceOpenRegistryKey(
            DevCtx->Device,
            PLUGPLAY_REGKEY_DEVICE,
            KEY_READ,
            WDF_NO_OBJECT_ATTRIBUTES,
            &hKey
            );
        if (NT_SUCCESS(status)) {
            status = WdfRegistryQueryUnicodeString(
                hKey,
                &portNameValueName,
                NULL,
                &DevCtx->PortName
                );
            WdfRegistryClose(hKey);
            hKey = NULL;
        }
    }

    /* Fall back to a generated name if PortName not found */
    if (!NT_SUCCESS(status) || DevCtx->PortName.Length == 0) {
        status = RtlUnicodeStringPrintf(
            &DevCtx->PortName,
            L"COM%d",
            (int)(DevCtx->PortIndex + 10)
            );
        if (!NT_SUCCESS(status)) {
            return status;
        }
    }

    /* Build DOS device name: \\DosDevices\\COMx */
    DevCtx->DosName.Buffer        = DevCtx->DosNameBuf;
    DevCtx->DosName.MaximumLength = sizeof(DevCtx->DosNameBuf);
    DevCtx->DosName.Length        = 0;

    status = RtlUnicodeStringPrintf(
        &DevCtx->DosName,
        L"\\DosDevices\\%wZ",
        &DevCtx->PortName
        );
    if (!NT_SUCCESS(status)) {
        return status;
    }

    /* Create symbolic link: \\DosDevices\\COMx -> \\Device\\mtkcdcacmN */
    status = IoCreateSymbolicLink(&DevCtx->DosName, &DevCtx->DeviceName);
    if (!NT_SUCCESS(status)) {
        return status;
    }

    DevCtx->SymbolicLinkCreated = TRUE;

    /* Write SERIALCOMM registry entry so the system knows this COM port exists */
    status = RtlWriteRegistryValue(
        RTL_REGISTRY_DEVICEMAP,
        L"SERIALCOMM",
        DevCtx->DeviceName.Buffer,
        REG_SZ,
        DevCtx->PortName.Buffer,
        DevCtx->PortName.Length + sizeof(WCHAR)
        );

    if (NT_SUCCESS(status)) {
        DevCtx->SerialCommWritten = TRUE;
    }

    return STATUS_SUCCESS;
}

/* =========================================================================
 *  DeviceRemoveSymbolicLink — clean up COM port symbolic link
 *  Equivalent to the original PnP_RemoveSymbolicLinks
 * ========================================================================= */
VOID
DeviceRemoveSymbolicLink(
    _In_ PDEVICE_CONTEXT DevCtx
    )
{
    PAGED_CODE();

    if (DevCtx->SerialCommWritten) {
        RtlDeleteRegistryValue(
            RTL_REGISTRY_DEVICEMAP,
            L"SERIALCOMM",
            DevCtx->DeviceName.Buffer
            );
        DevCtx->SerialCommWritten = FALSE;
    }

    if (DevCtx->SymbolicLinkCreated) {
        IoDeleteSymbolicLink(&DevCtx->DosName);
        DevCtx->SymbolicLinkCreated = FALSE;
    }
}

/* =========================================================================
 *  DeviceReadRegistrySettings — read IdleTime, IdleEnable, etc.
 *  Equivalent to the original PWR_GetIdleInfo
 * ========================================================================= */
NTSTATUS
DeviceReadRegistrySettings(
    _In_ PDEVICE_CONTEXT DevCtx
    )
{
    NTSTATUS    status;
    WDFKEY      hKey = NULL;
    ULONG       value;

    PAGED_CODE();

    status = WdfDeviceOpenRegistryKey(
        DevCtx->Device,
        PLUGPLAY_REGKEY_DEVICE,
        KEY_READ,
        WDF_NO_OBJECT_ATTRIBUTES,
        &hKey
        );
    if (!NT_SUCCESS(status)) {
        return status;
    }

    {
        DECLARE_CONST_UNICODE_STRING(idleTimeName, L"IdleTime");
        if (NT_SUCCESS(WdfRegistryQueryULong(hKey, &idleTimeName, &value))) {
            DevCtx->IdleTimeSeconds = value;
        }
    }

    {
        DECLARE_CONST_UNICODE_STRING(idleEnableName, L"IdleEnable");
        if (NT_SUCCESS(WdfRegistryQueryULong(hKey, &idleEnableName, &value))) {
            DevCtx->IdleEnabled = (value != 0);
        }
    }

    {
        DECLARE_CONST_UNICODE_STRING(idleWWName, L"IdleWWBinded");
        if (NT_SUCCESS(WdfRegistryQueryULong(hKey, &idleWWName, &value))) {
            DevCtx->IdleWWBound = (value != 0);
        }
    }

    WdfRegistryClose(hKey);

    return STATUS_SUCCESS;
}
